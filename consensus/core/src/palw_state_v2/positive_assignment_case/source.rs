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
    assignment::{
        funding_relay::{FUNDING_RELAY_OPERATION_V2, FundingRelayRegistrationBodyV2, RelayFundingPurposeV2, RelayTargetV2},
        funding_relay_rows::{FundingRelayBindingV2, FundingRelayCustodyRowV2},
    },
    funding::{SourceFeeDeltaIntent, funding_scope},
    identity::{RequestId, principal},
    rows::{FundingRow, MAX_NATIVE_KEY_BYTES, NativeHash64, NativeOutpoint, NativeRowValue, ServiceReceiptRow},
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

struct CaseSourcePaymentV2 {
    funding: misaka_palw_positive_contract::rows::FundingRow,
    debit: NativeOutpoint,
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
        let (Some(minimum_service_cutoff), Some(point)) = (self.relay_service_cutoff, funding.current_outpoint) else {
            return Ok(None);
        };
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
        (decode_heap, daa, accepting_block): (u64, u64, Option<NativeHash64>),
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
        if self
            .available_heap
            .checked_sub(self.checker_heap)
            .and_then(|n| n.checked_sub(self.candidate_heap))
            .and_then(|n| n.checked_sub(decode_heap))
            .is_none_or(|n| bytes > n)
        {
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
        if unrelated_in.zip(unrelated_out).and_then(|(inputs, outputs)| inputs.checked_sub(outputs)) != Some(carrier_fee) {
            return Err(refuse("Case funds cannot sponsor unrelated outputs or carrier fees"));
        }
        funding.fee =
            debit_assignment_leg_v2(funding.fee, AssignmentLegDebitV2 { paid: total, ..Default::default() }, 0).map_err(refuse)?;
        funding.current_outpoint = if continuation == 0 {
            None
        } else {
            Some(misaka_palw_positive_contract::rows::NativeOutpoint {
                transaction: misaka_palw_positive_contract::rows::NativeHash64(tx.id().as_bytes()),
                index: u32::try_from(receipts.len()).map_err(refuse)?,
            })
        };
        drop(seen);
        drop(_scratch);
        let relay_heap = self
            .available_heap
            .checked_sub(self.checker_heap)
            .and_then(|n| n.checked_sub(self.candidate_heap))
            .and_then(|n| n.checked_sub(decode_heap))
            .ok_or_else(capacity)?;
        let successor =
            self.qualify_case_relay_successor_v2(state, backing, &funding, (debit, continuation), (daa, accepting_block), relay_heap)?;
        Ok(CaseSourcePaymentV2 { funding, debit, successor })
    }

    pub(super) fn replay_case_source_payment_v2(
        &mut self,
        backing: &NativeCommittedAssignedCaseV2,
        prefix: &PalwChainStateV2,
        after: &PalwChainStateV2,
        prior: &CaseRow,
        carriage: &NativeAcceptedCaseCarriageV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_positive_court_v2::coordinator::assignment_body::{
            PositiveAssignmentNativeOperationV2, decode_assignment_body_v2,
        };
        let original = backing.material();
        let rules = NativeAssignmentRulesV2::from_original_material_data_v2(backing.assignment(), original).map_err(refuse)?;
        let raw = carriage.transaction().payload.get(3..).ok_or_else(missing)?;
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
        let _decode = reserve_native_requests_v1(summary.heap).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity)?;
        let body = decode_assignment_body_v2(raw, limits).map_err(refuse)?;
        let PositiveAssignmentNativeOperationV2::SettleAssignment { assignment, case: Some(case), receipts } = body.operation else {
            return Err(refuse("Case settlement journal requires the literal op12"));
        };
        if case != prior.case {
            return Err(refuse("settlement journal names a different Case"));
        }
        let mut payment = self.qualify_case_source_payment_v2(
            prefix,
            backing,
            prior,
            (carriage.transaction(), carriage.entries(), carriage.fee()),
            assignment,
            &receipts,
            (summary.heap, carriage.origin().accepted_daa, Some(carriage.origin().accepting_block)),
        )?;
        if let Some(successor) = payment.successor.take() {
            let (expected, _lease) = successor.bind(carriage.origin(), prior.reservation);
            let key = NativeRowKey::FundingRelayCustody { funding: expected.funding_outpoint, purpose: expected.purpose };
            if required(after, key)? != NativeRowBody::FundingRelayCustody(expected) {
                return Err(refuse("Case successor relay was not bound atomically with the historical payment"));
            }
        }
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
        Ok(())
    }
}

impl TransitionBuilder<'_> {
    pub(in crate::palw_state_v2) fn apply_case_source_settlement_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
        assignment: AssignmentId,
        case: CaseId,
        receipts: &[AcceptedMaterial],
    ) -> Result<(), PalwStateV2Error> {
        use super::super::palw_settlement_once_fold_v1::PalwSettleOnceV1;
        if !self.params.settlement_once_active_at(ctx.daa_score) {
            return Err(refuse("actual settlement-once owner is dormant"));
        }
        let (backing, mut machine, mut prior) = self.prepare_case_subject_v2(action, case, None)?;
        let origin = action.origin();
        let mut payment = machine.qualify_case_source_payment_v2(
            &self.state,
            backing,
            &prior,
            (action.transaction(), action.entries(), action.fee()),
            assignment,
            receipts,
            (0, ctx.daa_score, Some(origin.accepting_block)),
        )?;
        let material = source_material(origin)?;
        payment.funding.last_settlement = Some(material);
        if let Some(point) = payment.funding.current_outpoint {
            self.require_assignment_output_unlocked_v2(point)?;
        }
        // The active raw op12 owns the enclosing checkpoint, including every
        // once marker, source receipt, funding row and retained journal entry.
        for supplied in receipts {
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
        self.write_assigned_case_payment_journal_v2(backing)
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
}

pub fn qualify_case_source_spend_v2(
    state: &PalwChainStateV2,
    backing: &NativeCommittedAssignedCaseV2,
    transaction: &crate::tx::Transaction,
    entries: &[crate::tx::UtxoEntry],
    daa: u64,
) -> Result<QualifiedCaseSourceSpendV2, PalwStateV2Error> {
    use crate::palw_positive_assignment_funds_v2::native_assignment_outpoint_v2;
    use crate::palw_positive_court_v2::coordinator::assignment_body::{
        PositiveAssignmentNativeOperationV2, decode_assignment_body_v2, scan_assignment_body_v2,
    };
    if transaction.payload.get(..3) != Some(&[1, 0, 176])
        || transaction.payload.get(7) != Some(&12)
        || transaction.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
        || !transaction.id_is_current()
        || state.state_root() != backing.parent_state_root()
    {
        return Err(refuse("Case spend differs from the actual selected-parent funding or carrier"));
    }
    let assignment = backing.assignment();
    let rules = NativeAssignmentRulesV2::from_original_material_data_v2(assignment, backing.material()).map_err(refuse)?;
    let limits = rules.body_limits().map_err(refuse)?;
    let raw = &transaction.payload[3..];
    let summary = scan_assignment_body_v2(raw, limits).map_err(refuse)?;
    let _decode = reserve_native_requests_v1(summary.heap).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity)?;
    let body = decode_assignment_body_v2(raw, limits).map_err(refuse)?;
    let PositiveAssignmentNativeOperationV2::SettleAssignment { assignment: supplied, case: Some(case), receipts } = body.operation
    else {
        return Err(refuse("Case spend must name its Case"));
    };
    if supplied != assignment.id() || case != backing.pin().case {
        return Err(refuse("Case spend names a different Assignment or Case"));
    }
    let (_, bundle, pin) = rooted_bundle(state, supplied)?;
    if pin != *backing.pin() {
        return Err(missing());
    }
    let fee = entries
        .iter()
        .map(|entry| entry.amount)
        .try_fold(0u64, u64::checked_add)
        .zip(transaction.outputs.iter().map(|output| output.value).try_fold(0u64, u64::checked_add))
        .and_then(|(inputs, outputs)| inputs.checked_sub(outputs))
        .ok_or_else(|| refuse("Case candidate fee arithmetic"))?;
    let mut machine = replay_assigned_court_history_v2(state, backing, &bundle)?;
    let payment = machine.qualify_case_source_payment_v2(
        state,
        backing,
        &bundle.case,
        (transaction, entries, fee),
        supplied,
        &receipts,
        (summary.heap, daa, None),
    )?;
    Ok(QualifiedCaseSourceSpendV2 {
        transaction: transaction.id(),
        carrier_hash: crate::hashing::tx::hash(transaction),
        inputs: crate::palw_positive_assignment_case_material_v2::assigned_case_inputs_id_v2(transaction.id(), entries)
            .map_err(refuse)?,
        debit: native_assignment_outpoint_v2(payment.debit),
        continuation: payment.funding.current_outpoint.map(native_assignment_outpoint_v2),
    })
}
