//! Ephemeral accepted-material output, separate from canonical state/delta.
//! Only the native checkpoint writer creates records. It retains typed buffer
//! reservations until the processor stages the SAME delta and UTXO WriteBatch.
use super::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, NativeRetainedV1};
use misaka_palw_positive_contract::{
    assignment::{
        journal::*,
        open_material::{AssignmentOpenLimitsV2, AssignmentOpenViewV2},
        rows::{AssignmentJournalFrameRow, AssignmentUseRow},
    },
    native::KERNEL_AUX_TABLE,
    rows::{NativeRowBody, NativeRowValue},
};

pub struct PositiveAssignmentAcceptedMaterialV2 {
    header: AssignmentJournalHeaderV2,
    raw: Vec<u8>,
    max_record_bytes: u64,
    pub(super) delta_end: usize,
}
impl PositiveAssignmentAcceptedMaterialV2 {
    pub fn header(&self) -> AssignmentJournalHeaderV2 {
        self.header
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn max_record_bytes(&self) -> u64 {
        self.max_record_bytes
    }
}
#[derive(Default)]
pub struct PositiveAssignmentMaterialOutcomeV2 {
    pub(super) records: Vec<PositiveAssignmentAcceptedMaterialV2>,
    pub(super) case_records: Vec<PositiveCaseAcceptedMaterialV2>,
}
impl PositiveAssignmentMaterialOutcomeV2 {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty() && self.case_records.is_empty()
    }
    pub fn case_records(&self) -> &[PositiveCaseAcceptedMaterialV2] {
        &self.case_records
    }
    pub fn records(&self) -> &[PositiveAssignmentAcceptedMaterialV2] {
        &self.records
    }
    /// Empty old-path output only. It cannot satisfy staging of any new
    /// Assignment mutation; no nonempty record constructor is public.
    pub fn empty_retained_for_legacy_v2() -> NativeRetainedV1<Self> {
        Self::empty_retained_v2()
    }
    pub(crate) fn empty_retained_v2() -> NativeRetainedV1<Self> {
        NativeRetainedV1::from_owner_reserved_v1(Self { records: Vec::new(), case_records: Vec::new() }, None)
    }
    /// Compare independent UTXO acceptance with the actual native writes.
    /// This check only runs after the raw actions have derived and applied
    /// their own settlements against the executed prefix.
    pub(super) fn check_utxo_settlements_v2(
        &self,
        entries: &[PalwDeltaEntryV2],
        plans: &[crate::palw_positive_assignment_funds_v2::AssignmentSpendPlanV2],
    ) -> Result<(), PalwStateV2Error> {
        let mismatch = || PalwStateV2Error::DeltaMismatch("accepted Assignment UTXO spend and rooted settlement differ");
        let mut plans = plans.iter();
        let mut start = 0;
        for record in &self.records {
            let event = entries.get(start..record.delta_end).ok_or_else(mismatch)?;
            start = record.delta_end;
            if record.header.kind != AssignmentJournalEventKindV2::Settlement {
                continue;
            }
            let plan = plans.next().ok_or_else(mismatch)?;
            let view = AssignmentJournalRecordViewV2::decode(&record.raw, record.max_record_bytes).map_err(|_| mismatch())?;
            let expected = plan.accepted_funding_row_v2(&view.frame(), view.payload()).map_err(|_| mismatch())?;
            let funding_key = misaka_palw_positive_contract::native::NativeRowKey::AssignmentFunding(plan.old.assignment).encode();
            let mut found = false;
            for entry in event {
                let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry else { continue };
                if *table != KERNEL_AUX_TABLE || key != &funding_key {
                    continue;
                }
                let (Some(old), Some(new)) = (old.as_deref(), new.as_deref()) else { return Err(mismatch()) };
                let old = NativeRowValue::decode(key, old).map_err(|_| mismatch())?.body;
                let new = NativeRowValue::decode(key, new).map_err(|_| mismatch())?.body;
                if found
                    || !matches!(old, NativeRowBody::AssignmentFunding(row) if row == plan.old)
                    || !matches!(new, NativeRowBody::AssignmentFunding(row) if row == expected)
                {
                    return Err(mismatch());
                }
                found = true;
            }
            if !found {
                return Err(mismatch());
            }
        }
        if plans.next().is_some() {
            return Err(mismatch());
        }
        Ok(())
    }

    pub(super) fn validate_v2(&self, entries: &[PalwDeltaEntryV2]) -> Result<(), PalwStateV2Error> {
        let failure = |why| PalwStateV2Error::DeltaMismatch(why);
        for record in &self.records {
            let view = AssignmentJournalRecordViewV2::decode(&record.raw, record.max_record_bytes)
                .map_err(|_| failure("accepted Assignment material bytes changed"))?;
            if view.header() != record.header || record.delta_end > entries.len() {
                return Err(failure("Assignment material watermark/header changed"));
            }
            let mut frame = false;
            let mut head = false;
            let mut assignment = false;
            for entry in entries.iter().take(record.delta_end) {
                let PalwDeltaEntryV2::KernelRouteRow { table, key, new: Some(bytes), .. } = entry else {
                    continue;
                };
                if *table != KERNEL_AUX_TABLE {
                    continue;
                }
                let row = NativeRowValue::decode(key, bytes).map_err(|_| failure("invalid aux151 material delta"))?;
                match row.body {
                    NativeRowBody::AssignmentJournalFrame(f) if f == view.frame() => frame = true,
                    NativeRowBody::AssignmentJournalHead(h)
                        if h.assignment == record.header.assignment && h.last_frame == record.header.id() =>
                    {
                        head = true
                    }
                    NativeRowBody::Assignment(a) if a.id() == record.header.assignment && a.last_event == record.header.origin => {
                        assignment = true
                    }
                    _ => {}
                }
            }
            if !frame || !head || !assignment {
                return Err(failure("accepted Assignment material lacks atomic frame/head/Assignment delta"));
            }
        }
        for entry in entries {
            let PalwDeltaEntryV2::KernelRouteRow { table, key, new: Some(bytes), .. } = entry else {
                continue;
            };
            if *table != KERNEL_AUX_TABLE {
                continue;
            }
            match NativeRowValue::decode(key, bytes).map_err(|_| failure("invalid aux151 material delta"))?.body {
                NativeRowBody::AssignmentJournalFrame(f) if self.records.iter().filter(|r| r.header == f.header).count() != 1 => {
                    return Err(failure("Assignment frame must have exactly one retained material record"));
                }
                NativeRowBody::AssignmentJournalHead(h) if !self.records.iter().any(|r| r.header.id() == h.last_frame) => {
                    return Err(failure("Assignment head has no retained material record"));
                }
                NativeRowBody::Assignment(a)
                    if !self.records.iter().any(|r| r.header.assignment == a.id() && r.header.origin == a.last_event) =>
                {
                    return Err(failure("Assignment mutation has no retained material record"));
                }
                _ => {}
            }
        }
        validate_case_materials_v2(&self.case_records, &self.records, entries)?;
        Ok(())
    }
}
impl NativeHeapV1 for PositiveAssignmentAcceptedMaterialV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { header, raw, max_record_bytes: _, delta_end: _ } = self;
        header.native_heap_v1(path).add::<Self>(raw.native_heap_v1(path), path)
    }
}
impl NativeHeapV1 for PositiveAssignmentMaterialOutcomeV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { records, case_records } = self;
        records.native_heap_v1(path).add::<Self>(case_records.native_heap_v1(path), path)
    }
}
impl TransitionBuilder<'_> {
    /// Reserves typed record slots before the first new operation. Exactly one
    /// accepted journal frame per raw action is supported by this profile.
    pub(super) fn reserve_assignment_material_slots_v2(&mut self, count: usize) -> Result<(), PalwStateV2Error> {
        if !self.assignment_materials.is_empty() || self.assignment_materials.capacity() != 0 {
            return Err(PalwStateV2Error::DeltaMismatch("Assignment material capacity already prepared"));
        }
        if count == 0 {
            return Ok(());
        }
        let slots = (count as u64)
            .checked_mul(2)
            .ok_or_else(|| PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<Self>("Assignment material slots")))?;
        let mut quote = native_writers::NativeWriteQuoteV1::new("accepted Assignment material slots");
        quote.exact::<PositiveAssignmentAcceptedMaterialV2>(slots)?;
        quote.exact::<PositiveAssignmentMaterialOutcomeV2>(1)?;
        quote.exact::<PositiveCaseAcceptedMaterialV2>(slots)?;
        self.admit_native_write_v1(quote)?;
        self.assignment_case_materials.try_reserve_exact(count).map_err(|_| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<Self>(
                "accepted Case material slots",
                "allocation refused",
            ))
        })?;
        self.assignment_materials.try_reserve_exact(count).map_err(|_| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<Self>(
                "accepted Assignment material slots",
                "allocation refused",
            ))
        })?;
        Ok(())
    }
    /// Must be called inside the same raw native checkpoint AFTER exact row,
    /// head and frame writes. Open supplies the actual immutable material and
    /// custody wrapper; every other event supplies its literal actual opbody.
    pub(super) fn emit_assignment_material_v2(
        &mut self,
        frame: &AssignmentJournalFrameRow,
        payload: &[u8],
        payload_cap: u64,
    ) -> Result<(), PalwStateV2Error> {
        let failure = |why| PalwStateV2Error::DeltaMismatch(why);
        let action = self.assignment_active_raw.as_ref().ok_or_else(|| failure("Assignment material has no active actual carrier"))?;
        if frame.header.origin != action.origin()
            || payload_cap == 0
            || payload_cap == u64::MAX
            || self.assignment_materials.len() == self.assignment_materials.capacity()
        {
            return Err(failure("Assignment material origin/capacity mismatch"));
        }
        let envelope =
            misaka_palw_positive_contract::assignment::control_material::AssignmentControlViewV2::decode(payload, payload_cap)
                .map_err(|_| failure("Assignment event lacks canonical native control envelope"))?;
        let boundary =
            AssignmentControlBoundaryV2::decode(envelope.boundary()).map_err(|_| failure("invalid native Assignment boundary"))?;
        let expected = self.current_assignment_control_boundary_v2(frame.header.assignment)?;
        if boundary != expected {
            return Err(failure("Assignment boundary differs from actual executed prefix"));
        }
        let inner = envelope.inner();
        let native = &action.transaction().payload[3..];
        if frame.header.kind == AssignmentJournalEventKindV2::Open {
            let open = AssignmentOpenViewV2::decode(inner, AssignmentOpenLimitsV2::within_payload(payload_cap))
                .map_err(|_| failure("invalid accepted Open material wrapper"))?;
            if open.native_body() != native {
                return Err(failure("Open material does not bind literal actual op8"));
            }
        } else if inner != native {
            return Err(failure("Assignment event material differs from literal actual opbody"));
        }
        frame.header.check_payload(payload, payload_cap).map_err(|_| failure("Assignment payload hash/shape changed"))?;
        if self.assignment_materials.iter().any(|r| r.header.id() == frame.header.id()) {
            return Err(failure("Assignment material emitted twice"));
        }
        let max_record_bytes = payload_cap
            .checked_add(ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64)
            .ok_or_else(|| failure("Assignment record cap overflow"))?;
        let size = (payload.len() as u64)
            .checked_add(ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64)
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(|| failure("Assignment record buffer overflow"))?;
        let mut quote = native_writers::NativeWriteQuoteV1::new("accepted Assignment literal record");
        quote.exact::<u8>(size)?;
        self.admit_native_write_v1(quote)?;
        let raw = encode_assignment_journal_record_v2(frame.header, payload, max_record_bytes)
            .map_err(|_| failure("Assignment literal record encoding failed"))?;
        self.assignment_materials.push(PositiveAssignmentAcceptedMaterialV2 {
            header: frame.header,
            raw,
            max_record_bytes,
            delta_end: self.entries.len(),
        });
        Ok(())
    }
}

pub struct PositiveCaseAcceptedMaterialV2 {
    header: misaka_palw_positive_contract::journal::CaseJournalHeader,
    raw: Vec<u8>,
    limits: misaka_palw_positive_contract::journal::CaseJournalLimits,
    pub(super) delta_end: usize,
}
impl PositiveCaseAcceptedMaterialV2 {
    pub fn header(&self) -> misaka_palw_positive_contract::journal::CaseJournalHeader {
        self.header
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn limits(&self) -> misaka_palw_positive_contract::journal::CaseJournalLimits {
        self.limits
    }
}
impl NativeHeapV1 for PositiveCaseAcceptedMaterialV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { header, raw, limits: _, delta_end: _ } = self;
        header.native_heap_v1(path).add::<Self>(raw.native_heap_v1(path), path)
    }
}
fn validate_case_materials_v2(
    records: &[PositiveCaseAcceptedMaterialV2],
    assignment_records: &[PositiveAssignmentAcceptedMaterialV2],
    entries: &[PalwDeltaEntryV2],
) -> Result<(), PalwStateV2Error> {
    use crate::palw_positive_assignment_case_material_v2::AssignmentCaseBoundaryV2;
    use misaka_palw_positive_contract::journal::{CaseJournalEventKind, CaseJournalRecordView};
    let failure = |why| PalwStateV2Error::DeltaMismatch(why);
    let mut previous_end = 0;
    let mut source_receipt_count = 0usize;
    let mut source_union_count = 0usize;
    for record in records {
        let view = CaseJournalRecordView::decode(&record.raw, record.limits).map_err(|_| failure("accepted Case material changed"))?;
        if view.header() != record.header || record.delta_end > entries.len() {
            return Err(failure("Case material header/watermark changed"));
        }
        let mut boundaries = view.aux().filter_map(|aux| {
            let boundary = AssignmentCaseBoundaryV2::decode(aux.preimage()).ok()?;
            (boundary.case == record.header.case && boundary.origin == record.header.origin && boundary.material() == aux.material())
                .then_some(boundary)
        });
        let boundary = boundaries.next().ok_or_else(|| failure("Case journal lacks its exact native boundary"))?;
        let start = usize::try_from(boundary.before_delta_end).map_err(|_| failure("Case journal delta start extent"))?;
        if boundaries.next().is_some() || start < previous_end || start >= record.delta_end {
            return Err(failure("Case journal has an overlapping or empty native checkpoint"));
        }
        let event = &entries[start..record.delta_end];
        previous_end = record.delta_end;
        let assigned_initial = record.header.kind == CaseJournalEventKind::NativeTransaction && record.header.sequence == 1;
        if record.header.kind == CaseJournalEventKind::NativeBody
            && let Some(operation @ (3 | 4)) = view.payload().get(4).copied()
        {
            validate_source_case_material_v2(view, operation, event, record.raw.len() as u64)?;
            source_union_count += 1;
            source_receipt_count += usize::from(operation == 4);
            continue;
        }
        let mut consumed = None;
        let mut case = false;
        let mut head = false;
        for entry in event {
            let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new: Some(bytes) } = entry else {
                continue;
            };
            if *table != KERNEL_AUX_TABLE {
                continue;
            }
            match NativeRowValue::decode(key, bytes).map_err(|_| failure("invalid Case material delta"))?.body {
                NativeRowBody::Case(c)
                    if c.case == record.header.case
                        && c.result == record.header.result
                        && c.coverage.scope == record.header.scope
                        && c.last_event == record.header.origin =>
                {
                    case = true
                }
                NativeRowBody::JournalHead(h)
                    if h.case == record.header.case
                        && h.sequence == record.header.sequence
                        && h.previous == record.header.previous
                        && h.head == view.head()
                        && h.history == view.history() =>
                {
                    head = true
                }
                NativeRowBody::AssignmentUse(row)
                    if assigned_initial
                        && row.case == record.header.case
                        && row.result == record.header.result
                        && row.origin == record.header.origin =>
                {
                    if old.is_some() || consumed.replace(row).is_some() {
                        return Err(failure("initial Case must consume one new AssignmentUse"));
                    }
                }
                _ => {}
            }
        }
        if !case || !head {
            return Err(failure("Case material missing atomic Case/head mutation"));
        }
        if assigned_initial {
            let use_row = consumed.ok_or_else(|| failure("initial Case is missing its atomic AssignmentUse"))?;
            check_initial_case_bind_result_v2(view, &use_row, assignment_records, record.delta_end)?;
        }
    }
    for (offset, entry) in entries.iter().enumerate() {
        let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new: Some(bytes) } = entry else {
            continue;
        };
        if *table != KERNEL_AUX_TABLE {
            continue;
        }
        match NativeRowValue::decode(key, bytes).map_err(|_| failure("invalid Case head delta"))?.body {
            NativeRowBody::JournalHead(h) => {
                let mut count = 0;
                for record in records {
                    let view =
                        CaseJournalRecordView::decode(&record.raw, record.limits).map_err(|_| failure("Case material decode"))?;
                    if view.head() == h.head && view.history() == h.history {
                        count += 1;
                    }
                }
                if count != 1 {
                    return Err(failure("Case head must have exactly one retained native material record"));
                }
            }
            NativeRowBody::AssignmentUse(row) => {
                let count = records
                    .iter()
                    .filter(|record| {
                        record.delta_end > offset
                            && record.header.sequence == 1
                            && record.header.kind == CaseJournalEventKind::NativeTransaction
                            && record.header.case == row.case
                            && record.header.result == row.result
                            && record.header.origin == row.origin
                    })
                    .count();
                if count != 1 {
                    return Err(failure("AssignmentUse must have exactly one atomic initial Case record"));
                }
            }
            NativeRowBody::ServiceReceipt(_) if old.is_none() => {
                source_receipt_count = source_receipt_count
                    .checked_sub(1)
                    .ok_or_else(|| failure("first source receipt has no atomic response journal"))?;
            }
            NativeRowBody::SourceUnion(_) => {
                source_union_count =
                    source_union_count.checked_sub(1).ok_or_else(|| failure("source union has no atomic source journal"))?;
            }
            _ => {}
        }
    }
    if source_receipt_count != 0 || source_union_count != 0 {
        return Err(failure("source journal row counts differ from the executed delta"));
    }
    Ok(())
}
/// A source operation must carry the exact Case boundary and own every
/// corresponding state mutation, including the first receipt on an op4.
fn validate_source_case_material_v2(
    record: misaka_palw_positive_contract::journal::CaseJournalRecordView<'_>,
    operation: u8,
    changes: &[PalwDeltaEntryV2],
    encoded_bytes: u64,
) -> Result<(), PalwStateV2Error> {
    use crate::{
        palw_positive_court_v2::coordinator::native_body::SOURCE_REQUEST_BODY_BYTES_V2,
        palw_positive_source_v2::history::response_body_data,
    };
    use misaka_palw_positive_contract::{messages::SourceRequest, rows::AcceptedMaterial};

    let failure = |why| PalwStateV2Error::DeltaMismatch(why);
    let header = record.header();
    if record.aux().count() != 1 || header.origin.event_ordinal != 0 {
        return Err(failure("source event requires exactly one native boundary"));
    }
    let mut case = None;
    let mut union = None;
    let mut receipt = None;
    let mut head = None;
    for entry in changes {
        let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new: Some(bytes) } = entry else {
            return Err(failure("unexpected source event mutation"));
        };
        if *table != KERNEL_AUX_TABLE {
            return Err(failure("unexpected source event namespace"));
        }
        let decode =
            |raw: &[u8]| NativeRowValue::decode(key, raw).map(|value| value.body).map_err(|_| failure("source delta row codec"));
        let before = old.as_deref().map(decode).transpose()?;
        match (before, decode(bytes)?) {
            (Some(NativeRowBody::Case(before)), NativeRowBody::Case(after)) if case.is_none() => case = Some((before, after)),
            (None, NativeRowBody::SourceUnion(after)) if union.is_none() => union = Some((None, after)),
            (Some(NativeRowBody::SourceUnion(before)), NativeRowBody::SourceUnion(after)) if union.is_none() => {
                union = Some((Some(before), after))
            }
            (None, NativeRowBody::ServiceReceipt(value)) if receipt.is_none() => receipt = Some(value),
            (Some(NativeRowBody::JournalHead(before)), NativeRowBody::JournalHead(after)) if head.is_none() => {
                head = Some((before, after))
            }
            _ => return Err(failure("duplicate or foreign source event mutation")),
        }
    }
    let (before, after) = case.ok_or_else(|| failure("source event is missing its Case mutation"))?;
    let (previous_union, next_union) = union.ok_or_else(|| failure("source event is missing its union mutation"))?;
    let (previous_head, next_head) = head.ok_or_else(|| failure("source event is missing its journal head"))?;
    let normalized = misaka_palw_positive_contract::rows::CaseRow {
        usage: before.usage,
        miner_data: before.miner_data,
        last_event: before.last_event,
        ..after
    };
    if normalized != before
        || after.case != header.case
        || after.result != header.result
        || after.coverage.scope != header.scope
        || after.last_event != header.origin
        || after.usage.witness_attempts != before.usage.witness_attempts
        || before.usage.transmitted_bytes.checked_add(record.payload().len() as u64) != Some(after.usage.transmitted_bytes)
        || before.usage.retained_bytes.checked_add(encoded_bytes) != Some(after.usage.retained_bytes)
        || next_union.case != header.case
        || previous_head.case != header.case
        || previous_head.head != header.previous
        || previous_head.sequence.checked_add(1) != Some(header.sequence)
        || next_head.case != header.case
        || next_head.sequence != header.sequence
        || next_head.previous != header.previous
        || next_head.head != record.head()
        || next_head.history != record.history()
    {
        return Err(failure("source event does not advance its own Case and journal"));
    }
    let work = after
        .usage
        .additional_work
        .checked_sub(before.usage.additional_work)
        .filter(|n| *n != 0)
        .ok_or_else(|| failure("source event work was not charged"))?;
    let material = AcceptedMaterial {
        origin: header.origin,
        digest: misaka_palw_positive_contract::identity::Digest(header.origin.carrier_transaction.0),
    };
    let raw = record.payload();
    if operation == 3 {
        const REQUEST_BYTES: usize = 4 + 3 * 64 + 1 + 16;
        if raw.get(..5) != Some(&[2, 0, 1, 0, 3]) || raw.len() as u64 != SOURCE_REQUEST_BODY_BYTES_V2 {
            return Err(failure("source request journal literal extent"));
        }
        let request: SourceRequest =
            borsh::from_slice(&raw[5..5 + REQUEST_BYTES]).map_err(|_| failure("source request journal codec"))?;
        let old_requested = previous_union.map_or(0, |row| row.requested_units);
        let fresh = next_union.requested_units.checked_sub(old_requested).ok_or_else(|| failure("source request removed units"))?;
        request.validate(next_union.source_units).map_err(|_| failure("source request journal range"))?;
        if receipt.is_some()
            || request.case != header.case
            || request.contract != before.service.id()
            || request.kind != next_union.kind
            || request.source != next_union.source
            || next_union.union != material
            || next_union.completed_units != previous_union.map_or(0, |row| row.completed_units)
            || fresh > request.end - request.first
            || before.usage.rounds.checked_add(u64::from(fresh != 0)) != Some(after.usage.rounds)
            || after.usage.unique_source_bytes < before.usage.unique_source_bytes
            || (fresh == 0 && after.usage.unique_source_bytes != before.usage.unique_source_bytes)
            || after.usage.fee != before.usage.fee
        {
            return Err(failure("source request delta differs from its literal request"));
        }
    } else {
        response_body_data(header.case, raw).map_err(|_| failure("source response journal codec"))?;
        let first = receipt.ok_or_else(|| failure("source response is missing its first receipt"))?;
        let mut expected_union = previous_union.ok_or_else(|| failure("source response has no requested union"))?;
        expected_union.completed_units =
            expected_union.completed_units.checked_add(1).ok_or_else(|| failure("source response unit overflow"))?;
        if next_union != expected_union
            || first.case != header.case
            || first.result != header.result
            || first.kind != next_union.kind
            || first.source != next_union.source
            || first.response != material
            || first.settlement.is_some()
            || first.additional_work != work
            || after.usage.rounds != before.usage.rounds
            || after.usage.unique_source_bytes != before.usage.unique_source_bytes
            || after.usage.fee < before.usage.fee
        {
            return Err(failure("source response does not add one unpaid receipt to the requested union"));
        }
    }
    Ok(())
}

/// The Case record carries the same BindResult frame the Assignment journal
/// committed at this event. Check that cross-link while both native outcomes
/// and their exact pre-commit delta are still owned by the writer.
fn check_initial_case_bind_result_v2(
    record: misaka_palw_positive_contract::journal::CaseJournalRecordView<'_>,
    use_row: &AssignmentUseRow,
    assignment_records: &[PositiveAssignmentAcceptedMaterialV2],
    case_delta_end: usize,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_positive_assignment_case_material_v2::{AssignmentCaseBoundaryV2, initial_assigned_case_bind_header_v2};

    let boundary = record
        .aux()
        .find_map(|aux| AssignmentCaseBoundaryV2::decode(aux.preimage()).ok())
        .ok_or(PalwStateV2Error::DeltaMismatch("initial Case boundary is missing"))?;
    let bind = initial_assigned_case_bind_header_v2(record, boundary, use_row)
        .map_err(|_| PalwStateV2Error::DeltaMismatch("initial Case references a different Assignment BindResult"))?;
    if !assignment_records.iter().any(|r| r.header == bind && r.delta_end <= case_delta_end) {
        return Err(PalwStateV2Error::DeltaMismatch("initial Case BindResult is missing from its atomic Assignment journal"));
    }
    Ok(())
}

fn encode_exact_case_material_v2<T: borsh::BorshSerialize>(value: &T, bytes: usize) -> Result<Vec<u8>, PalwStateV2Error> {
    let mut out = Vec::new();
    out.try_reserve_exact(bytes).map_err(|_| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<T>("initial Case journal", "allocation refused"))
    })?;
    if out.capacity() > bytes {
        return Err(PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<T>(
            "initial Case journal",
            "allocation exceeded the quote",
        )));
    }
    out.resize(bytes, 0);
    let mut writer = out.as_mut_slice();
    borsh::to_writer(&mut writer, value).map_err(|_| PalwStateV2Error::DeltaMismatch("initial Case journal extent changed"))?;
    if !writer.is_empty() {
        return Err(PalwStateV2Error::DeltaMismatch("initial Case journal extent changed"));
    }
    Ok(out)
}

pub(super) fn quote_assigned_case_payment_journal_v2(
    backing: &super::positive_assignment_backing::NativeCommittedAssignedCaseV2,
    transaction: &crate::tx::Transaction,
) -> Result<(u64, usize), PalwStateV2Error> {
    let tx_bytes =
        borsh::object_length(transaction).map_err(|_| PalwStateV2Error::DeltaMismatch("Case settlement transaction extent"))?;
    let sequence = quote_assigned_case_followup_journal_v2(backing, tx_bytes)?;
    Ok((sequence, tx_bytes))
}

pub(super) fn quote_assigned_case_followup_journal_v2(
    backing: &super::positive_assignment_backing::NativeCommittedAssignedCaseV2,
    payload_bytes: usize,
) -> Result<u64, PalwStateV2Error> {
    use crate::palw_positive_assignment_case_material_v2::{
        ASSIGNED_CASE_BOUNDARY_BYTES_V2, assigned_case_followup_journal_extent_v2,
    };
    use misaka_palw_positive_contract::journal::CaseJournalAux;
    let failure = |why| PalwStateV2Error::DeltaMismatch(why);
    let pin = backing.pin();
    let policy = &backing.material().positive_policy;
    let sequence = pin.head.sequence.checked_add(1).ok_or_else(|| failure("Case follow-up sequence"))?;
    if sequence > u64::from(backing.material().assignment_policy.max_history_frames) {
        return Err(failure("Case follow-up journal exceeds original event count"));
    }
    let extent =
        assigned_case_followup_journal_extent_v2(payload_bytes as u64).ok_or_else(|| failure("Case follow-up record extent"))?;
    let logical_heap = payload_bytes
        .checked_add(ASSIGNED_CASE_BOUNDARY_BYTES_V2 + std::mem::size_of::<CaseJournalAux>())
        .ok_or_else(|| failure("Case settlement journal logical heap"))?;
    if logical_heap as u64 > policy.max_case_heap_bytes {
        return Err(failure("Case follow-up exceeds original journal heap"));
    }
    let retained = backing
        .records()
        .iter()
        .map(|record| record.raw().len() as u64)
        .try_fold(extent, u64::checked_add)
        .ok_or_else(|| failure("Case journal cumulative extent"))?;
    if retained > policy.max_case_journal_bytes {
        return Err(failure("Case follow-up exceeds original journal storage"));
    }
    Ok(sequence)
}

impl TransitionBuilder<'_> {
    pub(super) fn unpublished_positive_case_materials_v2(
        &self,
        case: misaka_palw_positive_contract::identity::CaseId,
    ) -> impl Iterator<Item = &PositiveCaseAcceptedMaterialV2> {
        self.assignment_case_materials.iter().filter(move |r| r.header.case == case)
    }
    pub(super) fn write_initial_assigned_case_journal_v2(
        &mut self,
        use_row: &misaka_palw_positive_contract::assignment::rows::AssignmentUseRow,
        scope: misaka_palw_positive_contract::identity::ChainScope,
        bind: &AssignmentJournalHeaderV2,
        policy: &crate::config::params::PositiveCourtPolicyV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_positive_assignment_case_material_v2::{
            ASSIGNED_CASE_BOUNDARY_BYTES_V2, assigned_case_initial_journal_extent_v2,
        };
        use misaka_palw_positive_contract::journal::{
            CaseJournalAux, CaseJournalEventKind, CaseJournalHeader, CaseJournalLimits, CaseJournalRecord,
        };
        let failure = |why| PalwStateV2Error::DeltaMismatch(why);
        let action = self.assignment_active_raw.clone().ok_or_else(|| failure("initial Case journal has no actual carrier"))?;
        let boundary = self.current_positive_case_boundary_v2(use_row.case)?;
        let tx = action.transaction();
        let tx_bytes = borsh::object_length(tx).map_err(|_| failure("initial Case native transaction extent"))?;
        let aux_bytes = ASSIGNED_CASE_BOUNDARY_BYTES_V2 + ASSIGNMENT_JOURNAL_HEADER_BYTES_V2;
        let record_bytes =
            assigned_case_initial_journal_extent_v2(tx_bytes as u64).ok_or_else(|| failure("initial Case record extent"))?;
        let logical = tx_bytes
            .checked_add(aux_bytes + 2 * std::mem::size_of::<CaseJournalAux>())
            .ok_or_else(|| failure("initial Case material extent"))?;
        if record_bytes > policy.max_case_journal_bytes || logical as u64 > policy.max_case_heap_bytes {
            return Err(PalwStateV2Error::KernelRouteRefused("initial assigned Case journal exceeds original policy".into()));
        }
        let mut quote = native_writers::NativeWriteQuoteV1::new("initial Case journal literal bytes");
        quote.exact::<u8>((tx_bytes + aux_bytes) as u64)?;
        quote.exact::<CaseJournalAux>(2)?;
        self.admit_native_write_v1(quote)?;
        let mut aux = [
            CaseJournalAux {
                material: boundary.material(),
                preimage: encode_exact_case_material_v2(&boundary, ASSIGNED_CASE_BOUNDARY_BYTES_V2)?,
            },
            CaseJournalAux {
                material: use_row.binding,
                preimage: encode_exact_case_material_v2(bind, ASSIGNMENT_JOURNAL_HEADER_BYTES_V2)?,
            },
        ];
        // Both references have the exact same origin, so canonical order is
        // digest order. The codec still rejects equal reference bytes.
        if aux[0].material.digest.0 > aux[1].material.digest.0 {
            aux.swap(0, 1);
        }
        let record = CaseJournalRecord {
            header: CaseJournalHeader {
                version: 2,
                revision: 1,
                scope,
                case: use_row.case,
                result: use_row.result,
                sequence: 1,
                previous: misaka_palw_positive_contract::identity::Digest([0; 64]),
                origin: use_row.origin,
                kind: CaseJournalEventKind::NativeTransaction,
            },
            payload: encode_exact_case_material_v2(tx, tx_bytes)?,
            aux: Vec::from(aux),
        };
        self.write_and_emit_positive_case_material_v2(
            &record,
            CaseJournalLimits {
                max_record_bytes: policy.max_case_journal_bytes,
                max_payload_bytes: policy.max_case_journal_bytes,
                max_aux_entries: 2,
                max_aux_preimage_bytes: ASSIGNED_CASE_BOUNDARY_BYTES_V2 as u64,
                max_total_preimage_bytes: policy.max_case_journal_bytes,
                max_logical_heap_bytes: policy.max_case_heap_bytes,
            },
        )
    }

    pub(super) fn write_assigned_case_followup_journal_v2(
        &mut self,
        backing: &super::positive_assignment_backing::NativeCommittedAssignedCaseV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_positive_assignment_case_material_v2::ASSIGNED_CASE_BOUNDARY_BYTES_V2;
        use misaka_palw_positive_contract::journal::{
            CaseJournalAux, CaseJournalEventKind, CaseJournalHeader, CaseJournalLimits, CaseJournalRecord,
        };
        let failure = |why| PalwStateV2Error::DeltaMismatch(why);
        let action = self.assignment_active_raw.clone().ok_or_else(|| failure("Case follow-up journal has no actual carrier"))?;
        let tx = action.transaction();
        let (kind, body) = match tx.payload.get(7) {
            Some(1..=7) => (CaseJournalEventKind::NativeBody, Some(&tx.payload[3..])),
            Some(12) => (CaseJournalEventKind::NativeTransaction, None),
            _ => return Err(failure("Case follow-up journal has the wrong actual carrier")),
        };
        let pin = backing.pin();
        let policy = &backing.material().positive_policy;
        let (sequence, payload_bytes) = match body {
            Some(raw) => (quote_assigned_case_followup_journal_v2(backing, raw.len())?, raw.len()),
            None => quote_assigned_case_payment_journal_v2(backing, tx)?,
        };
        let boundary = self.current_positive_case_boundary_v2(pin.case)?;
        let mut quote = native_writers::NativeWriteQuoteV1::new("Case follow-up literal journal bytes");
        quote.exact::<u8>((payload_bytes + ASSIGNED_CASE_BOUNDARY_BYTES_V2) as u64)?;
        quote.exact::<CaseJournalAux>(1)?;
        self.admit_native_write_v1(quote)?;
        let record = CaseJournalRecord {
            header: CaseJournalHeader {
                version: 2,
                revision: 1,
                scope: pin.scope,
                case: pin.case,
                result: pin.result,
                sequence,
                previous: pin.head.head,
                origin: action.origin(),
                kind,
            },
            payload: match body {
                Some(raw) => {
                    let mut bytes = Vec::new();
                    bytes.try_reserve_exact(payload_bytes).map_err(|_| failure("Case body allocation refused"))?;
                    if bytes.capacity() > payload_bytes {
                        return Err(failure("Case body exceeded the reserved extent"));
                    }
                    bytes.extend_from_slice(raw);
                    bytes
                }
                None => encode_exact_case_material_v2(tx, payload_bytes)?,
            },
            aux: vec![CaseJournalAux {
                material: boundary.material(),
                preimage: encode_exact_case_material_v2(&boundary, ASSIGNED_CASE_BOUNDARY_BYTES_V2)?,
            }],
        };
        self.write_and_emit_positive_case_material_v2(
            &record,
            CaseJournalLimits {
                max_record_bytes: policy.max_case_journal_bytes,
                max_payload_bytes: policy.max_case_journal_bytes,
                max_aux_entries: 1,
                max_aux_preimage_bytes: ASSIGNED_CASE_BOUNDARY_BYTES_V2 as u64,
                max_total_preimage_bytes: policy.max_case_journal_bytes,
                max_logical_heap_bytes: policy.max_case_heap_bytes,
            },
        )
    }

    fn write_and_emit_positive_case_material_v2(
        &mut self,
        record: &misaka_palw_positive_contract::journal::CaseJournalRecord,
        limits: misaka_palw_positive_contract::journal::CaseJournalLimits,
    ) -> Result<(), PalwStateV2Error> {
        use misaka_palw_positive_contract::journal::{CaseJournalEventKind, CaseJournalRecordView};
        let failure = |why| PalwStateV2Error::DeltaMismatch(why);
        let action = self.assignment_active_raw.as_ref().ok_or_else(|| failure("Case material has no actual raw context"))?;
        if record.header.origin != action.origin() || self.assignment_case_materials.len() == self.assignment_case_materials.capacity()
        {
            return Err(failure("Case material origin/capacity changed"));
        }
        match record.header.kind {
            CaseJournalEventKind::NativeBody if record.payload.as_slice() != &action.transaction().payload[3..] => {
                return Err(failure("Case NativeBody differs from literal actual op"));
            }
            CaseJournalEventKind::NativeTransaction => {
                struct Compare<'a> {
                    expected: &'a [u8],
                    at: usize,
                }
                impl std::io::Write for Compare<'_> {
                    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                        let end = self.at.checked_add(bytes.len()).ok_or(std::io::ErrorKind::InvalidData)?;
                        if self.expected.get(self.at..end) != Some(bytes) {
                            return Err(std::io::ErrorKind::InvalidData.into());
                        }
                        self.at = end;
                        Ok(bytes.len())
                    }
                    fn flush(&mut self) -> std::io::Result<()> {
                        Ok(())
                    }
                }
                let mut compare = Compare { expected: &record.payload, at: 0 };
                borsh::to_writer(&mut compare, action.transaction())
                    .map_err(|_| failure("Case NativeTransaction is not actual full carrier"))?;
                if compare.at != record.payload.len() {
                    return Err(failure("Case native tx has trailing bytes"));
                }
            }
            _ => {}
        }
        let bytes = borsh::object_length(record).map_err(|_| failure("Case record canonical length"))? as u64;
        let requests = bytes.checked_mul(2).ok_or_else(|| failure("Case record allocation overflow"))?;
        let mut quote = native_writers::NativeWriteQuoteV1::new("accepted literal Case record");
        quote.exact::<u8>(requests)?;
        self.admit_native_write_v1(quote)?;
        let raw = record.encode(limits).map_err(|_| failure("Case record encode/limits"))?;
        let view = CaseJournalRecordView::decode(&raw, limits).map_err(|_| failure("Case record changed after encode"))?;
        if self
            .assignment_case_materials
            .iter()
            .any(|r| r.header.case == record.header.case && r.header.sequence == record.header.sequence)
        {
            return Err(failure("Case material emitted twice"));
        }
        self.write_assignment_aux_v2(NativeRowBody::JournalHead(misaka_palw_positive_contract::rows::JournalHeadRow {
            case: record.header.case,
            sequence: record.header.sequence,
            previous: record.header.previous,
            head: view.head(),
            history: view.history(),
        }))?;
        self.assignment_case_materials.push(PositiveCaseAcceptedMaterialV2 {
            header: record.header,
            raw,
            limits,
            delta_end: self.entries.len(),
        });
        Ok(())
    }
}

impl TransitionBuilder<'_> {
    /// Exact DATA recipe for the active actual raw event. The private Case
    /// factory must compare this with independently loaded native lineage.
    pub(super) fn current_positive_case_boundary_v2(
        &self,
        case: misaka_palw_positive_contract::identity::CaseId,
    ) -> Result<crate::palw_positive_assignment_case_material_v2::AssignmentCaseBoundaryV2, PalwStateV2Error> {
        let failure = |why| PalwStateV2Error::DeltaMismatch(why);
        let action = self.assignment_active_raw.as_ref().ok_or_else(|| failure("Case boundary has no active native action"))?;
        let (root, end) = self.assignment_raw_boundary.ok_or_else(|| failure("Case boundary has no actual executed prefix"))?;
        crate::palw_positive_assignment_case_material_v2::AssignmentCaseBoundaryV2::from_actual_data_v2(
            case,
            action.origin(),
            action.selected_parent(),
            root,
            end,
            action.entries(),
            action.fee(),
        )
        .map_err(|_| failure("actual Case boundary cannot be canonically encoded"))
    }
}

impl TransitionBuilder<'_> {
    pub(super) fn current_assignment_control_boundary_v2(
        &self,
        assignment: misaka_palw_positive_contract::assignment::AssignmentId,
    ) -> Result<AssignmentControlBoundaryV2, PalwStateV2Error> {
        let failure = |why| PalwStateV2Error::DeltaMismatch(why);
        let action = self.assignment_active_raw.as_ref().ok_or_else(|| failure("Assignment boundary has no active actual action"))?;
        let (root, end) = self.assignment_raw_boundary.ok_or_else(|| failure("Assignment boundary has no executed prefix"))?;
        AssignmentControlBoundaryV2::from_actual_data_v2(
            assignment,
            action.origin(),
            action.selected_parent(),
            root,
            end,
            action.entries(),
            action.fee(),
        )
        .map_err(|_| failure("Assignment boundary canonical data"))
    }
    /// Must run BEFORE constructing the frame whose payload digest binds the
    /// full envelope; emitting a wrapper only after hashing is forbidden.
    pub(super) fn encode_assignment_control_payload_v2(
        &mut self,
        assignment: misaka_palw_positive_contract::assignment::AssignmentId,
        inner: &[u8],
        max_payload: u64,
    ) -> Result<Vec<u8>, PalwStateV2Error> {
        use misaka_palw_positive_contract::assignment::control_material::{
            ASSIGNMENT_CONTROL_OVERHEAD_BYTES_V2, encode_assignment_control_v2,
        };
        let size = inner
            .len()
            .checked_add(ASSIGNMENT_CONTROL_OVERHEAD_BYTES_V2)
            .ok_or(PalwStateV2Error::Overflow("Assignment control envelope"))?;
        if max_payload == 0 || max_payload == u64::MAX || size as u64 > max_payload {
            return Err(PalwStateV2Error::DeltaMismatch("Assignment native control envelope exceeds original cap"));
        }
        let boundary = self
            .current_assignment_control_boundary_v2(assignment)?
            .canonical_bytes()
            .map_err(|_| PalwStateV2Error::DeltaMismatch("Assignment boundary canonical bytes"))?;
        let mut quote = native_writers::NativeWriteQuoteV1::new("Assignment native control envelope");
        quote.exact::<u8>(size as u64)?;
        self.admit_native_write_v1(quote)?;
        encode_assignment_control_v2(&boundary, inner, max_payload)
            .map_err(|_| PalwStateV2Error::DeltaMismatch("Assignment control envelope encoding"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_positive_contract::{
        assignment::AssignmentId,
        identity::{CaseId, Digest, ResultId},
        native::NativeRowKey,
        rows::{AcceptedMaterial, AcceptedOrigin, NativeHash64},
    };

    #[test]
    fn first_source_receipt_cannot_publish_without_atomic_response_journal() {
        use misaka_palw_positive_contract::{identity::PrincipalId, messages::SourceKind, rows::ServiceReceiptRow};
        let origin = AcceptedOrigin {
            accepting_block: NativeHash64([1; 64]),
            carrier_block: NativeHash64([2; 64]),
            carrier_transaction: NativeHash64([3; 64]),
            accepted_tx_ordinal: 1,
            event_ordinal: 0,
            accepted_daa: 10,
            accepted_blue_score: 11,
        };
        let response = AcceptedMaterial { origin, digest: Digest(origin.carrier_transaction.0) };
        let body = NativeRowBody::ServiceReceipt(ServiceReceiptRow {
            case: CaseId(Digest([4; 64])),
            kind: SourceKind::Input,
            source: Digest([5; 64]),
            unit: 0,
            result: ResultId(Digest([6; 64])),
            duty: AcceptedMaterial { origin: AcceptedOrigin { accepted_daa: 9, ..origin }, digest: Digest([7; 64]) },
            response,
            provider: PrincipalId(Digest([8; 64])),
            bytes: 1,
            additional_work: 1,
            settlement: None,
        });
        let key = body.key().encode();
        let entry = PalwDeltaEntryV2::KernelRouteRow {
            table: KERNEL_AUX_TABLE,
            key,
            old: None,
            new: Some(NativeRowValue::new(body).unwrap().encode().unwrap()),
        };
        assert!(matches!(
            PositiveAssignmentMaterialOutcomeV2::default().validate_v2(&[entry]),
            Err(PalwStateV2Error::DeltaMismatch("first source receipt has no atomic response journal"))
        ));
    }

    #[test]
    fn orphan_assignment_consumption_cannot_publish_without_its_case_record() {
        let origin = AcceptedOrigin {
            accepting_block: NativeHash64([1; 64]),
            carrier_block: NativeHash64([2; 64]),
            carrier_transaction: NativeHash64([3; 64]),
            accepted_tx_ordinal: 1,
            event_ordinal: 0,
            accepted_daa: 10,
            accepted_blue_score: 11,
        };
        let consumed = AssignmentUseRow {
            assignment: AssignmentId(Digest([4; 64])),
            case: CaseId(Digest([5; 64])),
            result: ResultId(Digest([6; 64])),
            origin,
            binding: AcceptedMaterial { origin, digest: Digest([7; 64]) },
        };
        let delta = PalwDeltaEntryV2::KernelRouteRow {
            table: KERNEL_AUX_TABLE,
            key: NativeRowKey::AssignmentUse(consumed.assignment).encode(),
            old: None,
            new: Some(NativeRowValue::new(NativeRowBody::AssignmentUse(consumed)).unwrap().encode().unwrap()),
        };
        assert!(matches!(
            PositiveAssignmentMaterialOutcomeV2::default().validate_v2(&[delta]),
            Err(PalwStateV2Error::DeltaMismatch("AssignmentUse must have exactly one atomic initial Case record"))
        ));
    }
}

#[cfg(test)]
mod settlement_tests {
    use super::*;
    use crate::palw_positive_assignment_funds_v2::{qualify_assignment_spend_v2, tentative_tests::fixture};
    use misaka_palw_positive_contract::{
        assignment::control_material::{ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2, encode_assignment_control_v2},
        identity::{Digest, PUBKEY_BYTES},
        native::NativeRowKey,
        rows::NativeHash64,
    };

    #[test]
    fn assignment_utxo_debit_requires_matching_rooted_settlement() {
        let (funding, tx, _, auth) = fixture(&[17; PUBKEY_BYTES]);
        let plan = qualify_assignment_spend_v2(&funding, &auth, &tx, &[175, 10], 20).unwrap();
        let origin = misaka_palw_positive_contract::rows::AcceptedOrigin {
            accepting_block: NativeHash64([7; 64]),
            carrier_block: NativeHash64([8; 64]),
            carrier_transaction: NativeHash64(tx.id().as_bytes()),
            accepted_tx_ordinal: 1,
            event_ordinal: 0,
            accepted_daa: 20,
            accepted_blue_score: 20,
        };
        let payload =
            encode_assignment_control_v2(&[9; ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2], &tx.payload[3..], auth.max_frame_bytes).unwrap();
        let frame = assignment_frame_v2(
            funding.assignment,
            1,
            Digest([8; 64]),
            AssignmentJournalEventKindV2::Settlement,
            origin,
            &payload,
            auth.max_frame_bytes,
        )
        .unwrap();
        let new = plan.accepted_funding_row_v2(&frame, &payload).unwrap();
        let key = NativeRowKey::AssignmentFunding(funding.assignment).encode();
        let encode = |row| NativeRowValue::new(NativeRowBody::AssignmentFunding(row)).unwrap().encode().unwrap();
        let mut entries = vec![PalwDeltaEntryV2::KernelRouteRow {
            table: KERNEL_AUX_TABLE,
            key,
            old: Some(encode(funding)),
            new: Some(encode(new.clone())),
        }];
        let max_record_bytes = ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64 + auth.max_frame_bytes;
        let mut material = PositiveAssignmentMaterialOutcomeV2 {
            records: vec![PositiveAssignmentAcceptedMaterialV2 {
                header: frame.header,
                raw: encode_assignment_journal_record_v2(frame.header, &payload, max_record_bytes).unwrap(),
                max_record_bytes,
                delta_end: 1,
            }],
            case_records: Vec::new(),
        };
        let plans = std::slice::from_ref(&plan);
        assert!(material.check_utxo_settlements_v2(&entries, plans).is_ok());
        assert!(material.check_utxo_settlements_v2(&entries, &[]).is_err());
        assert!(PositiveAssignmentMaterialOutcomeV2::default().check_utxo_settlements_v2(&entries, plans).is_err());
        entries.push(entries[0].clone());
        material.records[0].delta_end = 2;
        assert!(material.check_utxo_settlements_v2(&entries, plans).is_err());
        material.records[0].delta_end = 1;
        let PalwDeltaEntryV2::KernelRouteRow { new: encoded, .. } = &mut entries[0] else { unreachable!() };
        let mut foreign = new;
        foreign.current_outpoint.as_mut().unwrap().index += 1;
        *encoded = Some(encode(foreign));
        assert!(material.check_utxo_settlements_v2(&entries, plans).is_err());
    }
}

#[cfg(test)]
mod source_material_tests {
    use super::*;
    use crate::palw_positive_assignment_case_material_v2::AssignmentCaseBoundaryV2;
    use misaka_palw_positive_contract::{
        identity::{Digest, PrincipalId, SIGNATURE_BYTES},
        journal::{
            CaseJournalAux, CaseJournalEventKind, CaseJournalHeader, CaseJournalLimits, CaseJournalRecord, CaseJournalRecordView,
        },
        messages::{SourceKind, SourceRequest},
        rows::{
            AcceptedMaterial, AcceptedOrigin, AggregateUsage, CaseRow, JournalHeadRow, NativeHash64, ServiceReceiptRow, SourceUnionRow,
        },
    };

    fn d(n: u8) -> Digest {
        Digest([n; 64])
    }
    fn fixture(operation: u8) -> (PositiveCaseAcceptedMaterialV2, Vec<PalwDeltaEntryV2>) {
        let (_, _, mut before, _, _, _) = crate::palw_positive_rows_v2::tests::signed_bundle();
        before.service.budget.transmitted_bytes = 10_000;
        before.service.budget.retained_bytes = 10_000;
        before.usage = AggregateUsage {
            rounds: 1,
            unique_source_bytes: 4,
            transmitted_bytes: 100,
            additional_work: 4,
            retained_bytes: 100,
            fee: 0,
            witness_attempts: 0,
        };
        let case = before.case;
        let result = before.result;
        let scope = before.coverage.scope;
        let prior_material = AcceptedMaterial { origin: before.last_event, digest: d(19) };
        let origin = AcceptedOrigin {
            accepting_block: NativeHash64([21; 64]),
            carrier_transaction: NativeHash64([23; 64]),
            event_ordinal: 0,
            accepted_daa: 21,
            accepted_blue_score: 21,
            ..before.last_event
        };
        let current = AcceptedMaterial { origin, digest: d(23) };
        let request = SourceRequest {
            version: 2,
            revision: 1,
            case,
            contract: before.service.id(),
            source: d(31),
            kind: SourceKind::Input,
            first: 0,
            end: 2,
        };
        let previous_union = SourceUnionRow {
            case,
            kind: request.kind,
            source: request.source,
            source_units: 3,
            requested_units: 1,
            completed_units: 0,
            union: prior_material,
        };
        let (after_usage, next_union, payload, receipt) = if operation == 3 {
            let mut payload = vec![2, 0, 1, 0, 3];
            payload.extend(borsh::to_vec(&(request, PrincipalId(d(33)), [0u8; SIGNATURE_BYTES])).unwrap());
            (
                AggregateUsage {
                    rounds: 2,
                    unique_source_bytes: 8,
                    transmitted_bytes: 100 + payload.len() as u64,
                    retained_bytes: 100 + payload.len() as u64,
                    additional_work: 6,
                    ..before.usage
                },
                SourceUnionRow { requested_units: 2, union: current, ..previous_union },
                payload,
                None,
            )
        } else {
            let mut payload = vec![2, 0, 1, 0, 4];
            payload.extend(case.0.0);
            payload.extend(request.id().0.0);
            payload.extend(1u32.to_le_bytes());
            payload.push(42);
            (
                AggregateUsage {
                    transmitted_bytes: 100 + payload.len() as u64,
                    retained_bytes: 100 + payload.len() as u64,
                    additional_work: 6,
                    fee: 1,
                    ..before.usage
                },
                SourceUnionRow { completed_units: 1, ..previous_union },
                payload,
                Some(ServiceReceiptRow {
                    case,
                    kind: request.kind,
                    source: request.source,
                    unit: 0,
                    result,
                    duty: prior_material,
                    response: current,
                    provider: PrincipalId(d(33)),
                    bytes: 4,
                    additional_work: 2,
                    settlement: None,
                }),
            )
        };
        let mut after = CaseRow { usage: after_usage, last_event: origin, ..before };
        let previous_head = JournalHeadRow { case, sequence: 1, previous: d(0), head: d(40), history: prior_material };
        let boundary = AssignmentCaseBoundaryV2 {
            version: 2,
            revision: 1,
            case,
            origin,
            selected_parent: NativeHash64([20; 64]),
            pre_event_root: d(41),
            before_delta_end: 0,
            populated_inputs: d(42),
            input_count: 1,
            fee: 1,
        };
        let header = CaseJournalHeader {
            version: 2,
            revision: 1,
            scope,
            case,
            result,
            sequence: 2,
            previous: previous_head.head,
            origin,
            kind: CaseJournalEventKind::NativeBody,
        };
        let limits = CaseJournalLimits {
            max_record_bytes: 10000,
            max_payload_bytes: 8000,
            max_aux_entries: 1,
            max_aux_preimage_bytes: 500,
            max_total_preimage_bytes: 10000,
            max_logical_heap_bytes: 20000,
        };
        let record = CaseJournalRecord {
            header,
            payload,
            aux: vec![CaseJournalAux { material: boundary.material(), preimage: borsh::to_vec(&boundary).unwrap() }],
        };
        let raw = record.encode(limits).unwrap();
        after.usage.retained_bytes = before.usage.retained_bytes + raw.len() as u64;
        let view = CaseJournalRecordView::decode(&raw, limits).unwrap();
        let next_head = JournalHeadRow { case, sequence: 2, previous: previous_head.head, head: view.head(), history: view.history() };
        let entry = |old: Option<NativeRowBody>, new: NativeRowBody| {
            let key = new.key().encode();
            let encode = |body| NativeRowValue::new(body).unwrap().encode().unwrap();
            PalwDeltaEntryV2::KernelRouteRow { table: KERNEL_AUX_TABLE, key, old: old.map(encode), new: Some(encode(new)) }
        };
        let mut delta = vec![
            entry(Some(NativeRowBody::Case(before)), NativeRowBody::Case(after)),
            entry(Some(NativeRowBody::SourceUnion(previous_union)), NativeRowBody::SourceUnion(next_union)),
        ];
        if let Some(receipt) = receipt {
            delta.push(entry(None, NativeRowBody::ServiceReceipt(receipt)));
        }
        delta.push(entry(Some(NativeRowBody::JournalHead(previous_head)), NativeRowBody::JournalHead(next_head)));
        (PositiveCaseAcceptedMaterialV2 { header, raw, limits, delta_end: delta.len() }, delta)
    }
    #[test]
    fn request_and_first_receipt_require_their_atomic_case_mutations() {
        for operation in [3, 4] {
            let (record, delta) = fixture(operation);
            let records = [record];
            assert_eq!(validate_case_materials_v2(&records, &[], &delta), Ok(()), "op {operation}");
            let mut absent = delta.clone();
            absent.remove(if operation == 3 { 1 } else { 2 });
            let mut absent_record = fixture(operation).0;
            absent_record.delta_end = absent.len();
            assert!(validate_case_materials_v2(&[absent_record], &[], &absent).is_err());
            let mut orphan = delta.clone();
            orphan.push(delta[1].clone());
            assert!(validate_case_materials_v2(&records, &[], &orphan).is_err());
            assert!(validate_case_materials_v2(&[], &[], &[delta[1].clone()]).is_err());
            let view = CaseJournalRecordView::decode(&records[0].raw, records[0].limits).unwrap();
            assert!(validate_source_case_material_v2(view, operation, &delta, records[0].raw.len() as u64 + 1).is_err());
            let mut wrong_case = delta.clone();
            let PalwDeltaEntryV2::KernelRouteRow { key, new: Some(raw), .. } = &mut wrong_case[0] else {
                panic!("fixture begins with a Case row");
            };
            let mut value = NativeRowValue::decode(key, raw).unwrap();
            let NativeRowBody::Case(case) = &mut value.body else { panic!("fixture Case row") };
            case.usage.transmitted_bytes -= 1;
            *raw = value.encode().unwrap();
            assert!(validate_case_materials_v2(&records, &[], &wrong_case).is_err());
        }
    }
}
