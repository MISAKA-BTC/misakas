//! Accepted-prefix replay of assigned source openings and first receipt facts.
use super::*;
use crate::{
    palw_native_resource_v1::typed::BTreeNodeEnvelopeV1,
    palw_positive_court_v2::coordinator::native_body::{
        PositiveNativeBodyV2, PositiveNativeOperationV2, SOURCE_RESPONSE_BODY_OVERHEAD_V2,
    },
    palw_positive_source_v2::RetainedSourceRequestV2,
};
use misaka_palw_positive_contract::{
    funding::{SourceFeeDeltaIntent, funding_scope},
    identity::{RequestId, principal},
    rows::ServiceReceiptRow,
};

pub(super) struct CheckedAssignedSourceResponseV2 {
    fee: SourceFeeDeltaIntent,
    duty: AcceptedMaterial,
    data_bytes: u64,
}

impl CheckedAssignedSourceResponseV2 {
    pub(super) fn check_committed(self, state: &PalwChainStateV2, prior: &CaseRow) -> Result<(), PalwStateV2Error> {
        let NativeRowBody::ServiceReceipt(receipt) = required(state, self.fee.receipt_key)? else {
            return Err(refuse("missing first native source receipt"));
        };
        let NativeRowBody::Case(after) = required(state, NativeRowKey::Case(prior.case))? else {
            return Err(refuse("source response changed Case identity"));
        };
        if receipt.result != self.fee.result
            || receipt.duty != self.duty
            || receipt.response != self.fee.accepted_response
            || receipt.provider != self.fee.provider
            || receipt.bytes != self.data_bytes
            || prior.usage.fee.checked_add(self.fee.amount) != Some(after.usage.fee)
            || after.result != prior.result
            || after.service != prior.service
            || after.last_event != self.fee.accepted_response.origin
        {
            return Err(refuse("rooted source receipt or fee differs from the actual response"));
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

fn capacity() -> PalwStateV2Error {
    PalwStateV2Error::NativeContextUnavailable(crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::CapacityUnavailable)
}

impl NativeAssignedCourtV2 {
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
        let mut quote = TirNativeRequestsV1::new();
        quote.exact_v1::<BTreeNodeEnvelopeV1<RequestId, RetainedSourceRequestV2>>(1).map_err(PalwStateV2Error::NativeResource)?;
        let request_heap = quote.bytes_v1();
        let heap = self.checker_heap.checked_add(request_heap).ok_or_else(capacity)?;
        if heap.checked_add(self.candidate_heap).and_then(|n| n.checked_add(body_heap)).is_none_or(|n| n > self.available_heap) {
            return Err(capacity());
        }
        let material = source_material(origin)?;
        self._lease.grow_memory_v1(request_heap).map_err(PalwStateV2Error::NativeResource)?;
        self.checker_heap = heap;
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
        let schedule = schedule.id();
        let scope = funding_scope(result, &prior.service, &funding.basis);
        if funding.current_outpoint.is_none()
            || funding.consent.scope != scope
            || funding.consent.case != *case
            || funding.consent.id().0 != prior.service.funding_consent
            || funding.basis.schedule != schedule
            || funding.fee.remaining < fee
            || prior.usage.fee.checked_add(fee).is_none_or(|n| n > prior.service.budget.fee)
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
        }))
    }
}
