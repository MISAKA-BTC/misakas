//! Accepted-prefix replay of assigned source openings and first receipt facts.
use super::*;
use crate::{
    palw_native_resource_v1::typed::BTreeNodeEnvelopeV1,
    palw_positive_court_v2::coordinator::native_body::{
        PositiveNativeBodyV2, PositiveNativeOperationV2, SOURCE_RESPONSE_BODY_OVERHEAD_V2,
    },
    palw_positive_source_v2::{RetainedSourceRequestV2, contains, interval_units, merge},
};
use misaka_palw_positive_contract::{
    assignment::{
        funding_relay::{FUNDING_RELAY_OPERATION_V2, FundingRelayRegistrationBodyV2, RelayFundingPurposeV2, RelayTargetV2},
        funding_relay_rows::{FundingRelayBindingV2, FundingRelayCustodyRowV2},
    },
    funding::{SourceFeeDeltaIntent, funding_scope},
    identity::{RequestId, principal},
    messages::{MinerDataStatus, SourceKind},
    rows::{FundingRow, MAX_NATIVE_KEY_BYTES, NativeHash64, NativeOutpoint, NativeRowValue, ServiceReceiptRow, SourceUnionRow},
};

/// Source fees only. Rebuilt from the literal first responses and their
/// verified UTXO payments. Other Case charges cannot free source capacity.
#[derive(Default)]
pub(super) struct CaseSourceFeesV2 {
    earned: u64,
    paid: u64,
}
impl CaseSourceFeesV2 {
    fn pending_with(&self, due: u64, locked: u64, budget: u64) -> Option<u64> {
        self.earned.checked_add(due).filter(|earned| *earned <= budget)?;
        self.earned.checked_sub(self.paid)?.checked_add(due).filter(|pending| *pending <= locked)
    }

    /// Before a partial payment the Case coin must cover both accrued fees
    /// and deliveries that have been promised but have no first receipt yet.
    /// Only the accrued, unpaid portion can appear as a payout.
    fn settlement_limit(&self, reserved: u64, locked: u64, budget: u64) -> Option<u64> {
        self.pending_with(reserved, locked, budget)?.checked_sub(reserved)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SourceDeliveryReserveV2 {
    units: u64,
    body: u64,
    work: u64,
    journal: u64,
}
impl SourceDeliveryReserveV2 {
    fn combine(self, other: Self, op: fn(u64, u64) -> Option<u64>) -> Option<Self> {
        Some(Self {
            units: op(self.units, other.units)?,
            body: op(self.body, other.body)?,
            work: op(self.work, other.work)?,
            journal: op(self.journal, other.journal)?,
        })
    }
    fn quote(
        body: u64,
        work: &crate::palw_positive_assignment_resources_v2::AssignmentWorkScheduleV2,
    ) -> Result<Self, PalwStateV2Error> {
        let journal =
            crate::palw_positive_assignment_case_material_v2::assigned_case_followup_journal_extent_v2(body).ok_or_else(capacity)?;
        let work = work.court_body_work(body).map_err(refuse)?;
        Ok(Self { units: 1, body, work, journal })
    }
}

pub(super) struct PreparedAssignedSourceEventV2 {
    pub(super) case: CaseRow,
    pub(super) union: SourceUnionRow,
    pub(super) receipt: Option<ServiceReceiptRow>,
}

#[derive(Default)]
pub(super) struct ReplayedCaseSourceUnionsV2 {
    ranges: [Vec<(u64, u64)>; 2],
    range_leases: [Option<NativeDemandLeaseV1>; 2],
    rows: [Option<SourceUnionRow>; 2],
    rounds: u64,
    unique_bytes: u64,
    pending_fee: u64,
    pending: SourceDeliveryReserveV2,
    used: SourceDeliveryReserveV2,
}
fn source_index(kind: SourceKind) -> usize {
    match kind {
        SourceKind::ArtifactInventory => 0,
        SourceKind::Input => 1,
    }
}
impl ReplayedCaseSourceUnionsV2 {
    fn miner_data(&self) -> MinerDataStatus {
        let mut rows = self.rows.iter().flatten().peekable();
        if rows.peek().is_none() {
            MinerDataStatus::NotRequested
        } else if rows.all(|row| row.requested_units == row.completed_units) {
            MinerDataStatus::Complete
        } else {
            MinerDataStatus::Pending
        }
    }
}

pub(super) struct CheckedAssignedSourceResponseV2 {
    fee: SourceFeeDeltaIntent,
    duty: AcceptedMaterial,
    data_bytes: u64,
    additional_work: u64,
    reserved_fee: u64,
    reserved_delivery: SourceDeliveryReserveV2,
}

impl PreparedAssignedSourceEventV2 {
    pub(super) fn check_committed(&self, state: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
        let matches = |body: NativeRowBody| -> Result<bool, PalwStateV2Error> { Ok(required(state, body.key())? == body) };
        if !matches(NativeRowBody::Case(self.case))? || !matches(NativeRowBody::SourceUnion(self.union))? {
            return Err(refuse("rooted source Case or union differs from the literal event"));
        }
        if let Some(receipt) = self.receipt
            && !matches(NativeRowBody::ServiceReceipt(receipt))?
        {
            return Err(refuse("rooted first receipt differs from the literal response"));
        }
        Ok(())
    }
}

fn source_material(origin: AcceptedOrigin) -> Result<AcceptedMaterial, PalwStateV2Error> {
    if origin.event_ordinal != 0 {
        return Err(refuse("source operation must be the literal transaction event"));
    }
    Ok(AcceptedMaterial { origin, digest: Digest(origin.carrier_transaction.0) })
}

pub(super) fn capacity() -> PalwStateV2Error {
    PalwStateV2Error::NativeContextUnavailable(crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::CapacityUnavailable)
}

impl NativeAssignedCourtV2 {
    pub(super) fn check_source_fee_projection_v2(&self, row: &CaseRow) -> Result<(), PalwStateV2Error> {
        if row.usage.fee != self.source_fees.earned {
            return Err(refuse("rooted source fee usage differs from literal first responses"));
        }
        Ok(())
    }

    pub(super) fn check_source_union_projection_v2(&self, state: &PalwChainStateV2, case: &CaseRow) -> Result<(), PalwStateV2Error> {
        if case.usage.rounds != self.source_unions.rounds
            || case.usage.unique_source_bytes != self.source_unions.unique_bytes
            || case.miner_data != self.source_unions.miner_data()
        {
            return Err(refuse("Case source usage differs from the accepted source union"));
        }
        for kind in [SourceKind::ArtifactInventory, SourceKind::Input] {
            let source = self.source.geometry.source_root(kind);
            let key = NativeRowKey::SourceUnion { case: case.case, kind, source };
            let saved = PositiveRowsV2::new(state).read(key).map_err(refuse)?.map(|row| row.body);
            if saved != self.source_unions.rows[source_index(kind)].map(NativeRowBody::SourceUnion) {
                return Err(refuse("rooted source union differs from the literal request history"));
            }
        }
        Ok(())
    }

    pub(super) fn prepare_source_request_union_v2(
        &mut self,
        material: &AssignmentAdmissionMaterialV2,
        prefix: &PalwChainStateV2,
        prior: &CaseRow,
        origin: AcceptedOrigin,
        body: &PositiveNativeBodyV2,
        body_heap: u64,
    ) -> Result<SourceUnionRow, PalwStateV2Error> {
        let PositiveNativeOperationV2::SourceRequest { request, .. } = &body.operation else {
            return Err(refuse("expected source request"));
        };
        if self.source_requests.contains_key(&request.id()) {
            return Err(refuse("duplicate source request in the Case journal"));
        }
        let kind = request.kind;
        let index = source_index(kind);
        let previous = &self.source_unions.ranges[index];
        let slots = (previous.len() as u64).checked_add(1).ok_or_else(capacity)?;
        let units = request.end.checked_sub(request.first).ok_or_else(capacity)?;
        let source_policy = &material.native_preimages.source_policy;
        let per_unit = u64::from(slots.ilog2()) + u64::from(source_policy.max_parameter_instances.max(1).ilog2()) + 8;
        let controls = slots.checked_mul(4).and_then(|n| n.checked_add(units.checked_mul(per_unit)?)).ok_or_else(capacity)?;
        let work = source_policy.work.court_operation_work(0, controls, 0, 0).map_err(refuse)?;
        self.remaining_work = self.remaining_work.checked_sub(work).ok_or_else(|| refuse("source union replay work"))?;
        let mut quote = TirNativeRequestsV1::new();
        quote.exact_v1::<(u64, u64)>(slots).map_err(PalwStateV2Error::NativeResource)?;
        let bytes = quote.bytes_v1();
        let heap = self.checker_heap.checked_add(bytes).ok_or_else(capacity)?;
        if heap.checked_add(self.candidate_heap).and_then(|n| n.checked_add(body_heap)).is_none_or(|n| n > self.available_heap) {
            return Err(capacity());
        }
        let range_lease = reserve_native_requests_v1(bytes).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity)?;
        let ranges = merge(previous, (request.first, request.end));
        if ranges.capacity() as u64 > slots
            || (ranges.len() as u64)
                .checked_add(self.source_unions.ranges[1 - index].len() as u64)
                .is_none_or(|n| n > source_policy.max_intervals)
        {
            return Err(refuse("source union exceeds original interval bounds"));
        }
        let requested_units = interval_units(&ranges).map_err(refuse)?;
        let fresh_units = requested_units
            .checked_sub(self.source_unions.rows[index].map_or(0, |row| row.requested_units))
            .ok_or_else(|| refuse("source union lost earlier requests"))?;
        let mut unique_bytes = self.source_unions.unique_bytes;
        let mut pending_fee = self.source_unions.pending_fee;
        let mut pending = self.source_unions.pending;
        let fees = &material.native_preimages.case_fees;
        for unit in request.first..request.end {
            if !contains(previous, unit) {
                let bound = self.source.geometry.unit_bound(kind, unit).map_err(refuse)?;
                unique_bytes = unique_bytes.checked_add(bound.max_data_bytes).ok_or_else(capacity)?;
                let fee = fees.source_unit_due(bound.max_unit_bytes).map_err(refuse)?;
                pending_fee = pending_fee.checked_add(fee).ok_or_else(capacity)?;
                let max_body = bound.max_unit_bytes.checked_add(SOURCE_RESPONSE_BODY_OVERHEAD_V2).ok_or_else(capacity)?;
                let delivery = SourceDeliveryReserveV2::quote(max_body, &source_policy.work)?;
                pending = pending.combine(delivery, u64::checked_add).ok_or_else(capacity)?;
            }
        }
        let NativeRowBody::Funding(funding) = required(prefix, NativeRowKey::Funding(prior.case))? else {
            return Err(refuse("missing source request funding"));
        };
        if funding.current_outpoint.is_none()
            || funding.consent.id().0 != prior.service.funding_consent
            || funding.basis.schedule != fees.id()
            || funding.consent.scope != funding_scope(self.court.result(), &prior.service, &funding.basis)
            || self.source_fees.pending_with(pending_fee, funding.fee.remaining, prior.service.budget.fee).is_none()
        {
            return Err(refuse("source request does not reserve all outstanding first-response fees"));
        }
        self.source_unions.pending_fee = pending_fee;
        self.source_unions.pending = pending;
        self.source_unions.unique_bytes = unique_bytes;
        self.source_unions.rounds = self.source_unions.rounds.checked_add(u64::from(fresh_units != 0)).ok_or_else(capacity)?;
        if self.source_unions.unique_bytes > prior.service.budget.unique_source_bytes
            || self.source_unions.rounds > prior.service.budget.rounds
        {
            return Err(refuse("source request exceeds original source budget"));
        }
        let union = SourceUnionRow {
            case: prior.case,
            kind,
            source: request.source,
            source_units: self.source.geometry.source_units(kind),
            requested_units,
            completed_units: self.source_unions.rows[index].map_or(0, |row| row.completed_units),
            union: source_material(origin)?,
        };
        self.source_unions.rows[index] = Some(union);
        self.source_unions.ranges[index] = ranges;
        let old = self.source_unions.range_leases[index].replace(range_lease);
        self.checker_heap = heap.checked_sub(old.as_ref().map_or(0, NativeDemandLeaseV1::memory_v1)).ok_or_else(capacity)?;
        drop(old);
        Ok(union)
    }

    fn prepare_source_response_union_v2(
        &mut self,
        response: &CheckedAssignedSourceResponseV2,
    ) -> Result<SourceUnionRow, PalwStateV2Error> {
        let NativeRowKey::ServiceReceipt { kind, source, unit, .. } = response.fee.receipt_key else {
            return Err(refuse("source response receipt key"));
        };
        let index = source_index(kind);
        let union = self.source_unions.rows[index].as_mut().ok_or_else(|| refuse("source response has no accepted union"))?;
        if union.source != source || !contains(&self.source_unions.ranges[index], unit) {
            return Err(refuse("source response is outside the accepted union"));
        }
        union.completed_units = union
            .completed_units
            .checked_add(1)
            .filter(|n| *n <= union.requested_units)
            .ok_or_else(|| refuse("source response exceeds its accepted union"))?;
        self.source_unions.pending_fee = self
            .source_unions
            .pending_fee
            .checked_sub(response.reserved_fee)
            .ok_or_else(|| refuse("source response has no reserved service fee"))?;
        self.source_unions.pending = self
            .source_unions
            .pending
            .combine(response.reserved_delivery, u64::checked_sub)
            .ok_or_else(|| refuse("source response has no reserved service capacity"))?;
        Ok(*union)
    }

    fn check_source_journal_reserve_v2(
        &self,
        material: &AssignmentAdmissionMaterialV2,
        prior: (u64, u64),
        next_bytes: u64,
    ) -> Result<(), PalwStateV2Error> {
        let pending = self.source_unions.pending;
        if prior
            .0
            .checked_add(next_bytes)
            .and_then(|n| n.checked_add(pending.journal))
            .and_then(|n| n.checked_add(self.source_court_reserve))
            .is_none_or(|n| n > material.positive_policy.max_case_journal_bytes)
            || prior
                .1
                .checked_add(1)
                .and_then(|n| n.checked_add(pending.units))
                .is_none_or(|n| n > u64::from(material.assignment_policy.max_history_frames))
        {
            return Err(refuse("event consumes reserved source or Court history"));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_assigned_source_event_v2(
        &mut self,
        material: &AssignmentAdmissionMaterialV2,
        prefix: &PalwChainStateV2,
        assignment: &AssignmentRow,
        prior: &CaseRow,
        origin: AcceptedOrigin,
        body: &PositiveNativeBodyV2,
        raw_bytes: u64,
        prior_journal: (u64, u64),
        body_heap: u64,
    ) -> Result<Option<PreparedAssignedSourceEventV2>, PalwStateV2Error> {
        let policy = &material.native_preimages.source_policy;
        if raw_bytes > policy.max_native_body_bytes {
            return Err(refuse("source carrier body exceeds original source policy"));
        }
        let cost = SourceDeliveryReserveV2::quote(raw_bytes, &policy.work)?;
        let (union, receipt) = match &body.operation {
            PositiveNativeOperationV2::SourceRequest { request, .. } => {
                if self.source_requests.contains_key(&request.id()) {
                    return Ok(None);
                }
                let union = self.prepare_source_request_union_v2(material, prefix, prior, origin, body, body_heap)?;
                self.remember_source_request_v2(material, origin, body, body_heap)?;
                (union, None)
            }
            PositiveNativeOperationV2::SourceResponse { .. } => {
                let Some(response) = self.check_source_response_v2(prefix, assignment, material, prior, origin, body, body_heap)?
                else {
                    return Ok(None);
                };
                let union = self.prepare_source_response_union_v2(&response)?;
                let NativeRowKey::ServiceReceipt { case, kind, source, unit } = response.fee.receipt_key else {
                    return Err(refuse("source response receipt key"));
                };
                let receipt = ServiceReceiptRow {
                    case,
                    kind,
                    source,
                    unit,
                    result: prior.result,
                    duty: response.duty,
                    response: response.fee.accepted_response,
                    provider: response.fee.provider,
                    bytes: response.data_bytes,
                    additional_work: response.additional_work,
                    settlement: None,
                };
                self.remember_source_response_v2(material, response, body_heap)?;
                (union, Some(receipt))
            }
            _ => return Err(refuse("source event must be a literal request or response")),
        };
        let used = self.source_unions.used.combine(cost, u64::checked_add).ok_or_else(capacity)?;
        let full_source = used.combine(self.source_unions.pending, u64::checked_add).ok_or_else(capacity)?;
        let court = self.source_court_reserve;
        let budget = &prior.service.budget;
        let pending = self.source_unions.pending;
        if full_source.units > policy.max_history_events
            || full_source.body > policy.max_history_bytes
            || (self.source_responses.len() as u64).checked_add(pending.units).is_none_or(|n| n > policy.max_responses)
            || full_source.body.checked_add(court).is_none_or(|n| n > budget.transmitted_bytes)
        {
            return Err(refuse("source event consumes reserved Case transport or history"));
        }
        self.check_source_journal_reserve_v2(material, prior_journal, cost.journal)?;
        let mut next = *prior;
        let usage = &mut next.usage;
        usage.rounds = self.source_unions.rounds;
        usage.unique_source_bytes = self.source_unions.unique_bytes;
        usage.fee = self.source_fees.earned;
        usage.transmitted_bytes = usage.transmitted_bytes.checked_add(cost.body).ok_or_else(capacity)?;
        usage.additional_work = usage.additional_work.checked_add(cost.work).ok_or_else(capacity)?;
        usage.retained_bytes = usage.retained_bytes.checked_add(cost.journal).ok_or_else(capacity)?;
        misaka_palw_positive_contract::reservations::RemainingCapacity {
            transmitted_bytes: pending.body,
            additional_work: pending.work,
            retained_bytes: pending.journal,
        }
        .check(usage, budget)
        .map_err(refuse)?;
        next.miner_data = self.source_unions.miner_data();
        next.last_event = origin;
        self.source_unions.used = used;
        Ok(Some(PreparedAssignedSourceEventV2 { case: next, union, receipt }))
    }

    fn admit_source_index_entry_v2<K, V>(&mut self, body_heap: u64) -> Result<(), PalwStateV2Error> {
        let mut quote = TirNativeRequestsV1::new();
        quote.exact_v1::<BTreeNodeEnvelopeV1<K, V>>(1).map_err(PalwStateV2Error::NativeResource)?;
        let request_heap = quote.bytes_v1();
        let heap = self.checker_heap.checked_add(request_heap).ok_or_else(capacity)?;
        if heap.checked_add(self.candidate_heap).and_then(|n| n.checked_add(body_heap)).is_none_or(|n| n > self.available_heap) {
            return Err(capacity());
        }
        self._lease.grow_memory_v1(request_heap).map_err(PalwStateV2Error::NativeResource)?;
        self.checker_heap = heap;
        Ok(())
    }

    pub(super) fn remember_source_response_v2(
        &mut self,
        material: &AssignmentAdmissionMaterialV2,
        checked: CheckedAssignedSourceResponseV2,
        body_heap: u64,
    ) -> Result<(), PalwStateV2Error> {
        let id = checked.fee.accepted_response.digest;
        if self.source_responses.contains_key(&id)
            || self.source_responses.len() as u64 >= material.native_preimages.source_policy.max_responses
        {
            return Err(refuse("duplicate or over-budget first source response"));
        }
        let earned = self.source_fees.earned.checked_add(checked.fee.amount).ok_or_else(|| refuse("source fee overflow"))?;
        self.admit_source_index_entry_v2::<Digest, CheckedAssignedSourceResponseV2>(body_heap)?;
        self.source_responses.insert(id, checked);
        self.source_fees.earned = earned;
        Ok(())
    }

    /// Call only after the literal request and its role were authenticated.
    pub(super) fn remember_source_request_v2(
        &mut self,
        material: &AssignmentAdmissionMaterialV2,
        origin: AcceptedOrigin,
        body: &PositiveNativeBodyV2,
        body_heap: u64,
    ) -> Result<(), PalwStateV2Error> {
        let PositiveNativeOperationV2::SourceRequest { request, requester, .. } = &body.operation else { return Ok(()) };
        let id = request.id();
        if self.source_requests.contains_key(&id) {
            return Err(refuse("duplicate source request changed the rooted journal"));
        }
        if self.source_requests.len() as u64 >= material.native_preimages.source_policy.max_requests {
            return Err(refuse("source request history exceeds original cap"));
        }
        let material = source_material(origin)?;
        self.admit_source_index_entry_v2::<RequestId, RetainedSourceRequestV2>(body_heap)?;
        self.source_requests.insert(id, RetainedSourceRequestV2 { request: *request, requester: *requester, material });
        Ok(())
    }

    /// Decode the real G14 opening using the earlier signed request. Bind the
    /// first receipt to the original worker duty and C funding scope.
    pub(super) fn check_source_response_v2(
        &mut self,
        state: &PalwChainStateV2,
        assignment: &AssignmentRow,
        material: &AssignmentAdmissionMaterialV2,
        prior: &CaseRow,
        origin: AcceptedOrigin,
        body: &PositiveNativeBodyV2,
        body_heap: u64,
    ) -> Result<Option<CheckedAssignedSourceResponseV2>, PalwStateV2Error> {
        let PositiveNativeOperationV2::SourceResponse { case, request, response } = &body.operation else {
            return Err(refuse("expected source response"));
        };
        let response_material = source_material(origin)?;
        let duty = self.source_requests.get(request).ok_or_else(|| refuse("source response has no earlier accepted request"))?;
        let result = self.court.result();
        let source = &material.native_preimages.source_policy;
        if *case != prior.case
            || *case != result.case_id()
            || prior.result != result.id()
            || prior.service.id() != result.service_contract
            || duty.request.contract != prior.service.id()
            || origin.accepted_daa >= prior.service.deadline.min(assignment.source_consent.service_before_daa)
            || (response.len() as u64)
                .checked_add(SOURCE_RESPONSE_BODY_OVERHEAD_V2)
                .is_none_or(|bytes| bytes > source.max_native_body_bytes)
        {
            return Err(refuse("source response differs from the accepted Case duty"));
        }

        // Charge the canonical range scan and up to 32 Merkle steps before
        // the G14 decoder allocates or hashes any opening.
        let bytes = response.len() as u64;
        let hash_work = source.work.court_body_work(bytes.checked_add(32 * 128).ok_or_else(capacity)?).map_err(refuse)?;
        let controls = duty.request.end.checked_sub(duty.request.first).and_then(|n| n.checked_add(32)).ok_or_else(capacity)?;
        let range_work = source.work.court_operation_work(0, controls, 0, 0).map_err(refuse)?;
        let work = hash_work.checked_add(range_work).ok_or_else(capacity)?;
        self.remaining_work = self.remaining_work.checked_sub(work).ok_or_else(|| refuse("source opening replay work"))?;
        let heap = bytes
            .checked_mul(3)
            .and_then(|n| n.checked_add((2 * size_of::<misaka_palw_positive_contract::rows::NativeRowValue>()) as u64))
            .ok_or_else(capacity)?;
        if self
            .available_heap
            .checked_sub(self.checker_heap)
            .and_then(|n| n.checked_sub(self.candidate_heap))
            .and_then(|n| n.checked_sub(body_heap))
            .is_none_or(|n| heap > n)
        {
            return Err(capacity());
        }
        let _owner = reserve_native_requests_v1(heap).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity)?;
        let (unit, opening) = self.source.geometry.decode_g14_response_v2(&duty.request, response).map_err(refuse)?;
        let (data_bytes, opening_bytes) = opening.sizes().map_err(refuse)?;
        let additional_work =
            source.work.court_body_work(bytes.checked_add(SOURCE_RESPONSE_BODY_OVERHEAD_V2).ok_or_else(capacity)?).map_err(refuse)?;
        let key = NativeRowKey::ServiceReceipt { case: *case, kind: duty.request.kind, source: duty.request.source, unit };
        let provider = principal(&assignment.worker_key);
        if let Some(value) = PositiveRowsV2::new(state).read(key).map_err(refuse)? {
            let NativeRowBody::ServiceReceipt(ServiceReceiptRow { result: received, bytes, provider: payee, .. }) = value.body else {
                return Err(refuse("source receipt key/value"));
            };
            if received != prior.result || bytes != data_bytes || payee != provider {
                return Err(refuse("source retransmission differs from first receipt"));
            }
            return Ok(None);
        }
        let NativeRowBody::Funding(funding) = required(state, NativeRowKey::Funding(*case))? else {
            return Err(refuse("missing source response funding"));
        };
        let schedule = &material.native_preimages.case_fees;
        let fee = schedule.source_unit_due(opening_bytes).map_err(refuse)?;
        let reserved = self.source.geometry.unit_bound(duty.request.kind, unit).map_err(refuse)?;
        let reserved_fee = schedule.source_unit_due(reserved.max_unit_bytes).map_err(refuse)?;
        let reserved_body = reserved.max_unit_bytes.checked_add(SOURCE_RESPONSE_BODY_OVERHEAD_V2).ok_or_else(capacity)?;
        let reserved_delivery = SourceDeliveryReserveV2::quote(reserved_body, &source.work)?;

        let total_new_due = self
            .source_unions
            .pending_fee
            .checked_sub(reserved_fee)
            .and_then(|pending| pending.checked_add(fee))
            .ok_or_else(|| refuse("source response fee was not reserved"))?;
        let schedule = schedule.id();
        let scope = funding_scope(result, &prior.service, &funding.basis);
        self.check_source_fee_projection_v2(prior)?;
        if funding.current_outpoint.is_none()
            || funding.consent.scope != scope
            || funding.consent.case != *case
            || funding.consent.id().0 != prior.service.funding_consent
            || funding.basis.schedule != schedule
            || fee > reserved_fee
            || self.source_fees.pending_with(total_new_due, funding.fee.remaining, prior.service.budget.fee).is_none()
        {
            return Err(refuse("source response fee exceeds the original C funding and service limits"));
        }
        Ok(Some(CheckedAssignedSourceResponseV2 {
            fee: SourceFeeDeltaIntent {
                receipt_key: key,
                result: prior.result,
                funding_scope: scope,
                schedule,
                provider,
                amount: fee,
                accepted_response: response_material,
            },
            duty: duty.material,
            data_bytes,
            additional_work,
            reserved_fee,
            reserved_delivery,
        }))
    }
}

struct CaseSourcePaymentV2 {
    funding: FundingRow,
    debit: NativeOutpoint,
    amount: u64,
    successor: Option<PendingCaseRelaySuccessorV2>,
}

struct PendingCaseRelaySuccessorV2 {
    custody: FundingRelayCustodyRowV2,
    predecessor: NativeOutpoint,
    original_funding: NativeOutpoint,
    original_contract: Digest,
    _lease: NativeDemandLeaseV1,
}

impl PendingCaseRelaySuccessorV2 {
    fn bind(mut self, origin: AcceptedOrigin, case_reservation: AcceptedMaterial) -> (FundingRelayCustodyRowV2, NativeDemandLeaseV1) {
        let digest = blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"misaka/positive/case-relay-rotation/v2/r1")
            .to_state()
            .update(&case_reservation.digest.0)
            .update(&self.custody.reservation.digest.0)
            .finalize();
        let reservation = AcceptedMaterial { origin, digest: Digest(*digest.as_array()) };
        self.custody.binding = Some(FundingRelayBindingV2 {
            origin,
            original_funding: self.original_funding,
            predecessor: Some(self.predecessor),
            original_contract: self.original_contract,
            reservation,
        });
        (self.custody, self._lease)
    }
}
use crate::palw_settlement_once_v1::{
    PALW_SETTLEMENT_ONCE_TABLE_V1, PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
    PalwSettlementOnceRowV1, palw_settlement_subject_v1,
};

fn source_fee_obligation_v2(assignment: AssignmentId, fee: &SourceFeeDeltaIntent) -> PalwSettlementObligationV1 {
    PalwSettlementObligationV1::with_party(
        PalwSettlementLegV1::AssignmentServiceFee,
        palw_settlement_subject_v1(b"assignment-service/v2", &[&assignment.0.0, &fee.accepted_response.digest.0]),
        fee.provider.0.0,
        0,
    )
}

fn is_case_source_payment_carrier_v2(tx: &crate::tx::Transaction) -> bool {
    tx.subnetwork_id == crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
        && tx.payload.get(..3) == Some(&[1, 0, 176])
        && tx.payload.get(7) == Some(&12)
}

impl NativeAssignedCourtV2 {
    fn qualify_case_relay_successor_v2(
        &mut self,
        state: &PalwChainStateV2,
        backing: &NativeCommittedAssignedCaseV2,
        funding: &FundingRow,
        (predecessor, continuation_amount): (NativeOutpoint, u64),
        (daa, accepting_block): (u64, Option<NativeHash64>),
        remaining_heap: u64,
    ) -> Result<Option<PendingCaseRelaySuccessorV2>, PalwStateV2Error> {
        use crate::palw_positive_funding_relay_v2::{FundingRelayBodyErrorV2, verify_funding_relay_package_signatures_v2};
        let Some((minimum_service_cutoff, remaining)) = self.relay_continuations else {
            return Ok(None);
        };
        // This payment only settles source receipts. Closing the last cursor
        // requires the separate terminal authority for all outstanding duties.
        let point = funding.current_outpoint.ok_or_else(|| refuse("source payment must retain the live Case relay cursor"))?;
        let remaining = remaining.checked_sub(1).ok_or_else(|| refuse("Case exhausted its original relay continuation reserve"))?;
        let mut quote = TirNativeRequestsV1::new();
        quote.exact_v1::<NativeRowValue>(3).map_err(PalwStateV2Error::NativeResource)?;
        quote.exact_v1::<FundingRelayRegistrationBodyV2>(1).map_err(PalwStateV2Error::NativeResource)?;
        quote.exact_v1::<u8>((2 * MAX_NATIVE_KEY_BYTES) as u64).map_err(PalwStateV2Error::NativeResource)?;
        let scratch = quote.bytes_v1();
        let signature_heap = remaining_heap.checked_sub(scratch).ok_or_else(capacity)?;
        let _lease = reserve_native_requests_v1(scratch).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity)?;
        let rows = PositiveRowsV2::new(state);
        let purpose = RelayFundingPurposeV2::CaseC;
        let Some(NativeRowBody::FundingRelayAuthorization(auth)) =
            rows.read(NativeRowKey::FundingRelayAuthorization { funding: point, purpose }).map_err(refuse)?.map(|row| row.body)
        else {
            return Err(refuse("Case continuation needs an accepted successor relay"));
        };
        let NativeRowBody::FundingRelayCustody(custody) =
            required(state, NativeRowKey::FundingRelayCustody { funding: point, purpose })?
        else {
            return Err(missing());
        };
        let assignment = backing.assignment();
        let original_contract = funding.consent.id().0;
        let target = RelayTargetV2::Continuation {
            original_funding: funding.original_outpoint,
            predecessor,
            parent_spend: point.transaction,
            original_contract,
        };
        custody.check_pair(&auth).map_err(refuse)?;
        if custody.binding.is_some()
            || auth.terms.target != target
            || auth.terms.assignment != assignment.id()
            || auth.terms.contract_network != assignment.bundle.terms.network
            || auth.terms.contract_ruleset != assignment.bundle.terms.ruleset
            || auth.terms.contract_anchor != assignment.bundle.terms.branch_anchor
            || auth.terms.payer_bond != assignment.bundle.terms.poster
            || auth.terms.payer != funding.consent.payer
            || auth.terms.refund_payload != funding.consent.refund_payload
            || auth.payer_key != funding.payer_key
            || auth.terms.expected_amount != continuation_amount
            || auth.origin.accepted_daa > daa
            || accepting_block == Some(auth.origin.accepting_block)
            || daa >= auth.terms.funding_admission_before_daa
            || custody
                .retention_until_daa
                .checked_sub(auth.policy.rollback_margin_daa)
                .is_none_or(|cutoff| cutoff < minimum_service_cutoff)
        {
            return Err(refuse("successor relay differs from the literal Case continuation or custody horizon"));
        }
        let body = FundingRelayRegistrationBodyV2 {
            version: misaka_palw_positive_contract::CONTRACT_VERSION,
            revision: misaka_palw_positive_contract::CONTRACT_REVISION,
            operation: FUNDING_RELAY_OPERATION_V2,
            terms: auth.terms,
            purpose_signature: auth.purpose_signature,
            relay_signature: custody.relay_signature,
        };
        let schedule = &backing.material().native_preimages.source_policy.work;
        let row_work = schedule.court_body_work(scratch).map_err(refuse)?;
        let signature_work = schedule.court_operation_work(2, 2, 0, 0).map_err(refuse)?;
        self.remaining_work = self
            .remaining_work
            .checked_sub(row_work)
            .and_then(|n| n.checked_sub(signature_work))
            .ok_or_else(|| refuse("Case successor relay replay work cap"))?;
        verify_funding_relay_package_signatures_v2(&body, &auth.payer_key, signature_heap.min(auth.quote.usage.heap_bytes)).map_err(
            |why| match why {
                FundingRelayBodyErrorV2::Native(why) => PalwStateV2Error::NativeResource(why),
                FundingRelayBodyErrorV2::Contract(why) => refuse(why),
            },
        )?;
        self.relay_continuations = Some((minimum_service_cutoff, remaining));
        Ok(Some(PendingCaseRelaySuccessorV2 {
            custody,
            predecessor,
            original_funding: funding.original_outpoint,
            original_contract,
            _lease,
        }))
    }

    /// Preflight op12 against the unique first receipts and actual current
    /// input. The enclosing writer owns the spend, updates and journal.
    fn qualify_case_source_payment_v2(
        &mut self,
        state: &PalwChainStateV2,
        backing: &NativeCommittedAssignedCaseV2,
        prior: &CaseRow,
        carriage: (&crate::tx::Transaction, &[crate::tx::UtxoEntry], u64),
        assignment: AssignmentId,
        receipts: &[AcceptedMaterial],
        (decode_heap, clock): (u64, (u64, Option<NativeHash64>)),
    ) -> Result<CaseSourcePaymentV2, PalwStateV2Error> {
        use crate::{
            mldsa87_primitives::p2pkh_mldsa87_payload,
            palw_positive_assignment_funds_v2::{AssignmentLegDebitV2, debit_assignment_leg_v2, native_assignment_outpoint_v2},
        };
        let (tx, entries, carrier_fee) = carriage;
        if backing.assignment().id() != assignment
            || receipts.is_empty()
            || receipts.len() as u64 > backing.material().native_preimages.source_policy.max_responses
        {
            return Err(refuse("source settlement has the wrong Assignment or receipt extent"));
        }
        let NativeRowBody::Funding(mut funding) = required(state, NativeRowKey::Funding(prior.case))? else {
            return Err(refuse("missing Case funding for settlement"));
        };
        self.check_source_fee_projection_v2(prior)?;
        let payable = self
            .source_fees
            .settlement_limit(self.source_unions.pending_fee, funding.fee.remaining, prior.service.budget.fee)
            .ok_or_else(|| refuse("unpaid and reserved source fees exceed Case funding"))?;
        let debit = funding.current_outpoint.ok_or_else(missing)?;
        let point = native_assignment_outpoint_v2(debit);
        let controls = (receipts.len() as u64)
            .checked_mul(4)
            .and_then(|n| n.checked_add(entries.len() as u64))
            .and_then(|n| n.checked_add(tx.outputs.len() as u64))
            .ok_or_else(capacity)?;
        let work = backing.material().native_preimages.source_policy.work.court_operation_work(0, controls, 0, 0).map_err(refuse)?;
        self.remaining_work =
            self.remaining_work.checked_sub(work).ok_or_else(|| refuse("source settlement exceeds the remaining Case work"))?;
        let locked = [funding.fee.remaining, funding.capital.remaining, funding.surplus.remaining]
            .into_iter()
            .try_fold(0u64, u64::checked_add)
            .ok_or_else(|| refuse("Case lock overflow"))?;
        let payer_script =
            |script| p2pkh_mldsa87_payload(script).map(|p| Digest(p.as_bytes())) == Some(funding.consent.refund_payload);
        if tx.inputs.len() != entries.len()
            || tx.inputs.first().is_none_or(|input| input.previous_outpoint != point)
            || tx.inputs.iter().skip(1).any(|input| input.previous_outpoint == point)
            || entries
                .first()
                .is_none_or(|entry| entry.is_coinbase || entry.amount != locked || !payer_script(&entry.script_public_key))
        {
            return Err(refuse("settlement must spend the exact current Case UTXO"));
        }

        let worker = &backing.assignment().worker_key;
        let payee = kaspa_hashes::blake2b_512_address_payload(worker);
        let provider = principal(worker);
        let route = state.kernel_route_state_v1().ok_or_else(missing)?;
        let mut quote = TirNativeRequestsV1::new();
        quote.exact_v1::<BTreeNodeEnvelopeV1<Digest, ()>>(receipts.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        quote.exact_v1::<u8>(64).map_err(PalwStateV2Error::NativeResource)?;
        let bytes = quote.bytes_v1();
        let available_heap = self
            .available_heap
            .checked_sub(self.checker_heap)
            .and_then(|n| n.checked_sub(self.candidate_heap))
            .and_then(|n| n.checked_sub(decode_heap))
            .ok_or_else(capacity)?;
        if bytes > available_heap {
            return Err(capacity());
        }
        let _scratch = reserve_native_requests_v1(bytes).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut total = 0u64;
        for (index, supplied) in receipts.iter().enumerate() {
            let checked =
                self.source_responses.get(&supplied.digest).ok_or_else(|| refuse("settlement receipt absent from Case replay"))?;
            let fee = &checked.fee;
            if fee.accepted_response != *supplied
                || fee.result != prior.result
                || fee.funding_scope != funding.consent.scope
                || fee.schedule != funding.basis.schedule
                || fee.provider != provider
                || !seen.insert(supplied.digest)
            {
                return Err(refuse("settlement receipt differs from its unique recovered duty"));
            }
            let NativeRowBody::ServiceReceipt(rooted) = required(state, fee.receipt_key)? else {
                return Err(refuse("settlement receipt is missing"));
            };
            let obligation = source_fee_obligation_v2(assignment, fee);
            if rooted.settlement.is_some()
                || rooted.response != *supplied
                || rooted.result != fee.result
                || rooted.provider != provider
                || rooted.duty != checked.duty
                || rooted.bytes != checked.data_bytes
                || rooted.additional_work != checked.additional_work
                || route.aux.contains_key(&(PALW_SETTLEMENT_ONCE_TABLE_V1, obligation.id_v1().to_vec()))
                || tx
                    .outputs
                    .get(index)
                    .is_none_or(|out| out.value != fee.amount || p2pkh_mldsa87_payload(&out.script_public_key) != Some(payee))
            {
                return Err(refuse("source receipt already paid or actual payment output differs"));
            }
            total = total
                .checked_add(fee.amount)
                .filter(|n| *n <= payable)
                .ok_or_else(|| refuse("source settlement exceeds the unpaid first-response total"))?;
        }

        let continuation = locked.checked_sub(total).ok_or_else(|| refuse("source settlement conservation"))?;
        if continuation != 0
            && tx.outputs.get(receipts.len()).is_none_or(|out| out.value != continuation || !payer_script(&out.script_public_key))
        {
            return Err(refuse("source settlement must preserve the remaining Case lock"));
        }
        let paid_outputs = receipts.len() + usize::from(continuation != 0);
        let unrelated_in = entries.iter().skip(1).map(|entry| entry.amount).try_fold(0u64, u64::checked_add);
        let unrelated_out = tx.outputs.iter().skip(paid_outputs).map(|out| out.value).try_fold(0u64, u64::checked_add);
        if unrelated_in.zip(unrelated_out).and_then(|(inputs, outputs)| inputs.checked_sub(outputs)) != Some(carrier_fee) {
            return Err(refuse("Case funds cannot sponsor unrelated outputs or carrier fees"));
        }
        funding.fee =
            debit_assignment_leg_v2(funding.fee, AssignmentLegDebitV2 { paid: total, ..Default::default() }, 0).map_err(refuse)?;
        funding.current_outpoint = if continuation == 0 {
            None
        } else {
            Some(NativeOutpoint {
                transaction: NativeHash64(tx.id().as_bytes()),
                index: u32::try_from(receipts.len()).map_err(refuse)?,
            })
        };
        drop(seen);
        drop(_scratch);
        let successor =
            self.qualify_case_relay_successor_v2(state, backing, &funding, (debit, continuation), clock, available_heap)?;
        Ok(CaseSourcePaymentV2 { funding, debit, amount: total, successor })
    }

    fn qualify_literal_case_source_payment_v2(
        &mut self,
        backing: &NativeCommittedAssignedCaseV2,
        prefix: &PalwChainStateV2,
        prior: &CaseRow,
        carriage: (&crate::tx::Transaction, &[crate::tx::UtxoEntry], u64),
        clock: (u64, Option<NativeHash64>),
    ) -> Result<(CaseSourcePaymentV2, Vec<AcceptedMaterial>, NativeDemandLeaseV1), PalwStateV2Error> {
        use crate::palw_positive_court_v2::coordinator::assignment_body::{
            PositiveAssignmentNativeOperationV2, decode_assignment_body_v2,
        };
        let original = backing.material();
        let rules = NativeAssignmentRulesV2::from_original_material_data_v2(backing.assignment(), original).map_err(refuse)?;
        let tx = carriage.0;
        if !is_case_source_payment_carrier_v2(tx) || !tx.id_is_current() {
            return Err(refuse("source settlement carrier differs"));
        }
        let raw = &tx.payload[3..];
        let quote = original.native_preimages.source_policy.work.court_body_work(raw.len() as u64).map_err(refuse)?;
        self.remaining_work = self.remaining_work.checked_sub(quote).ok_or_else(|| refuse("source payment replay codec work"))?;
        let limits = rules.body_limits().map_err(refuse)?;
        let summary =
            crate::palw_positive_court_v2::coordinator::assignment_body::scan_assignment_body_v2(raw, limits).map_err(refuse)?;
        if self
            .available_heap
            .checked_sub(self.checker_heap)
            .and_then(|n| n.checked_sub(self.candidate_heap))
            .is_none_or(|n| summary.heap > n)
        {
            return Err(capacity());
        }
        let decode_lease = reserve_native_requests_v1(summary.heap).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity)?;
        let body = decode_assignment_body_v2(raw, limits).map_err(refuse)?;
        let PositiveAssignmentNativeOperationV2::SettleAssignment { assignment, case: Some(case), receipts } = body.operation else {
            return Err(refuse("Case settlement journal requires the literal op12"));
        };
        if case != prior.case {
            return Err(refuse("settlement journal names a different Case"));
        }
        let payment =
            self.qualify_case_source_payment_v2(prefix, backing, prior, carriage, assignment, &receipts, (summary.heap, clock))?;
        Ok((payment, receipts, decode_lease))
    }

    fn check_case_payment_journal_reserve_v2(
        &self,
        backing: &NativeCommittedAssignedCaseV2,
        tx: &crate::tx::Transaction,
        prior: (u64, u64),
    ) -> Result<(), PalwStateV2Error> {
        let payload = borsh::object_length(tx).map_err(refuse)? as u64;
        let bytes = crate::palw_positive_assignment_case_material_v2::assigned_case_followup_journal_extent_v2(payload)
            .ok_or_else(capacity)?;
        self.check_source_journal_reserve_v2(backing.material(), prior, bytes)
    }

    pub(super) fn replay_case_source_payment_v2(
        &mut self,
        backing: &NativeCommittedAssignedCaseV2,
        prefix: &PalwChainStateV2,
        after: &PalwChainStateV2,
        prior: &CaseRow,
        carriage: &NativeAcceptedCaseCarriageV2,
        prior_journal: (u64, u64),
    ) -> Result<(), PalwStateV2Error> {
        self.check_case_payment_journal_reserve_v2(backing, carriage.transaction(), prior_journal)?;
        let (mut payment, receipts, _decode) = self.qualify_literal_case_source_payment_v2(
            backing,
            prefix,
            prior,
            (carriage.transaction(), carriage.entries(), carriage.fee()),
            (carriage.origin().accepted_daa, Some(carriage.origin().accepting_block)),
        )?;
        if let Some(successor) = payment.successor.take() {
            let (expected, _lease) = successor.bind(carriage.origin(), prior.reservation);
            let key = NativeRowKey::FundingRelayCustody { funding: expected.funding_outpoint, purpose: expected.purpose };
            if required(after, key)? != NativeRowBody::FundingRelayCustody(expected) {
                return Err(refuse("Case successor relay was not bound atomically with the historical payment"));
            }
        }
        let case = prior.case;
        let assignment = backing.assignment().id();
        let material = source_material(carriage.origin())?;
        payment.funding.last_settlement = Some(material);
        let mut expected_case = *prior;
        expected_case.last_event = carriage.origin();
        if required(after, NativeRowKey::Funding(case))? != NativeRowBody::Funding(payment.funding)
            || required(after, NativeRowKey::Case(case))? != NativeRowBody::Case(expected_case)
        {
            return Err(refuse("replayed source payment differs from the committed Case or funding rows"));
        }
        let route = after.kernel_route_state_v1().ok_or_else(missing)?;
        for supplied in &receipts {
            let checked = self.source_responses.get(&supplied.digest).ok_or_else(missing)?;
            let fee = &checked.fee;
            let NativeRowBody::ServiceReceipt(mut expected) = required(prefix, fee.receipt_key)? else {
                return Err(refuse("missing pre-payment source receipt"));
            };
            expected.settlement = Some(material);
            let obligation = source_fee_obligation_v2(assignment, fee);
            let id = obligation.id_v1();
            let recorded = route.aux.get(&(PALW_SETTLEMENT_ONCE_TABLE_V1, id.to_vec())).ok_or_else(missing)?;
            let marker: PalwSettlementOnceRowV1 = borsh::from_slice(recorded).map_err(refuse)?;
            marker.validate_v1(&id).map_err(refuse)?;
            if required(after, fee.receipt_key)? != NativeRowBody::ServiceReceipt(expected)
                || marker.obligation != obligation
                || marker.consumer != PalwSettlementConsumerV1::AssignmentNativeUtxo
                || marker.amount != fee.amount
                || marker.settled_daa != carriage.origin().accepted_daa
                || marker.debit
                    != (PalwSettlementDebitV1::NativeUtxo {
                        outpoint: crate::palw_positive_assignment_funds_v2::native_assignment_outpoint_v2(payment.debit),
                        amount: fee.amount,
                    })
            {
                return Err(refuse("source settlement receipt or once marker differs from replay"));
            }
        }
        self.source_fees.paid = self
            .source_fees
            .paid
            .checked_add(payment.amount)
            .ok_or_else(|| refuse("source settlement cumulative payment overflow"))?;
        Ok(())
    }
}

impl TransitionBuilder<'_> {
    pub(in crate::palw_state_v2) fn apply_case_source_settlement_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
        case: CaseId,
    ) -> Result<(), PalwStateV2Error> {
        use super::super::palw_settlement_once_fold_v1::PalwSettleOnceV1;
        if !self.params.settlement_once_active_at(ctx.daa_score) {
            return Err(refuse("actual settlement-once owner is dormant"));
        }
        let (backing, mut machine, mut prior) = self.prepare_case_subject_v2(action, case, None)?;
        let prior_journal =
            backing.records().iter().map(|record| record.raw().len() as u64).try_fold(0u64, u64::checked_add).ok_or_else(capacity)?;
        machine.check_case_payment_journal_reserve_v2(
            backing,
            action.transaction(),
            (prior_journal, backing.records().len() as u64),
        )?;
        let origin = action.origin();
        let (mut payment, receipts, _decode) = machine.qualify_literal_case_source_payment_v2(
            backing,
            &self.state,
            &prior,
            (action.transaction(), action.entries(), action.fee()),
            (origin.accepted_daa, Some(origin.accepting_block)),
        )?;
        let assignment = backing.assignment().id();
        let material = source_material(origin)?;
        payment.funding.last_settlement = Some(material);
        if let Some(point) = payment.funding.current_outpoint {
            self.require_assignment_output_unlocked_v2(point)?;
        }
        // The active raw op12 owns the enclosing checkpoint, including every
        // once marker, source receipt, funding row and retained journal entry.
        for supplied in &receipts {
            let checked = machine.source_responses.get(&supplied.digest).ok_or_else(missing)?;
            let fee = &checked.fee;
            let obligation = source_fee_obligation_v2(assignment, fee);
            if self.settle_once_v1(
                ctx.daa_score,
                obligation,
                PalwSettlementConsumerV1::AssignmentNativeUtxo,
                fee.amount,
                PalwSettlementDebitV1::NativeUtxo {
                    outpoint: crate::palw_positive_assignment_funds_v2::native_assignment_outpoint_v2(payment.debit),
                    amount: fee.amount,
                },
            )? != PalwSettleOnceV1::First
            {
                return Err(refuse("source fee settlement is dormant, refused or already consumed"));
            }
            let NativeRowBody::ServiceReceipt(mut receipt) = required(&self.state, fee.receipt_key)? else {
                return Err(refuse("source payment lost its actual receipt"));
            };
            receipt.settlement = Some(material);
            self.write_assignment_aux_v2(NativeRowBody::ServiceReceipt(receipt))?;
        }
        prior.last_event = origin;
        if let Some(successor) = payment.successor.take() {
            let (custody, _lease) = successor.bind(origin, prior.reservation);
            self.write_assignment_aux_v2(NativeRowBody::FundingRelayCustody(custody))?;
        }
        self.write_assignment_aux_v2(NativeRowBody::Funding(payment.funding))?;
        self.write_assignment_aux_v2(NativeRowBody::Case(prior))?;
        self.write_assigned_case_followup_journal_v2(backing)
    }
}

/// The exact Case settlement validated from rooted history and populated
/// inputs. The ordinary script check and the accepted-event fold still run.
#[derive(Clone, Copy, Debug)]
pub struct QualifiedCaseSourceSpendV2 {
    transaction: crate::TransactionId,
    carrier_hash: crate::TransactionHash,
    inputs: Digest,
    debit: crate::tx::TransactionOutpoint,
    continuation: Option<crate::tx::TransactionOutpoint>,
    successor_relay: Option<AcceptedMaterial>,
}
impl QualifiedCaseSourceSpendV2 {
    pub fn matches_v2(&self, transaction: &crate::tx::Transaction, entries: &[crate::tx::UtxoEntry]) -> bool {
        transaction.id() == self.transaction
            && transaction.id_is_current()
            && crate::hashing::tx::hash(transaction) == self.carrier_hash
            && crate::palw_positive_assignment_case_material_v2::assigned_case_inputs_id_v2(transaction.id(), entries)
                == Ok(self.inputs)
    }
    pub fn debit_v2(&self) -> crate::tx::TransactionOutpoint {
        self.debit
    }
    pub fn continuation_v2(&self) -> Option<crate::tx::TransactionOutpoint> {
        self.continuation
    }
    /// The pending registration selected by the literal successor output.
    /// Consensus must authenticate its accepted carrier before allowing the
    /// locked input to be spent.
    pub fn successor_relay_v2(&self) -> Option<AcceptedMaterial> {
        self.successor_relay
    }
}

pub fn qualify_case_source_spend_v2(
    state: &PalwChainStateV2,
    parent: BlockHash,
    backing: &NativeCommittedAssignedCaseV2,
    transaction: &crate::tx::Transaction,
    entries: &[crate::tx::UtxoEntry],
    daa: u64,
) -> Result<QualifiedCaseSourceSpendV2, PalwStateV2Error> {
    use crate::palw_positive_assignment_funds_v2::native_assignment_outpoint_v2;
    if !is_case_source_payment_carrier_v2(transaction) || !transaction.id_is_current() {
        return Err(refuse("Case spend differs from the actual selected-parent carrier"));
    }
    super::super::positive_assignment_materials::quote_assigned_case_payment_journal_v2(backing, transaction)?;
    let (mut machine, prior) = recover_case_subject_v2(state, backing, parent, backing.pin().case, None)?;
    let prior_journal =
        backing.records().iter().map(|record| record.raw().len() as u64).try_fold(0u64, u64::checked_add).ok_or_else(capacity)?;
    machine.check_case_payment_journal_reserve_v2(backing, transaction, (prior_journal, backing.records().len() as u64))?;
    let fee = entries
        .iter()
        .map(|entry| entry.amount)
        .try_fold(0u64, u64::checked_add)
        .zip(transaction.outputs.iter().map(|output| output.value).try_fold(0u64, u64::checked_add))
        .and_then(|(inputs, outputs)| inputs.checked_sub(outputs))
        .ok_or_else(|| refuse("Case candidate fee arithmetic"))?;
    let (payment, _, _decode) =
        machine.qualify_literal_case_source_payment_v2(backing, state, &prior, (transaction, entries, fee), (daa, None))?;
    Ok(QualifiedCaseSourceSpendV2 {
        transaction: transaction.id(),
        carrier_hash: crate::hashing::tx::hash(transaction),
        inputs: crate::palw_positive_assignment_case_material_v2::assigned_case_inputs_id_v2(transaction.id(), entries)
            .map_err(refuse)?,
        debit: native_assignment_outpoint_v2(payment.debit),
        continuation: payment.funding.current_outpoint.map(native_assignment_outpoint_v2),
        successor_relay: payment.successor.as_ref().map(|successor| successor.custody.reservation),
    })
}

#[cfg(test)]
mod source_fee_tests {
    use super::{CaseSourceFeesV2, SourceDeliveryReserveV2};

    #[test]
    fn first_delivery_releases_only_its_own_capacity_in_every_dimension() {
        let first = SourceDeliveryReserveV2 { units: 1, body: 15, work: 70, journal: 1120 };
        let second = SourceDeliveryReserveV2 { units: 1, body: 20, work: 95, journal: 1125 };
        let pending = first.combine(second, u64::checked_add).unwrap();
        let remaining = pending.combine(first, u64::checked_sub).unwrap();
        assert_eq!((remaining.units, remaining.body, remaining.work, remaining.journal), (1, 20, 95, 1125));
        let oversized = SourceDeliveryReserveV2 { journal: 1126, ..second };
        assert!(remaining.combine(oversized, u64::checked_sub).is_none());
    }

    #[test]
    fn partial_settlement_keeps_the_future_delivery_reserve() {
        let fees = CaseSourceFeesV2 { earned: 90, paid: 40 };
        assert_eq!(fees.settlement_limit(10, 60, 100), Some(50));
        assert_eq!(fees.settlement_limit(10, 59, 100), None);
        assert_eq!(fees.settlement_limit(10, 60, 99), None);
        let after = CaseSourceFeesV2 { paid: 90, ..fees };
        assert_eq!(after.settlement_limit(10, 10, 100), Some(0));
        assert_eq!(after.settlement_limit(10, 9, 100), None);
    }

    #[test]
    fn case_funding_covers_the_entire_unpaid_source_balance() {
        let mut fees = CaseSourceFeesV2 { earned: 90, paid: 0 };
        assert_eq!(fees.pending_with(10, 100, 110), Some(100));
        assert_eq!(fees.pending_with(11, 100, 110), None);
        fees.paid = 40;
        assert_eq!(fees.pending_with(10, 60, 100), Some(60));
        assert_eq!(fees.pending_with(10, 60, 99), None);
        assert_eq!(fees.pending_with(0, 49, 100), None);
        fees.paid = 91;
        assert_eq!(fees.pending_with(0, 100, 100), None);
        assert_eq!(fees.pending_with(1, 100, 100), None);
        fees.earned = u64::MAX;
        assert_eq!(fees.pending_with(1, u64::MAX, u64::MAX), None);
    }
}
