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
    pub(super) fn check_committed(&self, state: &PalwChainStateV2, prior: &CaseRow) -> Result<(), PalwStateV2Error> {
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
        self.admit_source_index_entry_v2::<Digest, CheckedAssignedSourceResponseV2>(body_heap)?;
        self.source_responses.insert(id, checked);
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

impl TransitionBuilder<'_> {
    /// Preflight op12 against the unique first receipts and actual current
    /// input. The enclosing writer owns the spend, updates and journal.
    pub(in crate::palw_state_v2) fn check_case_source_settlement_v2(
        &self,
        action: &PositiveAssignmentRawActionV2,
        assignment: AssignmentId,
        case: CaseId,
        receipts: &[AcceptedMaterial],
    ) -> Result<(), PalwStateV2Error> {
        use crate::{
            mldsa87_primitives::p2pkh_mldsa87_payload,
            palw_positive_assignment_funds_v2::native_assignment_outpoint_v2,
            palw_settlement_once_v1::{
                PALW_SETTLEMENT_ONCE_TABLE_V1, PalwSettlementLegV1, PalwSettlementObligationV1, palw_settlement_subject_v1,
            },
        };
        let (backing, machine, prior) = self.prepare_case_subject_v2(action, case, None)?;
        if backing.assignment().id() != assignment
            || receipts.is_empty()
            || receipts.len() as u64 > backing.material().native_preimages.source_policy.max_responses
        {
            return Err(refuse("source settlement has the wrong Assignment or receipt extent"));
        }
        let NativeRowBody::Funding(funding) = required(&self.state, NativeRowKey::Funding(case))? else {
            return Err(refuse("missing Case funding for settlement"));
        };
        let point = native_assignment_outpoint_v2(funding.current_outpoint.ok_or_else(missing)?);
        let tx = action.transaction();
        let entries = action.entries();
        let controls = (receipts.len() as u64)
            .checked_mul(4)
            .and_then(|n| n.checked_add(entries.len() as u64))
            .and_then(|n| n.checked_add(tx.outputs.len() as u64))
            .ok_or_else(capacity)?;
        let work = backing.material().native_preimages.source_policy.work.court_operation_work(0, controls, 0, 0).map_err(refuse)?;
        if work > machine.remaining_work {
            return Err(refuse("source settlement exceeds the remaining Case work"));
        }
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
        let route = self.state.kernel_route_state_v1().ok_or_else(missing)?;
        let mut quote = TirNativeRequestsV1::new();
        quote.exact_v1::<BTreeNodeEnvelopeV1<Digest, ()>>(receipts.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        quote.exact_v1::<u8>(64).map_err(PalwStateV2Error::NativeResource)?;
        let bytes = quote.bytes_v1();
        if machine
            .available_heap
            .checked_sub(machine.checker_heap)
            .and_then(|n| n.checked_sub(machine.candidate_heap))
            .is_none_or(|n| bytes > n)
        {
            return Err(capacity());
        }
        let _scratch = reserve_native_requests_v1(bytes).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut total = 0u64;
        for (index, supplied) in receipts.iter().enumerate() {
            let checked =
                machine.source_responses.get(&supplied.digest).ok_or_else(|| refuse("settlement receipt absent from Case replay"))?;
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
            let NativeRowBody::ServiceReceipt(rooted) = required(&self.state, fee.receipt_key)? else {
                return Err(refuse("settlement receipt is missing"));
            };
            let obligation = PalwSettlementObligationV1::with_party(
                PalwSettlementLegV1::AssignmentServiceFee,
                palw_settlement_subject_v1(b"assignment-service/v2", &[&assignment.0.0, &supplied.digest.0]),
                provider.0.0,
                0,
            );
            if rooted.settlement.is_some()
                || rooted.response != *supplied
                || rooted.result != fee.result
                || rooted.provider != provider
                || rooted.duty != checked.duty
                || rooted.bytes != checked.data_bytes
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
                .filter(|n| *n <= funding.fee.remaining)
                .ok_or_else(|| refuse("source settlement exceeds remaining Case fees"))?;
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
        match (unrelated_in, unrelated_out) {
            (Some(inputs), Some(outputs)) if inputs.checked_sub(outputs) == Some(action.fee()) => Ok(()),
            _ => Err(refuse("Case funds cannot sponsor unrelated outputs or carrier fees")),
        }
    }
}
