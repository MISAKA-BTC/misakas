//! Ephemeral accepted-material output, separate from canonical state/delta.
//! Only the native checkpoint writer creates records. It retains typed buffer
//! reservations until the processor stages the SAME delta and UTXO WriteBatch.
use super::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, NativeRetainedV1};
use misaka_palw_positive_contract::{
    assignment::{
        journal::*,
        open_material::{AssignmentOpenLimitsV2, AssignmentOpenViewV2},
        rows::AssignmentJournalFrameRow,
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
        validate_case_materials_v2(&self.case_records, entries)?;
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
    entries: &[PalwDeltaEntryV2],
) -> Result<(), PalwStateV2Error> {
    use misaka_palw_positive_contract::journal::CaseJournalRecordView;
    let failure = |why| PalwStateV2Error::DeltaMismatch(why);
    for record in records {
        let view = CaseJournalRecordView::decode(&record.raw, record.limits).map_err(|_| failure("accepted Case material changed"))?;
        if view.header() != record.header || record.delta_end > entries.len() {
            return Err(failure("Case material header/watermark changed"));
        }
        let mut case = false;
        let mut head = false;
        for entry in entries.iter().take(record.delta_end) {
            let PalwDeltaEntryV2::KernelRouteRow { table, key, new: Some(bytes), .. } = entry else {
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
                _ => {}
            }
        }
        if !case || !head {
            return Err(failure("Case material missing atomic Case/head mutation"));
        }
    }
    for entry in entries {
        let PalwDeltaEntryV2::KernelRouteRow { table, key, new: Some(bytes), .. } = entry else {
            continue;
        };
        if *table != KERNEL_AUX_TABLE {
            continue;
        }
        if let NativeRowBody::JournalHead(h) = NativeRowValue::decode(key, bytes).map_err(|_| failure("invalid Case head delta"))?.body
        {
            let mut count = 0;
            for record in records {
                let view = CaseJournalRecordView::decode(&record.raw, record.limits).map_err(|_| failure("Case material decode"))?;
                if view.head() == h.head && view.history() == h.history {
                    count += 1;
                }
            }
            if count != 1 {
                return Err(failure("Case head must have exactly one retained native material record"));
            }
        }
    }
    Ok(())
}
impl TransitionBuilder<'_> {
    pub(super) fn unpublished_positive_case_materials_v2(
        &self,
        case: misaka_palw_positive_contract::identity::CaseId,
    ) -> impl Iterator<Item = &PositiveCaseAcceptedMaterialV2> {
        self.assignment_case_materials.iter().filter(move |r| r.header.case == case)
    }
    pub(super) fn emit_positive_case_material_v2(
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
        let _head = view.head();
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
