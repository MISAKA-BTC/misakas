//! Private concrete archive loading from accepted native storage. The journal
//! supplies DATA identifiers; original acceptance/header/delta/UTXO stores must
//! independently resolve them. Missing/pruned originals never become empty
//! history or the current P state.
use super::*;
use crate::model::stores::{
    acceptance_data::AcceptanceArchiveViewV2, block_transactions::ArchivedTransactionV2, utxo_diffs::UtxoArchiveViewV2,
};
use crate::{
    model::stores::{ghostdag::GhostdagStoreReader, headers::HeaderStoreReader},
    processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags,
};
use kaspa_consensus_core::{
    palw_native_resource_v1::{
        NativeChainStateV1, NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, NativeStateDeltaV1,
        native_delta_replay_requests_v1, reserve_native_requests_v1,
    },
    palw_positive_assignment_case_material_v2::{AssignmentCaseBoundaryV2, assigned_case_native_carrier_matches_v2},
    palw_positive_assignment_material_v2::{AssignmentAdmissionMaterialV2, AssignmentAdmissionMaterialViewV2},
    palw_positive_funding_relay_v2::{pending_funding_relay_payer_scratch_v2, pending_funding_relay_row_images_match_v2},
    palw_positive_journal_chain_v2::{
        CaseJournalChainErrorV2, CaseJournalChainLimitsV2, CaseJournalLinkQuoteV2, CaseJournalPinV2, CaseJournalVisitErrorV2,
        CheckedCaseJournalLinksV2, check_case_journal_links_v2,
    },
    palw_positive_rows_v2::{PositiveRowErrorV2, PositiveRowsV2},
    palw_state_v2::{
        AssignmentControlBoundaryV2, NativeAcceptedCaseCarriageV2, NativeAssignedCaseBackingErrorV2, NativeAssignmentRulesV2,
        NativeCommittedAssignmentRecordV2, NativeCommittedCaseRecordV2, PalwBondKeyV2, PalwBondStateV2, PalwChainStateV2,
        PalwDeltaEntryV2, palw_bond_may_take_work_v2,
    },
    tx::{PopulatedTransaction, TransactionOutpoint, UtxoEntry},
};
use kaspa_database::prelude::StoreError;
use misaka_palw_positive_contract::{
    assignment::{
        AssignmentId,
        control_material::AssignmentControlViewV2,
        funding_relay::{FundingRelayTermsV2, RelayTargetV2},
        funding_relay_policy::FundingRelayPhysicalProfileV2,
        funding_relay_rows::{FundingRelayAuthorizationRowV2, FundingRelayCustodyRowV2},
        journal::{ASSIGNMENT_JOURNAL_HEADER_BYTES_V2, AssignmentJournalEventKindV2, AssignmentJournalRecordViewV2},
        open_material::AssignmentOpenLimitsV2,
        rows::{AssignmentJournalFrameRow, AssignmentJournalHeadRow, AssignmentNonceRow},
    },
    identity::Digest,
    journal::CaseJournalRecordView,
    native::{KERNEL_AUX_TABLE, NativeRowKey},
    rows::{AcceptedMaterial, AcceptedOrigin, MAX_NATIVE_KEY_BYTES, MAX_NATIVE_ROW_BYTES, NativeRowBody, NativeRowValue},
};

/// Private concrete boundary selection. This is not a successful replay or
/// arbitrary provider interface. Assignment and Case subjects stay distinct.
enum NativeArchiveBoundaryV2 {
    Assignment(AssignmentControlBoundaryV2),
    Case(AssignmentCaseBoundaryV2),
}
impl NativeArchiveBoundaryV2 {
    fn fields(&self) -> (AcceptedOrigin, [u8; 64], [u8; 64], u64, u64) {
        match self {
            Self::Assignment(b) => (b.origin, b.selected_parent.0, b.pre_event_root.0, b.before_delta_end, b.fee),
            Self::Case(b) => (b.origin, b.selected_parent.0, b.pre_event_root.0, b.before_delta_end, b.fee),
        }
    }
    fn check_populated_data(&self, tx: &kaspa_consensus_core::tx::Transaction, entries: &[UtxoEntry], fee: u64) -> bool {
        match self {
            Self::Assignment(b) => b.check_populated_data(tx, entries, fee).is_ok(),
            Self::Case(b) => b.check_populated_data(tx, entries, fee).is_ok(),
        }
    }
}

#[derive(Debug)]
pub(in crate::pipeline::virtual_processor) enum NativeAssignmentArchiveErrorV2 {
    Unavailable(&'static str),
    InvalidContinuation,
    Invalid(&'static str),
    Resource(NativeCoverageV1),
}

fn archive_store_fault_v2(error: StoreError, component: &'static str) -> NativeAssignmentArchiveErrorV2 {
    use NativeAssignmentArchiveErrorV2 as E;
    match error {
        StoreError::DataInconsistency(_) | StoreError::DeserializationError(_) => E::Invalid(component),
        StoreError::LocalResourceUnavailable(why) => {
            E::Resource(NativeCoverageV1::unsupported::<VirtualStateProcessor>(component, &why))
        }
        _ => E::Unavailable(component),
    }
}

/// Exact journal bytes in replay order. A native carriage owner must still
/// authenticate the accepting transaction, fee, prefix and event effects.
struct NativeCaseJournalBytesV2 {
    records: Vec<Vec<u8>>,
    work: u64,
    heap: u64,
    lease: NativeDemandLeaseV1,
}

pub(in crate::pipeline::virtual_processor) struct NativeCaseJournalCarriagesV2 {
    pub records: Vec<NativeCommittedCaseRecordV2>,
    pub work: u64,
    pub heap: u64,
    pub lease: NativeDemandLeaseV1,
}

pub(in crate::pipeline::virtual_processor) struct NativeAssignmentJournalCarriagesV2 {
    pub open: NativeCommittedAssignmentRecordV2,
    pub records: Vec<NativeCommittedAssignmentRecordV2>,
    pub work: u64,
    pub heap: u64,
    pub lease: NativeDemandLeaseV1,
}

struct NativeArchiveCarriageV2 {
    carriage: NativeAcceptedCaseCarriageV2,
    work: u64,
    heap: u64,
}

fn case_journal_fault_v2(error: CaseJournalChainErrorV2<StoreError>) -> NativeAssignmentArchiveErrorV2 {
    use NativeAssignmentArchiveErrorV2 as E;
    match error {
        CaseJournalChainErrorV2::Source(error) => archive_store_fault_v2(error, "rooted Case journal read"),
        CaseJournalChainErrorV2::Invalid(_) => E::Invalid("rooted Case journal identity or links"),
        CaseJournalChainErrorV2::LocalBudget(_) => E::Unavailable("rooted Case journal exceeds its original replay limits"),
    }
}

fn case_journal_retained_quote_v2(quote: CaseJournalLinkQuoteV2) -> Option<(u64, u64)> {
    let retained = quote
        .events
        .checked_mul(size_of::<Vec<u8>>() as u64)?
        .checked_add(quote.total_record_bytes / 2)?
        .checked_add(size_of::<NativeCaseJournalBytesV2>() as u64)?;
    let work = quote
        .record_reads
        .checked_add(quote.total_record_bytes)?
        .checked_add(quote.hash_bytes)?
        .checked_add(quote.total_record_bytes / 2)?;
    Some((retained, work))
}

/// The original relay registration pays for its bounded historical reads.
pub(in crate::pipeline::virtual_processor) struct RelayCarrierReplayV2<'a> {
    pub origin: AcceptedOrigin,
    pub index: u32,
    pub matches_payload: &'a dyn Fn(&[u8]) -> bool,
    pub gross_fee: u64,
    pub remaining_reads: u64,
    pub remaining_work: u64,
    pub maximum_heap: u64,
    pub acceptance_bytes: u64,
    pub selected_parent: BlockHash,
    pub state: &'a PalwChainStateV2,
    pub authorization: &'a FundingRelayAuthorizationRowV2,
    pub custody: &'a FundingRelayCustodyRowV2,
    pub case_contract_material: Option<AcceptedMaterial>,
}

impl RelayCarrierReplayV2<'_> {
    fn charge(&mut self, reads: u64, work: u64) -> Result<(), NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let remaining_reads = self.remaining_reads.checked_sub(reads).ok_or(E::Unavailable("relay replay read cap"))?;
        let remaining_work = self.remaining_work.checked_sub(work).ok_or(E::Unavailable("relay replay work cap"))?;
        self.remaining_reads = remaining_reads;
        self.remaining_work = remaining_work;
        Ok(())
    }
}

/// Mutable projection of the one Case contract row, limited to the aux
/// codec's maximum extent. Delta rows remain borrowed during comparison.
struct RelayCaseHistoryRowV2 {
    bytes: Vec<u8>,
    present: bool,
}

impl RelayCaseHistoryRowV2 {
    fn new(raw: Option<&[u8]>) -> Result<Self, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let mut row = Self { bytes: Vec::new(), present: false };
        row.bytes.try_reserve_exact(MAX_NATIVE_ROW_BYTES).map_err(|_| E::Unavailable("Case relay row allocation"))?;
        if row.bytes.capacity() > MAX_NATIVE_ROW_BYTES {
            return Err(E::Unavailable("Case relay row allocation exceeds lease"));
        }
        row.replace(raw)?;
        Ok(row)
    }

    fn raw(&self) -> Option<&[u8]> {
        self.present.then_some(self.bytes.as_slice())
    }

    fn replace(&mut self, raw: Option<&[u8]>) -> Result<(), NativeAssignmentArchiveErrorV2> {
        let bytes = raw.unwrap_or_default();
        if bytes.len() > MAX_NATIVE_ROW_BYTES {
            return Err(NativeAssignmentArchiveErrorV2::Invalid("Case relay Assignment row extent"));
        }
        self.bytes.clear();
        self.bytes.extend_from_slice(bytes);
        self.present = raw.is_some();
        Ok(())
    }
}

const ARCHIVE_WORK_PER_DELTA_BYTE_V2: u64 = 6;

fn charge_archive_work_v2(remaining: &mut u64, amount: u64) -> Result<(), NativeAssignmentArchiveErrorV2> {
    *remaining = remaining.checked_sub(amount).ok_or(NativeAssignmentArchiveErrorV2::Unavailable("native archive replay work cap"))?;
    Ok(())
}

fn original_input_replay_work_v2(acceptance_bytes: u64, inputs: u64) -> Result<u64, NativeAssignmentArchiveErrorV2> {
    // Each input may walk the pinned record twice. Count empty mergeset
    // blocks as well as accepted entries, plus the duplicate input check.
    acceptance_bytes
        .checked_mul(2)
        .and_then(|n| n.checked_add(inputs))
        .and_then(|n| n.checked_mul(inputs))
        .ok_or(NativeAssignmentArchiveErrorV2::Invalid("native input replay work overflow"))
}

fn utxo_diff_scans_v2(inputs: u64) -> Result<u64, NativeAssignmentArchiveErrorV2> {
    // Validate once, then price both passes for every requested outpoint.
    inputs.checked_mul(2).and_then(|n| n.checked_add(1)).ok_or(NativeAssignmentArchiveErrorV2::Invalid("UTXO replay work overflow"))
}

fn exact_case_boundary_v2(record: CaseJournalRecordView<'_>) -> Result<AssignmentCaseBoundaryV2, NativeAssignmentArchiveErrorV2> {
    use NativeAssignmentArchiveErrorV2 as E;
    let header = record.header();
    let mut found = None;
    for aux in record.aux() {
        if let Ok(boundary) = AssignmentCaseBoundaryV2::decode(aux.preimage())
            && boundary.material() == aux.material()
        {
            if boundary.case != header.case || boundary.origin != header.origin {
                return Err(E::Invalid("Case boundary scope/origin differs from literal record"));
            }
            if found.replace(boundary).is_some() {
                return Err(E::Invalid("duplicate actual Case boundary"));
            }
        }
    }
    found.ok_or(E::Invalid("missing actual Case boundary"))
}

fn assignment_archive_row_fault_v2(error: PositiveRowErrorV2) -> NativeAssignmentArchiveErrorV2 {
    use NativeAssignmentArchiveErrorV2 as E;
    match error {
        PositiveRowErrorV2::MissingKernelRoute | PositiveRowErrorV2::MissingRequiredRow => {
            E::Unavailable("required rooted Assignment archive row")
        }
        PositiveRowErrorV2::Contract(misaka_palw_positive_contract::ContractError::Budget) => {
            E::Unavailable("rooted Assignment archive exceeds original caps")
        }
        _ => E::Invalid("rooted Assignment archive rows or links"),
    }
}

fn check_assignment_predecessor_v2(
    prefix: &PalwChainStateV2,
    assignment: AssignmentId,
    expected: Option<AssignmentJournalHeadRow>,
) -> Result<(), NativeAssignmentArchiveErrorV2> {
    use NativeAssignmentArchiveErrorV2 as E;
    let rows = PositiveRowsV2::new(prefix);
    let actual =
        rows.read(NativeRowKey::AssignmentJournalHead(assignment)).map_err(assignment_archive_row_fault_v2)?.map(|row| row.body);
    match (actual, expected) {
        (Some(NativeRowBody::AssignmentJournalHead(head)), Some(expected)) if head == expected => Ok(()),
        (None, None) if rows.read(NativeRowKey::Assignment(assignment)).map_err(assignment_archive_row_fault_v2)?.is_none() => Ok(()),
        _ => Err(E::Invalid("Assignment original prefix does not have the expected predecessor")),
    }
}

impl VirtualStateProcessor {
    /// Walk a rooted Case head backwards and copy its verified raw records in
    /// execution order. Both reads are budgeted before hashing. Release the
    /// store lock before any caller reconstructs the archived native prefixes.
    fn load_rooted_case_journal_bytes_v2(
        &self,
        pin: CaseJournalPinV2,
        mut limits: CaseJournalChainLimitsV2,
        maximum_heap: u64,
        maximum_work: u64,
    ) -> Result<NativeCaseJournalBytesV2, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        if [maximum_heap, maximum_work].into_iter().any(|n| n == 0 || n == u64::MAX) {
            return Err(E::Invalid("Case journal limits must be finite"));
        }
        if pin.head.sequence == 0 || pin.head.sequence > limits.max_events {
            return Err(E::Invalid("Case journal head sequence is outside its original bound"));
        }
        let reads = pin.head.sequence.checked_mul(2).ok_or(E::Invalid("Case journal read count overflow"))?;
        // Two reads, four hash passes and one retained copy per unique byte.
        // Cap the shared checker's two-pass raw input before the first lookup.
        let unique_work = maximum_work.checked_sub(reads).ok_or(E::Unavailable("Case journal read work cap"))? / 7;
        let byte_work = unique_work * 2;
        limits.max_total_record_bytes = limits.max_total_record_bytes.min(byte_work);
        let index = pin.head.sequence.checked_mul(size_of::<Digest>() as u64).ok_or(E::Invalid("Case journal index overflow"))?;
        limits.max_index_bytes = limits.max_index_bytes.min(index);
        let fixed = size_of::<CheckedCaseJournalLinksV2>() as u64
            + kaspa_consensus_core::palw_native_resource_v1::positive_journal::POSITIVE_JOURNAL_DECODE_WORKSPACE_BYTES_V1 as u64;
        let scratch = index.checked_add(fixed).ok_or(E::Invalid("Case journal workspace overflow"))?;
        let remainder = maximum_heap.checked_sub(scratch).ok_or(E::Unavailable("Case journal index heap cap"))?;
        let _scratch = reserve_native_requests_v1(scratch).map_err(E::Resource)?.ok_or(E::Unavailable("Case journal index lease"))?;
        let store = self.palw_state_v2_store.read();
        let checked = check_case_journal_links_v2(&*store, pin, limits).map_err(case_journal_fault_v2)?;
        let (retained, work) =
            case_journal_retained_quote_v2(checked.quote()).ok_or(E::Invalid("Case journal retained quote overflow"))?;
        if retained > remainder || work > maximum_work {
            return Err(E::Unavailable("Case journal retained heap or replay work cap"));
        }
        let lease =
            reserve_native_requests_v1(retained).map_err(E::Resource)?.ok_or(E::Unavailable("Case journal raw records lease"))?;
        let count = usize::try_from(pin.head.sequence).map_err(|_| E::Invalid("Case journal count address space"))?;
        let mut records = Vec::new();
        records.try_reserve_exact(count).map_err(|_| E::Unavailable("Case journal record vector allocation"))?;
        if records.capacity() > count {
            return Err(E::Unavailable("Case journal record vector exceeded its lease"));
        }
        checked
            .visit_forward(&*store, |record| {
                let raw = record.raw_bytes();
                let mut copy = Vec::new();
                copy.try_reserve_exact(raw.len()).map_err(|_| E::Unavailable("Case journal raw record allocation"))?;
                if copy.capacity() > raw.len() {
                    return Err(E::Unavailable("Case journal raw record allocation exceeded its lease"));
                }
                copy.extend_from_slice(raw);
                records.push(copy);
                Ok(())
            })
            .map_err(|error| match error {
                CaseJournalVisitErrorV2::Chain(error) => case_journal_fault_v2(error),
                CaseJournalVisitErrorV2::Visitor(error) => error,
            })?;
        Ok(NativeCaseJournalBytesV2 { records, work, heap: retained, lease })
    }

    /// Resolve the exact rooted records against original native acceptance.
    /// The caller derives the pin and limits from the rooted Assignment/Case.
    /// This returns retained data for the Case factory, without a verdict.
    pub(in crate::pipeline::virtual_processor) fn load_rooted_case_journal_carriages_v2(
        &self,
        parent: BlockHash,
        state: &PalwChainStateV2,
        pin: CaseJournalPinV2,
        limits: CaseJournalChainLimitsV2,
        maximum_heap: u64,
        maximum_work: u64,
    ) -> Result<NativeCaseJournalCarriagesV2, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let NativeCaseJournalBytesV2 { records: raw_records, work, heap, mut lease } =
            self.load_rooted_case_journal_bytes_v2(pin, limits, maximum_heap, maximum_work)?;
        let count = raw_records.len();
        let output_heap = (count as u64)
            .checked_mul(size_of::<NativeCommittedCaseRecordV2>() as u64)
            .and_then(|bytes| bytes.checked_add(size_of::<NativeCaseJournalCarriagesV2>() as u64))
            .ok_or(E::Invalid("Case carriage vector heap overflow"))?;
        let mut remaining_heap = maximum_heap
            .checked_sub(heap)
            .and_then(|n| n.checked_sub(output_heap))
            .ok_or(E::Unavailable("Case carriage vector heap cap"))?;
        let mut remaining_work = maximum_work.checked_sub(work).ok_or(E::Unavailable("Case carriage replay work cap"))?;
        let scratch =
            kaspa_consensus_core::palw_native_resource_v1::positive_journal::POSITIVE_JOURNAL_DECODE_WORKSPACE_BYTES_V1 as u64;
        remaining_heap = remaining_heap.checked_sub(scratch).ok_or(E::Unavailable("Case carriage decode workspace cap"))?;
        let _scratch =
            reserve_native_requests_v1(scratch).map_err(E::Resource)?.ok_or(E::Unavailable("Case carriage decode workspace lease"))?;
        lease.grow_memory_v1(output_heap).map_err(E::Resource)?;
        let mut records = Vec::new();
        records.try_reserve_exact(count).map_err(|_| E::Unavailable("Case carriage vector allocation"))?;
        if records.capacity() > count {
            return Err(E::Unavailable("Case carriage vector exceeded its lease"));
        }
        for raw in raw_records {
            // Bound framing, aux boundary decoding/hashing and comparison of
            // the literal journal payload to the accepted transaction.
            let scan_work = (raw.len() as u64).checked_mul(4).ok_or(E::Invalid("Case carriage scan work overflow"))?;
            charge_archive_work_v2(&mut remaining_work, scan_work)?;
            let record =
                CaseJournalRecordView::decode(&raw, limits.record_limits).map_err(|_| E::Invalid("Case journal record framing"))?;
            let boundary = exact_case_boundary_v2(record)?;
            let accepted = self.load_positive_accepted_carriage_v2(
                parent,
                state,
                NativeArchiveBoundaryV2::Case(boundary),
                remaining_heap,
                remaining_work,
            )?;
            if !assigned_case_native_carrier_matches_v2(accepted.carriage.transaction(), record) {
                return Err(E::Invalid("original actual Case carrier differs from journal"));
            }
            charge_archive_work_v2(&mut remaining_work, accepted.work)?;
            remaining_heap = remaining_heap.checked_sub(accepted.heap).ok_or(E::Unavailable("retained Case carriage heap cap"))?;
            records.push(NativeCommittedCaseRecordV2::from_native_history_v2(raw, accepted.carriage));
        }
        Ok(NativeCaseJournalCarriagesV2 {
            records,
            work: maximum_work - remaining_work,
            heap: maximum_heap - remaining_heap - scratch,
            lease,
        })
    }

    /// Load the complete control history selected by the actual native parent.
    /// Each transaction also has to see the exact previous control head at
    /// its own executed prefix. A raw row copied from another branch cannot
    /// supply the missing acceptance or predecessor.
    pub(in crate::pipeline::virtual_processor) fn load_rooted_assignment_journal_carriages_v2(
        &self,
        parent: BlockHash,
        state: &PalwChainStateV2,
        material: &AssignmentAdmissionMaterialV2,
    ) -> Result<NativeAssignmentJournalCarriagesV2, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let id = material.assignment;
        let policy = &material.assignment_policy;
        let physical = &material.native_preimages.physical;
        let custody = &material.custody;
        let maximum_heap = custody.heap_bytes.min(physical.max_heap_bytes).min(policy.max_heap_bytes);
        let maximum_work = custody.replay_work.min(policy.max_replay_work);
        let maximum_raw = custody
            .retained_bytes
            .min(custody.replay_bytes / 2)
            .min(material.money.usage.retained_bytes)
            .min(material.money.usage.replay_bytes / 2)
            .min(policy.max_history_bytes);
        let maximum_frame = policy
            .max_frame_bytes
            .checked_add(ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64)
            .ok_or(E::Invalid("Assignment journal frame cap overflow"))?;
        let scratch = (8 * size_of::<NativeRowValue>()
            + 2 * size_of::<NativeRowKey>()
            + 2 * size_of::<AssignmentAdmissionMaterialV2>()
            + 2 * MAX_NATIVE_KEY_BYTES) as u64;
        let mut remaining_heap = maximum_heap.checked_sub(scratch).ok_or(E::Unavailable("Assignment archive workspace cap"))?;
        let _scratch =
            reserve_native_requests_v1(scratch).map_err(E::Resource)?.ok_or(E::Unavailable("Assignment archive workspace lease"))?;
        if state.last_point().map(|point| point.block) != Some(parent) {
            return Err(E::Invalid("Assignment archive state is not the selected parent"));
        }
        let rows = PositiveRowsV2::new(state);
        let rooted = rows.assignment(id).map_err(assignment_archive_row_fault_v2)?;
        material.check_row(&rooted.assignment).map_err(|_| E::Invalid("Assignment archive material differs from rooted row"))?;
        let count = rooted.journal.next_ordinal;
        if count == 0 || count > u64::from(policy.max_history_frames) {
            return Err(E::Unavailable("Assignment archive frame count outside original cap"));
        }
        // Two exact rooted frame reads per event. Price them before walking
        // the rooted headers to determine the exact retained DB byte extent.
        let frame_work = count
            .checked_mul(2 * (MAX_NATIVE_KEY_BYTES as u64 + ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64 + 64))
            .ok_or(E::Invalid("Assignment archive frame work overflow"))?;
        let mut remaining_work = maximum_work;
        charge_archive_work_v2(&mut remaining_work, frame_work)?;
        let extent = rows
            .assignment_journal_extent_v2(
                &rooted.journal,
                rooted.assignment.origin,
                count,
                maximum_frame,
                maximum_raw.min(remaining_work / 6),
            )
            .map_err(assignment_archive_row_fault_v2)?;
        // Literal read, copy, codec/hash and native body comparison are
        // covered separately from actual carrier reconstruction below.
        charge_archive_work_v2(&mut remaining_work, extent.record_bytes * 6)?;
        let retained = (count - 1)
            .checked_mul(size_of::<NativeCommittedAssignmentRecordV2>() as u64)
            .and_then(|n| n.checked_add(extent.record_bytes))
            .and_then(|n| n.checked_add(size_of::<NativeAssignmentJournalCarriagesV2>() as u64))
            .ok_or(E::Invalid("Assignment archive retained heap overflow"))?;
        remaining_heap = remaining_heap.checked_sub(retained).ok_or(E::Unavailable("Assignment archive retained heap cap"))?;
        let lease =
            reserve_native_requests_v1(retained).map_err(E::Resource)?.ok_or(E::Unavailable("Assignment archive retained lease"))?;
        let slots = usize::try_from(count - 1).map_err(|_| E::Invalid("Assignment archive address space"))?;
        let mut records = Vec::new();
        records.try_reserve_exact(slots).map_err(|_| E::Unavailable("Assignment archive record allocation"))?;
        if records.capacity() > slots {
            return Err(E::Unavailable("Assignment archive records exceeded lease"));
        }
        let mut open = None;
        let mut previous = None;
        for ordinal in 0..count {
            let frame = match rows
                .read(NativeRowKey::AssignmentJournalFrame { assignment: id, ordinal })
                .map_err(assignment_archive_row_fault_v2)?
                .map(|row| row.body)
            {
                Some(NativeRowBody::AssignmentJournalFrame(frame)) => frame,
                _ => return Err(E::Invalid("rooted Assignment frame disappeared")),
            };
            let raw = self.copy_rooted_assignment_frame_v2(frame)?;
            let view = AssignmentJournalRecordViewV2::decode(&raw, extent.largest_record_bytes)
                .map_err(|_| E::Invalid("rooted Assignment frame codec"))?;
            if view.frame() != frame {
                return Err(E::Invalid("literal Assignment frame differs from rooted frame"));
            }
            let maximum_payload = u64::from(frame.header.payload_bytes);
            let accepted =
                self.load_positive_assignment_carriage_v2(parent, state, &view, maximum_payload, remaining_heap, remaining_work)?;
            let prefix = accepted.carriage.prefix_state().map_err(|_| E::Unavailable("Assignment historical executed prefix"))?;
            check_assignment_predecessor_v2(prefix, id, previous)?;
            if ordinal == 0 {
                let envelope = AssignmentControlViewV2::decode(view.payload(), maximum_payload)
                    .map_err(|_| E::Invalid("original Assignment Open envelope"))?;
                let literal_body = &accepted.carriage.transaction().payload[3..];
                let original = AssignmentAdmissionMaterialViewV2::decode(
                    &rooted.assignment,
                    envelope.inner(),
                    literal_body,
                    AssignmentOpenLimitsV2::within_payload(maximum_payload),
                )
                .map_err(|_| E::Invalid("original Assignment Open material"))?;
                if original.material != *material {
                    return Err(E::Invalid("original Assignment Open material differs"));
                }
            }
            previous = Some(
                frame
                    .append_to(previous.as_ref(), rooted.journal.retention_until_daa)
                    .map_err(|_| E::Invalid("Assignment archive predecessor sequence"))?,
            );
            charge_archive_work_v2(&mut remaining_work, accepted.work)?;
            remaining_heap =
                remaining_heap.checked_sub(accepted.heap).ok_or(E::Unavailable("Assignment archive cumulative heap cap"))?;
            let record = NativeCommittedAssignmentRecordV2::from_native_history_v2(raw, accepted.carriage);
            if ordinal == 0 {
                open = Some(record);
            } else {
                records.push(record);
            }
        }
        if previous != Some(rooted.journal) {
            return Err(E::Invalid("Assignment archive did not reach the rooted head"));
        }
        Ok(NativeAssignmentJournalCarriagesV2 {
            open: open.ok_or(E::Invalid("Assignment archive omitted the original Open"))?,
            records,
            work: maximum_work - remaining_work,
            heap: maximum_heap - remaining_heap - scratch,
            lease,
        })
    }

    /// Caller has already reserved this frame's exact raw extent. Release the
    /// store guard before opening its original native acceptance and delta.
    fn copy_rooted_assignment_frame_v2(&self, frame: AssignmentJournalFrameRow) -> Result<Vec<u8>, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let record_bytes = u64::from(frame.header.payload_bytes) + ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64;
        self.palw_state_v2_store
            .read()
            .with_positive_assignment_journal_record_v2(frame.header.id(), record_bytes, |raw| {
                if raw.len() as u64 != record_bytes {
                    return Err(E::Invalid("literal Assignment record extent differs"));
                }
                let mut copy = Vec::new();
                copy.try_reserve_exact(raw.len()).map_err(|_| E::Unavailable("Assignment record allocation"))?;
                if copy.capacity() > raw.len() {
                    return Err(E::Unavailable("Assignment record exceeded lease"));
                }
                copy.extend_from_slice(raw);
                Ok(copy)
            })
            .map_err(|error| archive_store_fault_v2(error, "rooted Assignment record bytes"))?
    }

    /// Read original decode rules for a continuation against the actual walk's
    /// selected-parent state. Absence of an Assignment row can be a later
    /// same-block intake. Missing history for an existing row is unavailable.
    pub(in crate::pipeline::virtual_processor) fn original_assignment_rules_v2(
        &self,
        parent: BlockHash,
        state: &PalwChainStateV2,
        assignment: AssignmentId,
        continuation_body: &[u8],
    ) -> Result<Option<(NativeAssignmentRulesV2, NativeAcceptedCaseCarriageV2)>, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let workspace = (4 * std::mem::size_of::<NativeRowValue>()
            + 2 * std::mem::size_of::<AssignmentAdmissionMaterialV2>()
            + 2 * MAX_NATIVE_KEY_BYTES) as u64;
        let _workspace =
            reserve_native_requests_v1(workspace).map_err(E::Resource)?.ok_or(E::Unavailable("original rules workspace"))?;
        let rows = PositiveRowsV2::new(state);
        let row = match rows.read(NativeRowKey::Assignment(assignment)) {
            Ok(Some(value)) => match value.body {
                NativeRowBody::Assignment(row) => row,
                _ => return Err(E::Invalid("original Assignment key/value mismatch")),
            },
            Ok(None) | Err(PositiveRowErrorV2::MissingKernelRoute) => return Ok(None),
            Err(_) => return Err(E::Invalid("original Assignment row malformed")),
        };
        let frame = rows
            .read(NativeRowKey::AssignmentJournalFrame { assignment, ordinal: 0 })
            .map_err(|_| E::Invalid("original Assignment frame key/value"))?
            .ok_or(E::Unavailable("original Assignment frame missing"))?;
        let NativeRowBody::AssignmentJournalFrame(frame) = frame.body else {
            return Err(E::Invalid("original Assignment frame kind"));
        };
        if frame.header.kind != AssignmentJournalEventKindV2::Open || frame.header.origin != row.origin {
            return Err(E::Invalid("original Assignment frame differs from rooted origin"));
        }
        let max_payload = u64::from(frame.header.payload_bytes);
        let record_bytes = max_payload + ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64;
        let _raw_lease =
            reserve_native_requests_v1(record_bytes).map_err(E::Resource)?.ok_or(E::Unavailable("original Open bytes lease"))?;
        let raw = self.copy_rooted_assignment_frame_v2(frame)?;
        let record =
            AssignmentJournalRecordViewV2::decode(&raw, record_bytes).map_err(|_| E::Invalid("original Open journal codec"))?;
        if record.frame() != frame {
            return Err(E::Invalid("original Open frame does not match rooted frame"));
        }
        let envelope =
            AssignmentControlViewV2::decode(record.payload(), max_payload).map_err(|_| E::Invalid("original Open envelope"))?;
        let (_, body) = AssignmentControlBoundaryV2::native_body_from_record_data_v2(&record, max_payload)
            .map_err(|_| E::Invalid("original Open carrier framing"))?;
        let view = AssignmentAdmissionMaterialViewV2::decode(
            &row,
            envelope.inner(),
            body,
            AssignmentOpenLimitsV2::within_payload(max_payload),
        )
        .map_err(|_| E::Invalid("original Open material differs from rooted Assignment"))?;
        view.material
            .check_native_open_record_extent_v2(max_payload)
            .map_err(|_| E::Invalid("original Open record exceeds quoted history extent"))?;
        let rules = NativeAssignmentRulesV2::from_original_material_data_v2(&row, &view.material)
            .map_err(|_| E::Invalid("original Assignment rule scope"))?;
        let limits = rules.body_limits().map_err(|_| E::Invalid("original Assignment body limits"))?;
        kaspa_consensus_core::palw_positive_court_v2::coordinator::assignment_body::scan_assignment_body_v2(continuation_body, limits)
            .map_err(|_| E::InvalidContinuation)?;
        let accepted = self.load_positive_assignment_carriage_v2(
            parent,
            state,
            &record,
            max_payload,
            view.material.custody.heap_bytes.min(view.material.native_preimages.physical.max_heap_bytes),
            view.material.custody.replay_work,
        )?;
        let NativeArchiveCarriageV2 { carriage, work, .. } = accepted;
        let prefix = carriage.prefix_state().map_err(|_| E::Unavailable("original Open pre-event state"))?;
        if !matches!(PositiveRowsV2::new(prefix).read(NativeRowKey::Assignment(assignment)), Ok(None)) {
            return Err(E::Invalid("original Open did not start at an absent Assignment"));
        }
        let anchor = BlockHash::from_bytes(row.bundle.terms.branch_anchor.0);
        if !self
            .reachability_service
            .try_is_chain_ancestor_of(anchor, carriage.selected_parent())
            .map_err(|_| E::Unavailable("original signed ancestor reachability"))?
        {
            return Err(E::Invalid("signed Assignment anchor is outside original selected chain"));
        }
        let anchor_daa = self.headers_store.get_daa_score(anchor).map_err(|_| E::Unavailable("original signed ancestor header"))?;
        let remaining_work = view.material.custody.replay_work.checked_sub(work).ok_or(E::Invalid("original replay work cap"))?;
        prefix
            .check_original_assignment_verifier_candidates_v2(&view, anchor_daa, row.bundle.terms.result_before_daa, remaining_work)
            .map_err(|_| E::Invalid("original verifier candidates differ or exceed replay budget"))?;
        Ok(Some((rules, carriage)))
    }

    /// Resolve the original profile from the accepted Assignment's rooted
    /// Open record. Registration cannot replace it with the current Params.
    pub(in crate::pipeline::virtual_processor) fn case_relay_original_contract_v2(
        &self,
        parent: BlockHash,
        state: &PalwChainStateV2,
        terms: &FundingRelayTermsV2,
        physical: &FundingRelayPhysicalProfileV2,
        outer_heap: u64,
    ) -> Result<Option<(AcceptedMaterial, u64)>, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        use kaspa_consensus_core::palw_positive_funding_relay_v2::{CASE_RELAY_OPEN_LOOKUP_READS_V2, CaseRelayOpenReplayQuoteV2};
        let scratch = CaseRelayOpenReplayQuoteV2::scratch_bytes();
        let Some(pin_heap) = physical.max_heap_bytes.checked_sub(outer_heap).and_then(|bytes| bytes.checked_sub(scratch)) else {
            return Ok(None);
        };
        if physical.max_replay_reads < CASE_RELAY_OPEN_LOOKUP_READS_V2 {
            return Ok(None);
        }
        let _lease =
            reserve_native_requests_v1(scratch).map_err(E::Resource)?.ok_or(E::Unavailable("Case relay original scope workspace"))?;
        let rows = PositiveRowsV2::new(state);
        let row = match rows.read(NativeRowKey::Assignment(terms.assignment)) {
            Ok(Some(value)) => match value.body {
                NativeRowBody::Assignment(row) => row,
                _ => return Err(E::Invalid("Case relay original Assignment kind")),
            },
            Ok(None) | Err(PositiveRowErrorV2::MissingKernelRoute) => return Ok(None),
            Err(_) => return Err(E::Invalid("Case relay original Assignment codec")),
        };
        if row.bundle.terms.ruleset != terms.contract_ruleset
            || row.bundle.terms.network != terms.contract_network
            || row.bundle.terms.branch_anchor != terms.contract_anchor
            || row.bundle.terms.poster != terms.payer_bond
        {
            return Ok(None);
        }
        if !self
            .reachability_service
            .try_is_chain_ancestor_of(BlockHash::from_bytes(row.origin.accepting_block.0), parent)
            .map_err(|_| E::Unavailable("Case relay Assignment reachability"))?
        {
            return Err(E::Invalid("Case relay Assignment is outside the selected lineage"));
        }
        let frame = rows
            .read(NativeRowKey::AssignmentJournalFrame { assignment: terms.assignment, ordinal: 0 })
            .map_err(|_| E::Invalid("Case relay original Open frame codec"))?
            .ok_or(E::Unavailable("Case relay original Open frame"))?;
        let NativeRowBody::AssignmentJournalFrame(frame) = frame.body else {
            return Err(E::Invalid("Case relay original Open frame kind"));
        };
        if frame.header.kind != AssignmentJournalEventKindV2::Open || frame.header.origin != row.origin {
            return Err(E::Invalid("Case relay original Open frame differs from Assignment"));
        }
        let payload_bytes = u64::from(frame.header.payload_bytes);
        let quote = CaseRelayOpenReplayQuoteV2::for_payload(frame.header.payload_bytes)
            .map_err(|_| E::Invalid("Case relay original Open quote"))?;
        let record_bytes = quote.record_bytes;
        let work = quote.work;
        if record_bytes > pin_heap || work > physical.max_replay_work {
            return Ok(None);
        }
        let _record =
            reserve_native_requests_v1(record_bytes).map_err(E::Resource)?.ok_or(E::Unavailable("Case relay original Open pin"))?;
        self.palw_state_v2_store
            .read()
            .with_positive_assignment_journal_record_v2(frame.header.id(), record_bytes, |raw| {
                let record = AssignmentJournalRecordViewV2::decode(raw, record_bytes)
                    .map_err(|_| E::Invalid("Case relay original Open record"))?;
                if record.frame() != frame {
                    return Err(E::Invalid("Case relay original Open bytes differ from rooted frame"));
                }
                let envelope = AssignmentControlViewV2::decode(record.payload(), payload_bytes)
                    .map_err(|_| E::Invalid("Case relay original Open envelope"))?;
                let (_, body) = AssignmentControlBoundaryV2::native_body_from_record_data_v2(&record, payload_bytes)
                    .map_err(|_| E::Invalid("Case relay original Open carrier"))?;
                let material = AssignmentAdmissionMaterialViewV2::decode(
                    &row,
                    envelope.inner(),
                    body,
                    AssignmentOpenLimitsV2::within_payload(payload_bytes),
                )
                .map_err(|_| E::Invalid("Case relay original Open material"))?
                .material;
                material
                    .check_native_open_record_extent_v2(payload_bytes)
                    .map_err(|_| E::Invalid("Case relay original Open extent"))?;
                Ok(material.funding_relay.requires_relay().then_some((row.admission_material, work)))
            })
            .map_err(|error| archive_store_fault_v2(error, "Case relay original Open bytes"))?
    }

    /// Reconstruct a genuine Assignment control event from the original native
    /// stores. The caller still has to choose its record from CURRENT rooted
    /// history and replay duties/custody. This method issues no admission or
    /// accounting capability and never reselects a later intake profile.
    fn load_positive_assignment_carriage_v2(
        &self,
        current_selected_parent: BlockHash,
        current_state: &PalwChainStateV2,
        record: &AssignmentJournalRecordViewV2<'_>,
        maximum_payload: u64,
        maximum_heap: u64,
        maximum_work: u64,
    ) -> Result<NativeArchiveCarriageV2, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        if record.header().kind == AssignmentJournalEventKindV2::MetadataReceipt {
            return Err(E::Unavailable("actual metadata receipt carriage owner is not connected"));
        }
        let (boundary, native_body) = AssignmentControlBoundaryV2::native_body_from_record_data_v2(record, maximum_payload)
            .map_err(|_| E::Invalid("invalid original Assignment control carriage framing"))?;
        let accepted = self.load_positive_accepted_carriage_v2(
            current_selected_parent,
            current_state,
            NativeArchiveBoundaryV2::Assignment(boundary),
            maximum_heap,
            maximum_work,
        )?;
        let tx = accepted.carriage.transaction();
        if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
            || tx.payload.get(..3) != Some(&[1, 0, 176])
            || tx.payload.get(3..) != Some(native_body)
        {
            return Err(E::Invalid("original actual Assignment carrier differs from journal"));
        }
        Ok(accepted)
    }

    fn load_positive_accepted_carriage_v2(
        &self,
        current_selected_parent: BlockHash,
        current_state: &PalwChainStateV2,
        boundary: NativeArchiveBoundaryV2,
        maximum_heap: u64,
        maximum_work: u64,
    ) -> Result<NativeArchiveCarriageV2, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        if [maximum_heap, maximum_work].into_iter().any(|n| n == 0 || n == u64::MAX) {
            return Err(E::Invalid("original archive limits are not finite"));
        }
        let (origin, expected_parent, pre_event_root, before_delta_end, expected_fee) = boundary.fields();
        let accepting = BlockHash::from_bytes(origin.accepting_block.0);
        let carrier_block = BlockHash::from_bytes(origin.carrier_block.0);
        if !self
            .reachability_service
            .try_is_chain_ancestor_of(accepting, current_selected_parent)
            .map_err(|_| E::Unavailable("accepted lineage reachability"))?
        {
            return Err(E::Invalid("original accepting block is not on current selected chain"));
        }
        let actual_header = self.headers_store.get_header(accepting).map_err(|_| E::Unavailable("original accepting header"))?;
        let selected_parent =
            self.ghostdag_store.get_selected_parent(accepting).map_err(|_| E::Unavailable("original selected parent"))?;
        if actual_header.daa_score != origin.accepted_daa
            || actual_header.blue_score != origin.accepted_blue_score
            || selected_parent.as_bytes() != expected_parent
            || origin.event_ordinal != 0
        {
            return Err(E::Invalid("original header/parent/clock differs"));
        }
        let mut remaining_work = maximum_work;
        let (accepted_carrier, accepted, acceptance_bytes) = self
            .acceptance_data_store
            .accepted_origin_at_bounded_v2(accepting, origin.accepted_tx_ordinal, remaining_work / 2)
            .map_err(|error| archive_store_fault_v2(error, "bounded original acceptance data"))?
            .ok_or(E::Invalid("accepted ordinal outside original acceptance"))?;
        if accepted_carrier != carrier_block
            || accepted.index_within_block == 0
            || accepted.transaction_id.as_bytes() != origin.carrier_transaction.0
        {
            return Err(E::Invalid("actual accepted ordinal/carrier differs"));
        }
        // The bounded ordinal lookup and the pinned input view each
        // validate the complete encoding. Per-input scans are charged below.
        charge_archive_work_v2(&mut remaining_work, acceptance_bytes * 2)?;
        let carrier =
            self.read_archive_transaction_v2(carrier_block, accepted.index_within_block, maximum_heap, &mut remaining_work)?;
        let maximum_heap = maximum_heap.checked_sub(carrier.heap).ok_or(E::Unavailable("archived carrier exhausted replay heap"))?;
        let tx = &carrier.transaction;
        if tx.is_coinbase() || !tx.id_is_current() || tx.id().as_bytes() != origin.carrier_transaction.0 {
            return Err(E::Invalid("original actual transaction identity"));
        }
        // Bound both scans for same-prefix creators, plus the duplicate-input
        // check, before calling the shared populated-input reconstruction.
        charge_archive_work_v2(&mut remaining_work, original_input_replay_work_v2(acceptance_bytes, tx.inputs.len() as u64)?)?;
        let scans = utxo_diff_scans_v2(tx.inputs.len() as u64)?;
        let (entries, lease, retained_heap) = self
            .utxo_diffs_store
            .with_utxo_archive_v2(accepting, remaining_work / scans, |diff, diff_bytes| {
                charge_archive_work_v2(&mut remaining_work, diff_bytes * scans)?;
                self.acceptance_data_store
                    .with_accepted_archive_v2(accepting, acceptance_bytes, |acceptance| {
                        self.restore_original_archive_inputs_v2(
                            (&acceptance, &diff),
                            origin,
                            tx,
                            maximum_heap,
                            (size_of::<NativeAcceptedCaseCarriageV2>() + size_of::<[usize; 2]>()) as u64,
                            |creator, index, heap| self.read_archive_transaction_v2(creator, index, heap, &mut remaining_work),
                        )
                    })
                    .map_err(|error| archive_store_fault_v2(error, "bounded original acceptance archive"))?
            })
            .map_err(|error| archive_store_fault_v2(error, "bounded original UTXO archive"))??;
        if !boundary.check_populated_data(tx, &entries, expected_fee) {
            return Err(E::Invalid("actual original populated input/fee identity"));
        }
        let populated = PopulatedTransaction::new(tx, entries);
        let actual_fee = self
            .transaction_validator
            .validate_populated_transaction_and_get_fee(&populated, origin.accepted_daa, TxValidationFlags::Full, None, None)
            .map_err(|_| E::Invalid("original ordinary transaction Full validation"))?;
        if actual_fee != expected_fee {
            return Err(E::Invalid("original actual Full fee differs"));
        }
        let params = self.palw_state_params_v2.as_ref().ok_or(E::Unavailable("native state parameters"))?;
        let replay_heap =
            maximum_heap.checked_sub(retained_heap).ok_or(E::Unavailable("retained original inputs exhaust the archive heap cap"))?;
        let (parent, parent_heap) = self.restore_native_archive_parent_v2(
            (current_selected_parent, current_state),
            selected_parent,
            params,
            replay_heap,
            &mut remaining_work,
        )?;
        if actual_header.palw_state_root != self.palw_committed_root_v1(&parent) {
            return Err(E::Invalid("original accepting header commits a different parent state"));
        }
        let replay_heap =
            replay_heap.checked_sub(parent_heap).ok_or(E::Unavailable("restored parent exhausts the archive heap cap"))?;
        let (delta, delta_heap) = self.read_archive_delta_v2(accepting, replay_heap, &mut remaining_work)?;
        let replay_heap = replay_heap.checked_sub(delta_heap).ok_or(E::Unavailable("original delta exhausts prefix heap"))?;
        if delta.point.block != accepting
            || delta.point.daa_score != origin.accepted_daa
            || delta.point.blue_score != origin.accepted_blue_score
        {
            return Err(E::Invalid("actual native delta clock differs"));
        }
        let end = usize::try_from(before_delta_end).map_err(|_| E::Invalid("prefix position address space"))?;
        let root = Hash64::from_bytes(pre_event_root);
        let prefix_heap = native_delta_replay_requests_v1(&parent, &delta).map_err(E::Resource)?;
        let prefix = parent.try_delta_prefix_v2(&delta, end, root, params, replay_heap).map_err(|e| match e {
            kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => E::Resource(r),
            _ => E::Invalid("original executed native prefix mismatch"),
        })?;
        let heap = carrier
            .heap
            .checked_add(retained_heap)
            .and_then(|n| n.checked_add(prefix_heap))
            .ok_or(E::Invalid("retained carriage heap overflow"))?;
        let entries = populated.entries;
        let carriage = NativeAcceptedCaseCarriageV2::from_native_history_v2(
            carrier.transaction,
            carrier.lease,
            entries,
            actual_fee,
            origin,
            selected_parent,
            lease,
        )
        .map_err(|_| E::Invalid("actual native carriage shape"))?;
        let carriage = carriage.with_native_prefix_state_v2(Arc::new(prefix), root).map_err(|error| match error {
            NativeAssignedCaseBackingErrorV2::NativeResource(r) => E::Resource(r),
            _ => E::Invalid("actual prefix owner mismatch"),
        })?;
        Ok(NativeArchiveCarriageV2 { carriage, work: maximum_work - remaining_work, heap })
    }

    /// Start with the exact selected-parent view already held by the caller.
    /// Walk only its ancestors, charging and releasing one delta at a time.
    /// No tip or pruning snapshot from a different generation is consulted.
    fn restore_native_archive_parent_v2(
        &self,
        (mut cursor, current_state): (BlockHash, &PalwChainStateV2),
        original_parent: BlockHash,
        params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        maximum_heap: u64,
        remaining_work: &mut u64,
    ) -> Result<(NativeChainStateV1, u64), NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let mut restored: Option<NativeChainStateV1> = None;
        let mut retained_heap = 0;
        while cursor != original_parent {
            charge_archive_work_v2(remaining_work, 1)?;
            let heap = maximum_heap.checked_sub(retained_heap).ok_or(E::Unavailable("archive parent walk heap cap"))?;
            let (delta, delta_heap) = self.read_archive_delta_v2(cursor, heap, remaining_work)?;
            let heap = heap.checked_sub(delta_heap).ok_or(E::Unavailable("original delta exhausts parent walk heap"))?;
            let state = restored.as_deref().unwrap_or(current_state);
            if delta.point.block != cursor || state.last_point() != Some(&delta.point) {
                return Err(E::Invalid("archive parent walk state and delta differ"));
            }
            let previous = self.ghostdag_store.get_selected_parent(cursor).map_err(|_| E::Unavailable("archive parent walk link"))?;
            if previous == cursor {
                return Err(E::Invalid("archive parent walk did not advance"));
            }
            let (next, requests) =
                NativeChainStateV1::try_revert_archive_delta_v2(state, &delta, params, heap).map_err(|why| match why {
                    NativeDecodeErrorV1::Resource(why) => E::Resource(why),
                    NativeDecodeErrorV1::Decode(_) => E::Invalid("archive parent walk could not revert the actual delta"),
                })?;
            restored = Some(next);
            retained_heap = requests;
            cursor = previous;
        }
        // The event's accepting block is itself on the current selected chain,
        // so the selected parent of that event takes at least one reversal.
        Ok((restored.ok_or(E::Invalid("archive parent walk did not cross the accepting block"))?, retained_heap))
    }

    fn read_archive_delta_v2(
        &self,
        block: BlockHash,
        heap: u64,
        remaining_work: &mut u64,
    ) -> Result<(NativeStateDeltaV1, u64), NativeAssignmentArchiveErrorV2> {
        let (delta, bytes, retained_heap) = self
            .palw_state_v2_store
            .read()
            .delta_of_retained_bounded_v2(block, *remaining_work / ARCHIVE_WORK_PER_DELTA_BYTE_V2, heap)
            .map_err(|error| archive_store_fault_v2(error, "bounded original native delta"))?;
        charge_archive_work_v2(remaining_work, bytes * ARCHIVE_WORK_PER_DELTA_BYTE_V2)?;
        Ok((delta, retained_heap))
    }

    /// Verify the historical outer op14 carrier with its real native inputs.
    /// This replays ordinary Full validation, separate from the two package
    /// signatures and from the custody/budget decision.
    pub(in crate::pipeline::virtual_processor) fn replay_original_relay_carrier_v2(
        &self,
        mut replay: RelayCarrierReplayV2<'_>,
    ) -> Result<(), NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;

        let accepting = BlockHash::from_bytes(replay.origin.accepting_block.0);
        let carrier = BlockHash::from_bytes(replay.origin.carrier_block.0);
        replay.charge(1, 0)?;
        let carrier = self.read_archive_transaction_v2(carrier, replay.index, replay.maximum_heap, &mut replay.remaining_work)?;
        let input_heap =
            replay.maximum_heap.checked_sub(carrier.heap).ok_or(E::Unavailable("archived relay carrier exhausted replay heap"))?;
        let tx = &carrier.transaction;
        if tx.is_coinbase()
            || !tx.id_is_current()
            || tx.id().as_bytes() != replay.origin.carrier_transaction.0
            || tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
            || !(replay.matches_payload)(&tx.payload)
        {
            return Err(E::Invalid("original relay carrier differs from literal archive"));
        }

        let acceptance_bytes = replay.acceptance_bytes;
        replay.charge(1, acceptance_bytes)?;

        replay.charge(0, original_input_replay_work_v2(acceptance_bytes, tx.inputs.len() as u64)?)?;
        let scans = utxo_diff_scans_v2(tx.inputs.len() as u64)?;
        replay.charge(1, 0)?;
        let (entries, lease, _) = self
            .utxo_diffs_store
            .with_utxo_archive_v2(accepting, replay.remaining_work / scans, |diff, diff_bytes| {
                replay.charge(0, diff_bytes * scans)?;
                self.acceptance_data_store
                    .with_accepted_archive_v2(accepting, acceptance_bytes, |acceptance| {
                        self.restore_original_archive_inputs_v2(
                            (&acceptance, &diff),
                            replay.origin,
                            tx,
                            input_heap,
                            0,
                            |creator, index, heap| {
                                replay.charge(1, 0)?;
                                self.read_archive_transaction_v2(creator, index, heap, &mut replay.remaining_work)
                            },
                        )
                    })
                    .map_err(|error| archive_store_fault_v2(error, "bounded original relay acceptance archive"))?
            })
            .map_err(|error| archive_store_fault_v2(error, "bounded original relay UTXO archive"))??;
        let populated = PopulatedTransaction::new(tx, entries);
        let actual_fee = self
            .transaction_validator
            .validate_populated_transaction_and_get_fee(&populated, replay.origin.accepted_daa, TxValidationFlags::Full, None, None)
            .map_err(|_| E::Invalid("original relay ordinary transaction Full validation"))?;
        if actual_fee != replay.gross_fee {
            return Err(E::Invalid("original relay Full fee differs from archived gross fee"));
        }
        drop(populated);
        drop(lease);
        drop(carrier);
        self.replay_original_relay_payer_v2(replay)
    }

    /// Reverse only the payer fields from the rooted selected-parent view.
    /// Each delta is released before the next one is read. The original
    /// registration pair fixes the exact position within the accepting block.
    fn replay_original_relay_payer_v2(&self, mut replay: RelayCarrierReplayV2<'_>) -> Result<(), NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let authorization = replay.authorization;
        let current_custody = replay.custody;
        let terms = &authorization.terms;
        let (contract_key, poster_nonce) = match terms.target {
            RelayTargetV2::InitialAssignment { poster_nonce, .. } => {
                (NativeRowKey::AssignmentNonce(terms.payer_bond), Some(poster_nonce))
            }
            RelayTargetV2::InitialCase { .. } => (NativeRowKey::Assignment(terms.assignment), None),
            _ => return Err(E::Unavailable("original relay payer contract owner")),
        };
        // The rooted R rows remain under the outer lease. Compare them
        // directly with the delta's borrowed encodings. Only keys and the
        // small nonce projection need additional scratch.
        let scratch = pending_funding_relay_payer_scratch_v2(terms.funding_purpose);
        let delta_heap = replay.maximum_heap.checked_sub(scratch).ok_or(E::Invalid("relay payer replay heap cap"))?;
        let _lease =
            reserve_native_requests_v1(scratch).map_err(E::Resource)?.ok_or(E::Unavailable("relay payer replay workspace owner"))?;
        let funding = terms.funding_outpoint;
        let purpose = terms.funding_purpose;
        let auth_key = NativeRowKey::FundingRelayAuthorization { funding, purpose }.encode();
        let custody_key = NativeRowKey::FundingRelayCustody { funding, purpose }.encode();
        let contract_key = (KERNEL_AUX_TABLE, contract_key.encode());
        let payer =
            PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes(terms.payer_bond.transaction.0), terms.payer_bond.index));
        let floor = self.palw_state_params_v2.as_ref().ok_or(E::Unavailable("original relay bond floor"))?.min_collateral_sompi();
        let project = |bond: Option<&PalwBondStateV2>| {
            bond.map(|bond| {
                (bond.collateral, palw_bond_may_take_work_v2(bond, floor), bond.pubkey.as_slice() == authorization.payer_key)
            })
        };
        let read_nonce = |bytes: Option<&[u8]>| {
            bytes
                .map(|bytes| {
                    let malformed = E::Invalid("malformed original relay nonce");
                    let literal = bytes.strip_prefix(&[2, 0, 1, 0, 11]).ok_or(malformed)?;
                    let row: AssignmentNonceRow =
                        borsh::from_slice(literal).map_err(|_| E::Invalid("malformed original relay nonce"))?;
                    if row.poster != terms.payer_bond || (row.nonce.exhausted && row.nonce.next != u64::MAX) {
                        return Err(E::Invalid("original relay nonce key or range differs"));
                    }
                    Ok(row.nonce)
                })
                .transpose()
        };
        replay.charge(2, (size_of::<PalwBondKeyV2>() + authorization.payer_key.len() + contract_key.1.len()) as u64)?;
        let mut bond = project(replay.state.bond(&payer));
        let route = replay.state.kernel_route_state_v1().ok_or(E::Unavailable("rooted relay kernel route"))?;
        let raw = route.aux.get(&contract_key).map(Vec::as_slice);
        replay.charge(0, raw.map_or(0, |bytes| bytes.len() as u64))?;
        let (mut nonce, mut case_row) =
            if poster_nonce.is_some() { (read_nonce(raw)?, None) } else { (None, Some(RelayCaseHistoryRowV2::new(raw)?)) };
        let accepting = BlockHash::from_bytes(replay.origin.accepting_block.0);
        let mut cursor = replay.selected_parent;
        // Account for the wire preflights, decode and projected entry walk.
        loop {
            replay.charge(1, 0)?;
            let (delta, bytes, _) = self
                .palw_state_v2_store
                .read()
                .delta_of_retained_bounded_v2(cursor, replay.remaining_work / ARCHIVE_WORK_PER_DELTA_BYTE_V2, delta_heap)
                .map_err(|error| archive_store_fault_v2(error, "bounded original relay payer delta"))?;
            replay.charge(0, bytes * ARCHIVE_WORK_PER_DELTA_BYTE_V2)?;
            if delta.point.block != cursor {
                return Err(E::Invalid("relay payer history delta block differs"));
            }
            if cursor == accepting
                && (delta.point.daa_score != replay.origin.accepted_daa || delta.point.blue_score != replay.origin.accepted_blue_score)
            {
                return Err(E::Invalid("original relay payer delta clock differs"));
            }
            for (index, entry) in delta.entries.iter().enumerate().rev() {
                match entry {
                    PalwDeltaEntryV2::Bond { key, old, new } if *key == payer => {
                        if bond != project(new.as_ref()) {
                            return Err(E::Invalid("relay payer bond history differs"));
                        }
                        bond = project(old.as_ref());
                    }
                    PalwDeltaEntryV2::KernelRouteRow { table, key, old, new }
                        if *table == contract_key.0 && *key == contract_key.1 =>
                    {
                        if let Some(row) = &mut case_row {
                            if row.raw() != new.as_deref() {
                                return Err(E::Invalid("Case relay Assignment history differs"));
                            }
                            row.replace(old.as_deref())?;
                        } else {
                            if nonce != read_nonce(new.as_deref())? {
                                return Err(E::Invalid("relay payer nonce history differs"));
                            }
                            nonce = read_nonce(old.as_deref())?;
                        }
                    }
                    PalwDeltaEntryV2::KernelRouteRow { table, key, old: None, new: Some(bytes) }
                        if cursor == accepting && *table == KERNEL_AUX_TABLE && *key == auth_key =>
                    {
                        let Some(PalwDeltaEntryV2::KernelRouteRow { table, key, old: None, new: Some(custody_bytes) }) =
                            delta.entries.get(index + 1)
                        else {
                            return Err(E::Invalid("original relay registration lacks adjacent custody"));
                        };
                        if *table != KERNEL_AUX_TABLE
                            || *key != custody_key
                            || !pending_funding_relay_row_images_match_v2(authorization, current_custody, bytes, custody_bytes)
                            || !matches!(bond, Some((_, true, true)))
                        {
                            return Err(E::Invalid("original relay row images or payer bond differ"));
                        }
                        if let Some(poster_nonce) = poster_nonce {
                            nonce
                                .unwrap_or_default()
                                .after_accepting(poster_nonce)
                                .map_err(|_| E::Invalid("original relay payer nonce was ineligible"))?;
                        } else {
                            let raw = case_row
                                .as_ref()
                                .and_then(RelayCaseHistoryRowV2::raw)
                                .and_then(|row| row.strip_prefix(&[2, 0, 1, 0, 10]))
                                .ok_or(E::Invalid("Case relay Assignment missing at original R event"))?;
                            // Account for the bounded row decode and validation
                            // passes over the fixed Assignment preimages.
                            replay.charge(0, 8 * raw.len() as u64)?;
                            let assignment: misaka_palw_positive_contract::assignment::rows::AssignmentRow =
                                borsh::from_slice(raw).map_err(|_| E::Invalid("Case relay Assignment codec"))?;
                            if replay.case_contract_material != Some(assignment.admission_material) {
                                return Err(E::Invalid("Case relay original profile material differs at R event"));
                            }
                            kaspa_consensus_core::palw_positive_assignment_funds_v2::check_case_relay_registration_assignment_v2(
                                terms,
                                &assignment,
                                replay.origin,
                            )
                            .map_err(|_| E::Invalid("Case relay original Assignment scope or payer"))?;
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
            if cursor == accepting {
                return Err(E::Invalid("original relay registration absent from accepting delta"));
            }
            replay.charge(1, 64)?;
            cursor = self
                .ghostdag_store
                .get_selected_parent(cursor)
                .map_err(|_| E::Unavailable("relay payer historical selected parent"))?;
        }
    }

    fn read_archive_transaction_v2(
        &self,
        block: BlockHash,
        index: u32,
        maximum_heap: u64,
        remaining_work: &mut u64,
    ) -> Result<ArchivedTransactionV2, NativeAssignmentArchiveErrorV2> {
        // Charge both the block framing scan and the selected decode before
        // allocating the selected transaction.
        let (transaction, extent) = self
            .block_transactions_store
            .archive_transaction_v2(block, index, *remaining_work / 2, maximum_heap)
            .map_err(|error| archive_store_fault_v2(error, "bounded archived transaction"))?;
        charge_archive_work_v2(remaining_work, extent * 2)?;
        Ok(transaction)
    }

    fn restore_original_archive_inputs_v2(
        &self,
        (acceptance, diff): (&AcceptanceArchiveViewV2<'_>, &UtxoArchiveViewV2<'_>),
        origin: AcceptedOrigin,
        tx: &kaspa_consensus_core::tx::Transaction,
        maximum_heap: u64,
        owner_heap: u64,
        mut load_creator: impl FnMut(BlockHash, u32, u64) -> Result<ArchivedTransactionV2, NativeAssignmentArchiveErrorV2>,
    ) -> Result<
        (Vec<UtxoEntry>, kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1, u64),
        NativeAssignmentArchiveErrorV2,
    > {
        use NativeAssignmentArchiveErrorV2 as E;
        // First pass borrows actual originals and prices every clone before
        // constructing the populated input vector or validation scratch.
        let mut heap = owner_heap
            .checked_add(
                (tx.inputs.len() as u64)
                    .checked_mul(std::mem::size_of::<UtxoEntry>() as u64)
                    .ok_or(E::Invalid("input slots overflow"))?,
            )
            .ok_or(E::Invalid("input heap overflow"))?;
        for (position, input) in tx.inputs.iter().enumerate() {
            if tx.inputs[..position].iter().any(|prior| prior.previous_outpoint == input.previous_outpoint) {
                return Err(E::Invalid("duplicate original input"));
            }
            let bytes = if let Some(entry) =
                diff.find_removed_v2(input.previous_outpoint).map_err(|_| E::Invalid("original removed input archive"))?
            {
                entry.script.len() as u64
            } else {
                self.with_prior_assignment_output_v2(
                    acceptance,
                    origin,
                    input.previous_outpoint,
                    maximum_heap,
                    &mut load_creator,
                    |output, _| output.script_public_key.script().len() as u64,
                )?
                .ok_or(E::Unavailable("original populated input or same-prefix creator"))?
            };
            heap = heap.checked_add(bytes).ok_or(E::Invalid("input script heap overflow"))?;
        }
        // Full validator scratch is owned by its actual native implementation;
        // these retained inputs and owner remain separately reserved.
        if heap > maximum_heap {
            return Err(E::Invalid("original archive retained heap cap"));
        }
        let lease = reserve_native_requests_v1(heap).map_err(E::Resource)?.ok_or(E::Unavailable("actual archive allocation lease"))?;
        let mut entries = Vec::new();
        entries.try_reserve_exact(tx.inputs.len()).map_err(|_| E::Unavailable("pre-admitted input allocation"))?;
        for input in &tx.inputs {
            let entry = if let Some(entry) =
                diff.find_removed_v2(input.previous_outpoint).map_err(|_| E::Invalid("original removed input archive"))?
            {
                entry.to_entry_v2()
            } else {
                self.with_prior_assignment_output_v2(
                    acceptance,
                    origin,
                    input.previous_outpoint,
                    maximum_heap - heap,
                    &mut load_creator,
                    |output, coinbase| UtxoEntry::new(output.value, output.script_public_key.clone(), origin.accepted_daa, coinbase),
                )?
                .ok_or(E::Unavailable("same-prefix original creator disappeared"))?
            };
            entries.push(entry);
        }
        Ok((entries, lease, heap))
    }

    /// Same accepting-prefix creator outputs disappear from net UTXO diffs;
    /// resolve them only from ACTUAL earlier accepted transaction bodies.
    fn with_prior_assignment_output_v2<T>(
        &self,
        acceptance: &AcceptanceArchiveViewV2<'_>,
        origin: AcceptedOrigin,
        outpoint: kaspa_consensus_core::tx::TransactionOutpoint,
        maximum_heap: u64,
        load_creator: &mut impl FnMut(BlockHash, u32, u64) -> Result<ArchivedTransactionV2, NativeAssignmentArchiveErrorV2>,
        mut visit: impl FnMut(&kaspa_consensus_core::tx::TransactionOutput, bool) -> T,
    ) -> Result<Option<T>, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let Some((block, entry)) = acceptance
            .find_before_v2(origin.accepted_tx_ordinal, outpoint.transaction_id)
            .map_err(|_| E::Invalid("original acceptance archive traversal"))?
        else {
            return Ok(None);
        };
        let creator = load_creator(block, entry.index_within_block, maximum_heap)?;
        let tx = &creator.transaction;
        if !tx.id_is_current() || tx.id() != outpoint.transaction_id {
            return Err(E::Invalid("earlier accepted creator identity"));
        }
        tx.outputs
            .get(outpoint.index as usize)
            .map(|output| Some(visit(output, tx.is_coinbase())))
            .ok_or(E::Invalid("earlier creator output index"))
    }
}

#[cfg(test)]
mod archive_fault_tests {
    use super::*;

    #[test]
    fn case_boundary_is_unique_and_scoped_to_the_literal_record() {
        use misaka_palw_positive_contract::{
            CONTRACT_REVISION, CONTRACT_VERSION,
            identity::{CaseId, ChainScope, ResultId},
            journal::{CaseJournalAux, CaseJournalEventKind, CaseJournalHeader, CaseJournalLimits, CaseJournalRecord},
            rows::NativeHash64,
        };

        let origin = AcceptedOrigin {
            accepting_block: NativeHash64([1; 64]),
            carrier_block: NativeHash64([2; 64]),
            carrier_transaction: NativeHash64([3; 64]),
            accepted_tx_ordinal: 1,
            event_ordinal: 0,
            accepted_daa: 4,
            accepted_blue_score: 5,
        };
        let case = CaseId(Digest([6; 64]));
        let boundary = AssignmentCaseBoundaryV2 {
            version: 2,
            revision: 1,
            case,
            origin,
            selected_parent: NativeHash64([7; 64]),
            pre_event_root: Digest([8; 64]),
            before_delta_end: 2,
            populated_inputs: Digest([9; 64]),
            input_count: 1,
            fee: 10,
        };
        let limits = CaseJournalLimits {
            max_record_bytes: 8192,
            max_payload_bytes: 1024,
            max_aux_entries: 4,
            max_aux_preimage_bytes: 1024,
            max_total_preimage_bytes: 4096,
            max_logical_heap_bytes: 8192,
        };
        let header = CaseJournalHeader {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            scope: ChainScope { network: Digest([10; 64]), ruleset: Digest([11; 64]), branch: Digest([12; 64]) },
            case,
            result: ResultId(Digest([13; 64])),
            sequence: 1,
            previous: Digest([0; 64]),
            origin,
            kind: CaseJournalEventKind::NativeTransaction,
        };
        let material =
            |value: AssignmentCaseBoundaryV2| CaseJournalAux { material: value.material(), preimage: borsh::to_vec(&value).unwrap() };
        let raw = |mut aux: Vec<CaseJournalAux>| {
            aux.sort_by_key(|value| borsh::to_vec(&value.material).unwrap());
            CaseJournalRecord { header, payload: vec![9], aux }.encode(limits).unwrap()
        };
        let decode = |bytes: Vec<u8>| exact_case_boundary_v2(CaseJournalRecordView::decode(&bytes, limits).unwrap());
        assert_eq!(decode(raw(vec![material(boundary)])).unwrap(), boundary);

        let mut altered = material(boundary);
        altered.preimage[4] ^= 1;
        use NativeAssignmentArchiveErrorV2 as E;
        assert!(matches!(decode(raw(vec![altered])), Err(E::Invalid("missing actual Case boundary"))));
        let wrong_case = material(AssignmentCaseBoundaryV2 { case: CaseId(Digest([20; 64])), ..boundary });
        assert!(matches!(decode(raw(vec![wrong_case])), Err(E::Invalid("Case boundary scope/origin differs from literal record"))));
        let different_prefix = material(AssignmentCaseBoundaryV2 { before_delta_end: 3, ..boundary });
        assert!(matches!(decode(raw(vec![material(boundary), different_prefix])), Err(E::Invalid("duplicate actual Case boundary"))));
    }

    #[test]
    fn archive_store_faults_distinguish_corruption_capacity_and_missing_record() {
        const COMPONENT: &str = "fixture history";
        use NativeAssignmentArchiveErrorV2 as E;
        assert!(matches!(
            archive_store_fault_v2(StoreError::DataInconsistency("bad framing".into()), COMPONENT),
            E::Invalid(COMPONENT)
        ));
        assert!(matches!(
            archive_store_fault_v2(
                StoreError::DeserializationError(Box::new(bincode::ErrorKind::Custom("bad wire".into()))),
                COMPONENT
            ),
            E::Invalid(COMPONENT)
        ));
        assert!(matches!(archive_store_fault_v2(StoreError::LocalResourceUnavailable("heap cap".into()), COMPONENT), E::Resource(_)));
        let key = kaspa_database::prelude::DbKey::new(&[0], BlockHash::from_bytes([0; 64]));
        assert!(matches!(archive_store_fault_v2(StoreError::KeyNotFound(key), COMPONENT), E::Unavailable(COMPONENT)));
    }
}
