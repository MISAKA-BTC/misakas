//! Read projection of the same authenticated Case replay used by the writer.
//! A snapshot alone is not a live service authorization. Consensus must bind it
//! to its own publication/import generation and current real funding UTXO.
use super::source::capacity;
use super::*;
use crate::{
    palw_positive_assignment_funds_v2::native_assignment_outpoint_v2,
    palw_positive_assignment_resources_v2::AssignmentWorkQuantitiesV2,
    palw_positive_source_read_v2::{PositiveSourceReadBoundsV2, PositiveSourceReadErrorV2, PositiveSourceUnitBoundsV2},
    tx::{TransactionOutpoint, UtxoEntry},
};
use misaka_palw_positive_contract::{
    funding::funding_scope,
    identity::RequestId,
    messages::{MinerDataStatus, SourceRequest},
    rows::FundingRow,
};

/// Private construction requires replay of the full rooted native archive.
/// This intentionally does not implement the live `PositiveSourceReadV2` port.
pub struct NativeAssignedSourceSnapshotV2 {
    request: SourceRequest,
    result: MinerResultCommit,
    bounds: PositiveSourceReadBoundsV2,
    units: Vec<(PositiveSourceUnitBoundsV2, MinerDataStatus)>,
    funding: FundingRow,
    _lease: NativeDemandLeaseV1,
}
impl NativeAssignedSourceSnapshotV2 {
    pub fn request(&self) -> &SourceRequest {
        &self.request
    }
    pub fn result(&self) -> &MinerResultCommit {
        &self.result
    }
    pub fn bounds(&self) -> PositiveSourceReadBoundsV2 {
        self.bounds
    }
    pub fn unit(&self, unit: u64) -> Result<(PositiveSourceUnitBoundsV2, MinerDataStatus), PositiveSourceReadErrorV2> {
        let index =
            unit.checked_sub(self.request.first).and_then(|n| usize::try_from(n).ok()).ok_or(PositiveSourceReadErrorV2::Range)?;
        self.units.get(index).copied().ok_or(PositiveSourceReadErrorV2::Range)
    }
    pub fn funding_outpoint(&self) -> Option<TransactionOutpoint> {
        self.funding.current_outpoint.map(native_assignment_outpoint_v2)
    }
    pub fn funding_utxo_matches(&self, entry: Option<&UtxoEntry>) -> bool {
        let remaining = [self.funding.fee.remaining, self.funding.capital.remaining, self.funding.surplus.remaining]
            .into_iter()
            .try_fold(0u64, u64::checked_add);
        match (self.funding.current_outpoint, entry, remaining) {
            (None, None, Some(0)) => true,
            (Some(_), Some(entry), Some(amount)) => {
                !entry.is_coinbase
                    && entry.amount == amount
                    && crate::mldsa87_primitives::p2pkh_mldsa87_payload(&entry.script_public_key).map(|p| Digest(p.as_bytes()))
                        == Some(self.funding.consent.refund_payload)
            }
            _ => false,
        }
    }
}

/// Resolve only accepted op3 requests and literal op4 first receipts. The
/// caller supplies its native selected-parent archive, not a peer journal.
pub fn replay_assigned_source_snapshot_v2(
    state: &PalwChainStateV2,
    backing: &NativeCommittedAssignedCaseV2,
    request: RequestId,
    result: ResultId,
    now: u64,
) -> Result<NativeAssignedSourceSnapshotV2, PalwStateV2Error> {
    if state.state_root() != backing.parent_state_root() || result != backing.pin().result {
        return Err(refuse("source read differs from the selected-parent archive"));
    }
    let (assignment, bundle, pin) = rooted_bundle(state, backing.assignment().id())?;
    if &assignment != backing.assignment() || &pin != backing.pin() {
        return Err(refuse("source read archive does not match the rooted Case"));
    }
    let machine = replay_assigned_court_history_v2(state, backing, &bundle)?;
    let ledger = machine.source_ledger.as_ref().ok_or_else(missing)?;
    let retained = ledger.request(request).ok_or_else(missing)?;
    if retained.material.origin.accepted_daa > now
        || ledger.result().id() != result
        || ledger.service() != &bundle.case.service
        || machine.source_requests.get(&request) != Some(retained)
    {
        return Err(refuse("source read lacks its literal accepted request"));
    }
    let request = retained.request;
    let geometry = &machine.source.geometry;
    geometry.validate_request(&request).map_err(refuse)?;
    let NativeRowBody::Funding(funding) = required(state, NativeRowKey::Funding(pin.case))? else {
        return Err(refuse("source read lacks its funded Case"));
    };
    let material = backing.material();
    if funding.consent.scope != funding_scope(ledger.result(), ledger.service(), &funding.basis)
        || funding.basis.schedule != material.native_preimages.case_fees.id()
        || machine.settled_source_fee > funding.fee.paid
    {
        return Err(refuse("source read funding scope differs from original service"));
    }
    let pending = ledger.pending_units_v2().try_fold(0u64, |total, unit| total.checked_add(unit.fee.amount)).ok_or_else(capacity)?;
    let unpaid = ledger.usage().fee.checked_sub(machine.settled_source_fee).ok_or_else(capacity)?;
    if pending.checked_add(unpaid).is_none_or(|due| due > funding.fee.remaining)
        || (pending != 0 || unpaid != 0) && funding.current_outpoint.is_none()
    {
        return Err(refuse("source read duties exceed the actual remaining Case escrow"));
    }
    let count = request.end.checked_sub(request.first).ok_or_else(capacity)?;
    let mut quote = TirNativeRequestsV1::new();
    quote.exact_v1::<NativeAssignedSourceSnapshotV2>(1).map_err(PalwStateV2Error::NativeResource)?;
    quote.exact_v1::<(PositiveSourceUnitBoundsV2, MinerDataStatus)>(count).map_err(PalwStateV2Error::NativeResource)?;
    let bytes = quote.bytes_v1();
    if machine
        .checker_heap
        .checked_add(machine.candidate_heap)
        .and_then(|n| n.checked_add(bytes))
        .is_none_or(|n| n > machine.available_heap)
    {
        return Err(capacity());
    }
    let lease = reserve_native_requests_v1(bytes).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(capacity)?;
    let schedule = &material.native_preimages.source_policy.work;
    let controls = count.checked_add(ledger.quoted_units_v2()).ok_or_else(capacity)?;
    if schedule.court_operation_work(0, controls, 0, 0).map_err(refuse)? > machine.remaining_work {
        return Err(refuse("source read projection exceeds the original replay work"));
    }
    let mut units = Vec::new();
    units.try_reserve_exact(usize::try_from(count).map_err(|_| capacity())?).map_err(|_| capacity())?;
    if units.capacity() as u64 != count {
        return Err(capacity());
    }
    let mut maximum_body = 0;
    for unit in request.first..request.end {
        let bound = geometry.unit_bound(request.kind, unit).map_err(refuse)?;
        let body_bytes = geometry.canonical_opening_bytes_v2(request.kind, unit).map_err(refuse)?;
        maximum_body = maximum_body.max(body_bytes);
        units.push((
            PositiveSourceUnitBoundsV2 { data_bytes: bound.max_data_bytes, body_bytes },
            ledger.status(request.kind, request.source, unit),
        ));
    }
    let work_per_read = schedule
        .charge(AssignmentWorkQuantitiesV2 {
            codec_steps: maximum_body,
            hash_bytes: maximum_body,
            signatures: 0,
            elements: 0,
            terms: 0,
            io_bytes: maximum_body,
            controls: 1,
        })
        .map_err(refuse)?;
    Ok(NativeAssignedSourceSnapshotV2 {
        request,
        result: ledger.result().clone(),
        bounds: PositiveSourceReadBoundsV2 {
            source_units: geometry.source_units(request.kind),
            max_unit_bytes: maximum_body,
            work_per_read,
            budget: ledger.service().budget,
            now,
            cutoff: ledger.service().deadline.min(assignment.source_consent.service_before_daa),
        },
        units,
        funding,
        _lease: lease,
    })
}
