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
    palw_class_registration_origin_v1::{
        PALW_CLASS_REGISTRATION_ORIGIN_ROW_VERSION_V1, PalwClassRegistrationDoorV1, palw_class_admission_bytes_digest_v1,
        palw_class_registration_origin_row_v1,
    },
    palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2,
    palw_native_resource_v1::{
        NativeChainStateV1, NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, NativeHeapV1, NativeStateDeltaV1,
        native_delta_replay_requests_v1, reserve_native_requests_v1,
    },
    palw_positive_assignment_case_material_v2::{
        AssignmentCaseBoundaryV2, assigned_case_native_carrier_matches_v2, rooted_initial_assigned_case_bind_v2,
    },
    palw_positive_assignment_material_v2::{
        AssignmentAdmissionMaterialV2, AssignmentAdmissionMaterialViewV2, AssignmentRegisteredDescriptorV2,
    },
    palw_positive_funding_relay_v2::{pending_funding_relay_payer_scratch_v2, pending_funding_relay_row_images_match_v2},
    palw_positive_journal_chain_v2::{
        CaseJournalChainErrorV2, CaseJournalChainLimitsV2, CaseJournalLinkQuoteV2, CaseJournalPinV2, CaseJournalVisitErrorV2,
        CheckedCaseJournalLinksV2, check_case_journal_links_v2,
    },
    palw_positive_rows_v2::{PositiveRowErrorV2, PositiveRowsV2},
    palw_state_v2::{
        AssignmentControlBoundaryV2, NativeAcceptedCaseCarriageV2, NativeAssignedCaseBackingErrorV2, NativeAssignedCaseBackingV2,
        NativeAssignmentRulesV2, NativeCommittedAssignedCaseV2, NativeCommittedAssignmentRecordV2, NativeCommittedCaseRecordV2,
        PalwBondKeyV2, PalwBondStateV2, PalwChainStateV2, PalwDeltaEntryV2, palw_bond_may_take_work_v2,
    },
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
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
        rows::{AssignmentJournalFrameRow, AssignmentNonceRow, AssignmentRow},
    },
    identity::{CaseId, Digest, ResultId},
    journal::{CASE_JOURNAL_AUX_MATERIAL_BYTES, CaseJournalLimits, CaseJournalRecordView},
    native::{KERNEL_AUX_TABLE, NativeRowKey},
    rows::{
        AcceptedMaterial, AcceptedOrigin, MAX_NATIVE_KEY_BYTES, MAX_NATIVE_ROW_BYTES, NativeHash64, NativeRowBody, NativeRowValue,
    },
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

/// Open material backed by its literal accepted transaction and prefix.
struct OriginalAssignmentOpenArchiveV2 {
    assignment: AssignmentRow,
    frame: AssignmentJournalFrameRow,
    material: AssignmentAdmissionMaterialV2,
    rules: NativeAssignmentRulesV2,
    raw: Vec<u8>,
    carriage: NativeAcceptedCaseCarriageV2,
    lease: NativeDemandLeaseV1,
    heap: u64,
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

// ClassRegisteredTirV1 is a single lifecycle object (Borsh tag 61). Its
// admission is the final field. The rooted digest authenticates this suffix
// against a previously folded typed carriage, so no wide object is allocated.
pub(in crate::pipeline::virtual_processor) fn tir_registration_admission_bytes_v2(
    payload: &[u8],
    class: Digest,
    artifact: Digest,
) -> Option<&[u8]> {
    use borsh::BorshDeserialize;
    use kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2;
    let mut bytes = payload;
    if u16::deserialize(&mut bytes).ok()? != PALW_LIFECYCLE_TX_VERSION_V2
        || u8::deserialize(&mut bytes).ok()? != 61
        || <[u8; 64]>::deserialize(&mut bytes).ok()? != class.0
        || <[u8; 64]>::deserialize(&mut bytes).ok()? != artifact.0
    {
        return None;
    }
    u64::deserialize(&mut bytes).ok()?; // slash_value_per_pwu
    PalwPwuRuleV2::deserialize(&mut bytes).ok()?;
    u128::deserialize(&mut bytes).ok()?; // initial_target
    u16::deserialize(&mut bytes).ok()?; // share_permille
    u64::deserialize(&mut bytes).ok()?; // activation_daa
    Some(bytes)
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

impl VirtualStateProcessor {
    /// Reconstruct the complete selected-parent Case record chain, including
    /// the native carriage and executed prefix of each original event.
    pub(in crate::pipeline::virtual_processor) fn prepare_committed_case_history_v2(
        &self,
        parent: BlockHash,
        state: &PalwChainStateV2,
        (case, expected_result): (CaseId, Option<ResultId>),
    ) -> Result<Option<NativeAssignedCaseBackingV2>, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;

        let workspace = (8 * size_of::<NativeRowValue>() + 4 * MAX_NATIVE_KEY_BYTES) as u64;
        let _workspace =
            reserve_native_requests_v1(workspace).map_err(E::Resource)?.ok_or(E::Unavailable("committed Case head rows"))?;
        let Some((assignment, pin)) = state
            .assigned_case_journal_pin_v2(case, expected_result)
            .map_err(|_| E::Unavailable("committed Case head and Assignment links"))?
        else {
            return Ok(None);
        };
        let Some(open) = self.load_original_assignment_open_v2(parent, state, assignment, None)? else {
            return Err(E::Unavailable("committed Case original Assignment material"));
        };
        let rules = &open.rules;
        let positive = rules.positive_policy();
        let physical = rules.native_preimages().ok_or(E::Unavailable("original Case physical limits"))?;
        let maximum_heap =
            positive.max_case_heap_bytes.min(rules.assignment_policy().max_heap_bytes).min(physical.physical.max_heap_bytes);
        let maximum_work = positive.max_case_work.min(rules.assignment_policy().max_replay_work);
        let maximum_record = positive.max_case_journal_bytes;
        let maximum_aux = maximum_record / (CASE_JOURNAL_AUX_MATERIAL_BYTES as u64 + 5);
        let limits = CaseJournalChainLimitsV2 {
            record_limits: CaseJournalLimits {
                max_record_bytes: maximum_record,
                max_payload_bytes: maximum_record,
                max_aux_entries: u32::try_from(maximum_aux).map_err(|_| E::Invalid("Case aux count extent"))?,
                max_aux_preimage_bytes: maximum_record,
                max_total_preimage_bytes: maximum_record,
                max_logical_heap_bytes: maximum_heap,
            },
            max_events: u64::from(rules.assignment_policy().max_history_frames),
            max_total_record_bytes: maximum_record.checked_mul(2).ok_or(E::Invalid("Case journal read extent"))?,
            max_hash_bytes: maximum_record.checked_mul(4).ok_or(E::Invalid("Case journal hash extent"))?,
            max_index_bytes: u64::from(rules.assignment_policy().max_history_frames) * size_of::<Digest>() as u64,
        };
        let retained_open = open
            .assignment
            .native_heap_v1("retained Assignment")
            .add::<OriginalAssignmentOpenArchiveV2>(open.material.native_heap_v1("retained admission material"), "original Open");
        let retained_open = retained_open.bytes().ok_or(E::Resource(retained_open))?;
        let available_heap = maximum_heap
            .checked_sub(workspace)
            .and_then(|heap| heap.checked_sub(open.heap))
            .and_then(|heap| heap.checked_sub(open.raw.capacity() as u64))
            .and_then(|heap| heap.checked_sub(retained_open))
            .ok_or(E::Unavailable("original Open exhausts Case archive heap"))?;
        let NativeCaseJournalCarriagesV2 { records, work, heap, mut lease } =
            self.load_rooted_case_journal_carriages_v2((parent, state), (assignment, pin), limits, available_heap, maximum_work)?;
        let mut remaining_heap = available_heap.checked_sub(heap).ok_or(E::Unavailable("retained Case archive heap"))?;
        let mut remaining_work = maximum_work.checked_sub(work).ok_or(E::Unavailable("retained Case archive work"))?;
        let assignment_records =
            self.load_assignment_control_history_v2((parent, state), &open, &mut lease, &mut remaining_heap, &mut remaining_work)?;
        Ok(Some(NativeAssignedCaseBackingV2::Committed(NativeCommittedAssignedCaseV2::from_native_archive_v2(
            parent,
            state.state_root(),
            open.assignment,
            open.material,
            open.raw,
            open.carriage,
            pin,
            records,
            assignment_records,
            lease,
            open.lease,
        ))))
    }

    /// The rooted Assignment journal selects the exact retained frame at each
    /// ordinal. Reconstruct each control's actual pre-event state independently.
    fn load_assignment_control_history_v2(
        &self,
        (parent, state): (BlockHash, &PalwChainStateV2),
        open: &OriginalAssignmentOpenArchiveV2,
        lease: &mut NativeDemandLeaseV1,
        remaining_heap: &mut u64,
        remaining_work: &mut u64,
    ) -> Result<Vec<NativeCommittedAssignmentRecordV2>, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;

        let id = open.assignment.id();
        let rows = PositiveRowsV2::new(state);
        let Some(NativeRowBody::AssignmentJournalHead(head)) = rows
            .read(NativeRowKey::AssignmentJournalHead(id))
            .map_err(|_| E::Invalid("current Assignment journal head codec"))?
            .map(|row| row.body)
        else {
            return Err(E::Unavailable("current Assignment journal head"));
        };
        let count = head.next_ordinal.checked_sub(1).ok_or(E::Invalid("empty Assignment journal"))?;
        let policy = &open.material.assignment_policy;
        if count == 0 || head.next_ordinal > u64::from(policy.max_history_frames) {
            return Err(E::Invalid("consumed Assignment journal count"));
        }
        let maximum_record = policy
            .max_frame_bytes
            .checked_add(ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64)
            .ok_or(E::Invalid("Assignment journal record extent"))?;
        let maximum_storage = open.material.native_assignment_journal_storage_limit_v2();
        let slot_bytes = count
            .checked_mul(size_of::<NativeCommittedAssignmentRecordV2>() as u64)
            .ok_or(E::Invalid("Assignment control index extent"))?;
        *remaining_heap = remaining_heap.checked_sub(slot_bytes).ok_or(E::Unavailable("Assignment control index heap"))?;
        lease.grow_memory_v1(slot_bytes).map_err(E::Resource)?;
        let count = usize::try_from(count).map_err(|_| E::Invalid("Assignment control index address space"))?;
        let mut controls = Vec::new();
        controls.try_reserve_exact(count).map_err(|_| E::Unavailable("Assignment control index allocation"))?;
        if controls.capacity() > count {
            return Err(E::Unavailable("Assignment control index allocation exceeds quote"));
        }
        let mut remaining_storage =
            maximum_storage.checked_sub(open.raw.len() as u64).ok_or(E::Invalid("original Open exceeds Assignment journal extent"))?;
        let mut prior = open.frame.append_to(None, head.retention_until_daa).map_err(|_| E::Invalid("retained Open head"))?;
        for ordinal in 1..head.next_ordinal {
            charge_archive_work_v2(remaining_work, open.material.native_preimages.source_policy.work.per_control)?;
            let Some(NativeRowBody::AssignmentJournalFrame(frame)) = rows
                .read(NativeRowKey::AssignmentJournalFrame { assignment: id, ordinal })
                .map_err(|_| E::Invalid("rooted Assignment control codec"))?
                .map(|row| row.body)
            else {
                return Err(E::Unavailable("rooted Assignment control frame"));
            };
            if frame.header.assignment != id || frame.header.ordinal != ordinal {
                return Err(E::Invalid("rooted Assignment control subject"));
            }
            let next =
                frame.append_to(Some(&prior), head.retention_until_daa).map_err(|_| E::Invalid("rooted Assignment control chain"))?;
            let payload = u64::from(frame.header.payload_bytes);
            let bytes = payload + ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64;
            if bytes > maximum_record {
                return Err(E::Invalid("rooted Assignment control exceeds original frame cap"));
            }
            *remaining_heap = remaining_heap.checked_sub(bytes).ok_or(E::Unavailable("Assignment control raw heap"))?;
            remaining_storage = remaining_storage.checked_sub(bytes).ok_or(E::Invalid("Assignment control storage cap"))?;
            charge_archive_work_v2(remaining_work, 4 * bytes)?;
            lease.grow_memory_v1(bytes).map_err(E::Resource)?;
            let raw = self.read_assignment_archive_record_v2(frame, bytes)?;
            let record =
                AssignmentJournalRecordViewV2::decode(&raw, bytes).map_err(|_| E::Invalid("retained Assignment control codec"))?;
            if record.frame() != frame {
                return Err(E::Invalid("literal Assignment control differs from rooted frame"));
            }
            let accepted =
                self.load_positive_assignment_carriage_v2(parent, state, record, payload, *remaining_heap, *remaining_work)?;
            charge_archive_work_v2(remaining_work, accepted.work)?;
            let prefix = accepted.carriage.prefix_state().map_err(|_| E::Unavailable("Assignment control pre-event state"))?;
            let prefix_rows = PositiveRowsV2::new(prefix);
            let Some(NativeRowBody::AssignmentJournalHead(actual_prior)) = prefix_rows
                .read(NativeRowKey::AssignmentJournalHead(id))
                .map_err(|_| E::Invalid("Assignment control pre-event head codec"))?
                .map(|row| row.body)
            else {
                return Err(E::Invalid("Assignment control pre-event head absent"));
            };
            if actual_prior != prior {
                return Err(E::Invalid("Assignment control pre-event history differs"));
            }
            *remaining_heap = remaining_heap.checked_sub(accepted.heap).ok_or(E::Unavailable("Assignment control carriage heap"))?;
            controls.push(NativeCommittedAssignmentRecordV2::from_native_history_v2(raw, accepted.carriage));
            prior = next;
        }
        if prior != head {
            return Err(E::Invalid("Assignment control history differs from rooted head"));
        }
        Ok(controls)
    }

    /// Copy a previously reserved frame without holding the DB guard across
    /// native accepted-transaction or delta lookups.
    fn read_assignment_archive_record_v2(
        &self,
        frame: AssignmentJournalFrameRow,
        bytes: u64,
    ) -> Result<Vec<u8>, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        self.palw_state_v2_store
            .read()
            .with_positive_assignment_journal_record_v2(frame.header.id(), bytes, |raw| {
                if raw.len() as u64 != bytes {
                    return Err(E::Invalid("Assignment archive record differs from rooted extent"));
                }
                let mut copy = Vec::new();
                copy.try_reserve_exact(raw.len()).map_err(|_| E::Unavailable("Assignment archive allocation"))?;
                if copy.capacity() as u64 > bytes {
                    return Err(E::Unavailable("Assignment archive allocation exceeds quote"));
                }
                copy.extend_from_slice(raw);
                Ok(copy)
            })
            .map_err(|error| archive_store_fault_v2(error, "rooted Assignment archive bytes"))?
    }

    fn check_retained_case_bind_result_v2(
        &self,
        state: &PalwChainStateV2,
        case: CaseJournalRecordView<'_>,
        boundary: AssignmentCaseBoundaryV2,
        native_body: &[u8],
        remaining_work: &mut u64,
    ) -> Result<(), NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        use kaspa_consensus_core::palw_positive_court_v2::coordinator::assignment_body::scan_assignment_continuation_id_v2;

        let assignment = scan_assignment_continuation_id_v2(native_body).map_err(|_| E::Invalid("initial Case Assignment id"))?;
        let frame = rooted_initial_assigned_case_bind_v2(state, assignment, case, boundary).map_err(|why| match why {
            PositiveRowErrorV2::MissingRequiredRow | PositiveRowErrorV2::MissingKernelRoute => {
                E::Unavailable("rooted initial Case bind")
            }
            _ => E::Invalid("rooted initial Case bind differs"),
        })?;
        let payload_bytes = u64::from(frame.header.payload_bytes);
        let record_bytes = payload_bytes + ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64;
        // Reserve work for the full fixed framing/hash and byte comparison
        // before the DB returns even a borrowed record.
        charge_archive_work_v2(remaining_work, 3 * record_bytes)?;
        self.palw_state_v2_store
            .read()
            .with_positive_assignment_journal_record_v2(frame.header.id(), record_bytes, |raw| {
                let record = AssignmentJournalRecordViewV2::decode(raw, record_bytes)
                    .map_err(|_| E::Invalid("retained Case BindResult codec"))?;
                if record.frame() != frame {
                    return Err(E::Invalid("retained Case BindResult differs from rooted frame"));
                }
                let (other, body) = AssignmentControlBoundaryV2::native_body_from_record_data_v2(&record, payload_bytes)
                    .map_err(|_| E::Invalid("retained Case BindResult carrier framing"))?;
                if body != native_body
                    || other.selected_parent != boundary.selected_parent
                    || other.pre_event_root != boundary.pre_event_root
                    || other.before_delta_end != boundary.before_delta_end
                    || other.populated_inputs != boundary.populated_inputs
                    || other.input_count != boundary.input_count
                    || other.fee != boundary.fee
                {
                    return Err(E::Invalid("Case and Assignment journal boundaries differ"));
                }
                Ok(())
            })
            .map_err(|error| archive_store_fault_v2(error, "retained Case BindResult bytes"))?
    }

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
    fn load_rooted_case_journal_carriages_v2(
        &self,
        (parent, state): (BlockHash, &PalwChainStateV2),
        (assignment, pin): (AssignmentId, CaseJournalPinV2),
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
            let header = record.header();
            if header.sequence == 1 {
                let native_body = &accepted.carriage.transaction().payload[3..];
                self.check_retained_case_bind_result_v2(state, record, boundary, native_body, &mut remaining_work)?;
            } else {
                let prefix = accepted.carriage.prefix_state().map_err(|_| E::Unavailable("archived Case pre-event state"))?;
                let Some((prior_assignment, prior)) = prefix
                    .assigned_case_journal_pin_v2(pin.case, Some(pin.result))
                    .map_err(|_| E::Invalid("archived Case prefix links"))?
                else {
                    return Err(E::Invalid("Case predecessor absent at its archived event prefix"));
                };
                if prior_assignment != assignment
                    || prior.scope != pin.scope
                    || prior.head.sequence != header.sequence - 1
                    || prior.head.head != header.previous
                {
                    return Err(E::Invalid("archived Case prefix differs from its predecessor"));
                }
            }
            records.push(NativeCommittedCaseRecordV2::from_native_history_v2(raw, accepted.carriage));
        }
        Ok(NativeCaseJournalCarriagesV2 {
            records,
            work: maximum_work - remaining_work,
            heap: maximum_heap - remaining_heap - scratch,
            lease,
        })
    }

    /// Read original decode rules for a continuation against the actual walk's
    /// selected-parent state. Absence of an Assignment row can be a later
    /// same-block intake. Missing history for an existing row is unavailable.
    pub(in crate::pipeline::virtual_processor) fn original_assignment_rules_v2(
        &self,
        parent: BlockHash,
        state: &PalwChainStateV2,
        assignment: AssignmentId,
        continuation_body: Option<&[u8]>,
    ) -> Result<Option<(NativeAssignmentRulesV2, NativeAcceptedCaseCarriageV2, AssignmentAdmissionMaterialV2)>, NativeAssignmentArchiveErrorV2> {
        Ok(self
            .load_original_assignment_open_v2(parent, state, assignment, continuation_body)?
            .map(|open| (open.rules, open.carriage, open.material)))
    }

    fn load_original_assignment_open_v2(
        &self,
        parent: BlockHash,
        state: &PalwChainStateV2,
        assignment: AssignmentId,
        continuation_body: Option<&[u8]>,
    ) -> Result<Option<OriginalAssignmentOpenArchiveV2>, NativeAssignmentArchiveErrorV2> {
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
        let lease =
            reserve_native_requests_v1(record_bytes).map_err(E::Resource)?.ok_or(E::Unavailable("original Open bytes lease"))?;
        let raw = self.read_assignment_archive_record_v2(frame, record_bytes)?;
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
        view.material
            .check_native_open_quote_v2(&row, body)
            .map_err(|_| E::Invalid("original Open quote differs from literal carrier"))?;
        let rules = NativeAssignmentRulesV2::from_original_material_data_v2(&row, &view.material)
            .map_err(|_| E::Invalid("original Assignment rule scope"))?;
        if let Some(body) = continuation_body {
            let limits = rules.body_limits().map_err(|_| E::Invalid("original Assignment body limits"))?;
            kaspa_consensus_core::palw_positive_court_v2::coordinator::assignment_body::scan_assignment_body_v2(body, limits)
                .map_err(|_| E::InvalidContinuation)?;
        }
        let replay_heap = view.material.custody.heap_bytes.min(view.material.native_preimages.physical.max_heap_bytes);
        let accepted = self.load_positive_assignment_carriage_v2(
            parent,
            state,
            record,
            max_payload,
            replay_heap,
            view.material.custody.replay_work,
        )?;
        let prefix = accepted.carriage.prefix_state().map_err(|_| E::Unavailable("original Open pre-event state"))?;
        view.material
            .check_registered_class_v2(prefix)
            .map_err(|_| E::Invalid("original registered TIR class differs from the Open descriptor"))?;
        if !matches!(PositiveRowsV2::new(prefix).read(NativeRowKey::Assignment(assignment)), Ok(None)) {
            return Err(E::Invalid("original Open did not start at an absent Assignment"));
        }
        let anchor = BlockHash::from_bytes(row.bundle.terms.branch_anchor.0);
        if !self
            .reachability_service
            .try_is_chain_ancestor_of(anchor, accepted.carriage.selected_parent())
            .map_err(|_| E::Unavailable("original signed ancestor reachability"))?
        {
            return Err(E::Invalid("signed Assignment anchor is outside original selected chain"));
        }
        let anchor_daa = self.headers_store.get_daa_score(anchor).map_err(|_| E::Unavailable("original signed ancestor header"))?;
        let mut remaining_work =
            view.material.custody.replay_work.checked_sub(accepted.work).ok_or(E::Invalid("original replay work cap"))?;
        let registration_heap = replay_heap.checked_sub(accepted.heap).ok_or(E::Unavailable("original Open exhausted replay heap"))?;
        self.check_original_tir_registration_archive_v2(
            prefix,
            &view.material.registered,
            view.material.origin,
            registration_heap,
            &mut remaining_work,
        )?;
        prefix
            .check_original_assignment_verifier_candidates_v2(&view, anchor_daa, row.bundle.terms.result_before_daa, remaining_work)
            .map_err(|_| E::Invalid("original verifier candidates differ or exceed replay budget"))?;
        Ok(Some(OriginalAssignmentOpenArchiveV2 {
            assignment: row,
            frame,
            material: view.material,
            rules,
            raw,
            carriage: accepted.carriage,
            lease,
            heap: accepted.heap,
        }))
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
        record: AssignmentJournalRecordViewV2<'_>,
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
        // Use the unchanged Assignment identity to find the Case, then reverse
        // the actual funding cursor to the original registration position.
        let scratch = pending_funding_relay_payer_scratch_v2(terms);
        let delta_heap = replay.maximum_heap.checked_sub(scratch).ok_or(E::Invalid("relay payer replay heap cap"))?;
        let _lease =
            reserve_native_requests_v1(scratch).map_err(E::Resource)?.ok_or(E::Unavailable("relay payer replay workspace owner"))?;
        let mut continuation_assignment = None;
        let (contract_key, poster_nonce) = match terms.target {
            RelayTargetV2::InitialAssignment { poster_nonce, .. } => {
                (NativeRowKey::AssignmentNonce(terms.payer_bond), Some(poster_nonce))
            }
            RelayTargetV2::InitialCase { .. } => (NativeRowKey::Assignment(terms.assignment), None),
            RelayTargetV2::Continuation { .. }
                if terms.funding_purpose == misaka_palw_positive_contract::assignment::funding_relay::RelayFundingPurposeV2::CaseC =>
            {
                use misaka_palw_positive_contract::assignment::rows::AssignmentStatusV2;
                let assignment_key = NativeRowKey::Assignment(terms.assignment);
                let encoded = (KERNEL_AUX_TABLE, assignment_key.encode());
                let route = replay.state.kernel_route_state_v1().ok_or(E::Unavailable("rooted relay kernel route"))?;
                let bytes = route.aux.get(&encoded).ok_or(E::Unavailable("continuation original Assignment"))?;
                replay.charge(1, 8 * bytes.len() as u64)?;
                let NativeRowBody::Assignment(assignment) =
                    NativeRowValue::decode(&encoded.1, bytes).map_err(|_| E::Invalid("continuation Assignment codec"))?.body
                else {
                    return Err(E::Invalid("continuation Assignment kind"));
                };
                let AssignmentStatusV2::Consumed { case, .. } = assignment.status else {
                    return Err(E::Invalid("continuation Assignment was not consumed"));
                };
                if replay.case_contract_material != Some(assignment.admission_material) {
                    return Err(E::Invalid("continuation differs from its original Assignment material"));
                }
                continuation_assignment = Some(assignment);
                (NativeRowKey::Funding(case), None)
            }
            _ => return Err(E::Unavailable("original relay payer contract owner")),
        };
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
        let mut bond = continuation_assignment.is_none().then(|| project(replay.state.bond(&payer))).flatten();
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
                    PalwDeltaEntryV2::Bond { key, old, new } if *key == payer && continuation_assignment.is_none() => {
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
                            || (continuation_assignment.is_none() && !matches!(bond, Some((_, true, true))))
                        {
                            return Err(E::Invalid("original relay row images or payer bond differ"));
                        }
                        if let Some(poster_nonce) = poster_nonce {
                            nonce
                                .unwrap_or_default()
                                .after_accepting(poster_nonce)
                                .map_err(|_| E::Invalid("original relay payer nonce was ineligible"))?;
                        } else if let Some(assignment) = &continuation_assignment {
                            let bytes = case_row
                                .as_ref()
                                .and_then(RelayCaseHistoryRowV2::raw)
                                .ok_or(E::Invalid("Case funding missing at successor registration"))?;
                            replay.charge(0, 8 * bytes.len() as u64)?;
                            let NativeRowBody::Funding(account) = NativeRowValue::decode(&contract_key.1, bytes)
                                .map_err(|_| E::Invalid("successor funding codec"))?
                                .body
                            else {
                                return Err(E::Invalid("successor funding kind"));
                            };
                            if account.payer_key != authorization.payer_key {
                                return Err(E::Invalid("successor payer differs from historical funding key"));
                            }
                            kaspa_consensus_core::palw_positive_assignment_funds_v2::check_case_relay_continuation_account_v2(
                                terms,
                                assignment,
                                &account,
                                replay.origin,
                            )
                            .map_err(|_| E::Invalid("successor differs from historical Case cursor"))?;
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

    /// Resolve a signed class id to its literal registration on the selected
    /// parent. Aux141 pins the admitting block and exact carriage; the native
    /// acceptance archive supplies the carrier transaction and ordinal.
    pub(in crate::pipeline::virtual_processor) fn discover_tir_registration_archive_v2(
        &self,
        parent: BlockHash,
        state: &PalwChainStateV2,
        class: Digest,
        maximum_heap: u64,
        mut remaining_work: u64,
    ) -> Result<Option<AssignmentRegisteredDescriptorV2>, NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let class_id = Hash64::from_bytes(class.0);
        let Some((row, tir)) = state.class(&class_id).zip(state.tir_class_v1(&class_id)) else { return Ok(None) };
        let Some(rooted) = palw_class_registration_origin_row_v1(state.kernel_route_state_v1(), &class_id) else {
            return Ok(None);
        };
        let Some(digest) = rooted.admission_digest else { return Ok(None) };
        if rooted.version != PALW_CLASS_REGISTRATION_ORIGIN_ROW_VERSION_V1
            || rooted.door != PalwClassRegistrationDoorV1::Tir
            || rooted.class_id != class_id
            || rooted.daa_score != row.registered_daa
            || tir.facts.class_id != class_id
        {
            return Ok(None);
        }
        let accepting = BlockHash::from_bytes(rooted.block.as_bytes());
        if !self
            .reachability_service
            .try_is_chain_ancestor_of(accepting, parent)
            .map_err(|_| E::Unavailable("TIR registration selected ancestry"))?
        {
            return Err(E::Invalid("rooted TIR registration outside selected ancestry"));
        }
        let header = self.headers_store.get_header(accepting).map_err(|_| E::Unavailable("TIR registration header"))?;
        if header.daa_score != rooted.daa_score {
            return Err(E::Invalid("rooted TIR registration clock"));
        }
        let artifact = Digest(row.artifact_root.as_bytes());
        let origin = self
            .acceptance_data_store
            .try_with_accepted_archive_v2(accepting, remaining_work / 2, |view| {
                charge_archive_work_v2(&mut remaining_work, view.byte_len_v2() * 2)?;
                let mut matched = None;
                let mut fault = None;
                let mut refused = false;
                view.visit_origins_v2(|ordinal, (block, entry)| {
                    if entry.index_within_block == 0 {
                        return false;
                    }
                    let candidate = (|| {
                        let archive = self
                            .block_transactions_store
                            .archive_transaction_with_limits_v2(block, entry.index_within_block, remaining_work / 2, maximum_heap)
                            .map_err(|why| archive_store_fault_v2(why, "TIR registration archived transaction"))?;
                        let Ok((archived, extent)) = archive else {
                            refused = true;
                            return Ok(None);
                        };
                        charge_archive_work_v2(&mut remaining_work, extent * 2)?;
                        let tx = &archived.transaction;
                        if tx.id() != entry.transaction_id {
                            return Err(E::Invalid("TIR registration acceptance transaction differs from archived body"));
                        }
                        if tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE {
                            return Ok(None);
                        }
                        let Some(admission) = tir_registration_admission_bytes_v2(&tx.payload, class, artifact) else {
                            return Ok(None);
                        };
                        let Some(work) = remaining_work.checked_sub(admission.len() as u64) else {
                            refused = true;
                            return Ok(None);
                        };
                        remaining_work = work;
                        if palw_class_admission_bytes_digest_v1(admission) != digest {
                            return Ok(None);
                        }
                        if tx.is_coinbase() || !tx.id_is_current() {
                            return Err(E::Invalid("TIR registration archived transaction identity"));
                        }
                        if matched.is_some() {
                            refused = true;
                            return Ok(None);
                        }
                        Ok(Some(AcceptedOrigin {
                            accepting_block: NativeHash64(accepting.as_bytes()),
                            carrier_block: NativeHash64(block.as_bytes()),
                            carrier_transaction: NativeHash64(entry.transaction_id.as_bytes()),
                            accepted_tx_ordinal: u32::try_from(ordinal)
                                .map_err(|_| E::Invalid("TIR registration ordinal overflow"))?,
                            event_ordinal: 0,
                            accepted_daa: header.daa_score,
                            accepted_blue_score: header.blue_score,
                        }))
                    })();
                    match candidate {
                        Ok(Some(origin)) => matched = Some(origin),
                        Ok(None) => (),
                        Err(error) => {
                            fault = Some(error);
                            return true;
                        }
                    }
                    refused
                })
                .map_err(|why| archive_store_fault_v2(why, "TIR registration acceptance entries"))?;
                match fault {
                    Some(why) => Err(why),
                    None => Ok(matched.filter(|_| !refused)),
                }
            })
            .map_err(|why| archive_store_fault_v2(why, "TIR registration acceptance archive"))?
            .transpose()?
            .flatten();
        Ok(origin.map(|origin| AssignmentRegisteredDescriptorV2 {
            class,
            artifact,
            program: Digest(tir.graph_ir_root.as_bytes()),
            layout: Digest(tir.layout_digest.as_bytes()),
            registered_tokenizer: Digest(tir.tokenizer_id.as_bytes()),
            registration: AcceptedMaterial { origin, digest: Digest(digest.as_bytes()) },
        }))
    }

    /// Resolve the registration reference against the original selected chain,
    /// native acceptance archive, and the rooted admission digest.
    fn check_original_tir_registration_archive_v2(
        &self,
        prefix: &PalwChainStateV2,
        descriptor: &AssignmentRegisteredDescriptorV2,
        before: AcceptedOrigin,
        maximum_heap: u64,
        remaining_work: &mut u64,
    ) -> Result<(), NativeAssignmentArchiveErrorV2> {
        use NativeAssignmentArchiveErrorV2 as E;
        let registration = descriptor.registration;
        let origin = registration.origin;
        let class = Hash64::from_bytes(descriptor.class.0);
        let rooted = palw_class_registration_origin_row_v1(prefix.kernel_route_state_v1(), &class)
            .ok_or(E::Unavailable("original rooted TIR admission identity"))?;
        if rooted.version != PALW_CLASS_REGISTRATION_ORIGIN_ROW_VERSION_V1
            || rooted.door != PalwClassRegistrationDoorV1::Tir
            || rooted.class_id != class
            || rooted.block.as_bytes() != origin.accepting_block.0
            || rooted.daa_score != origin.accepted_daa
            || rooted.admission_digest != Some(Hash64::from_bytes(registration.digest.0))
            || origin.event_ordinal != 0
        {
            return Err(E::Invalid("original rooted TIR admission identity"));
        }
        let accepting = BlockHash::from_bytes(origin.accepting_block.0);
        if origin.accepting_block == before.accepting_block {
            if origin.accepted_tx_ordinal >= before.accepted_tx_ordinal {
                return Err(E::Invalid("registration did not precede the Open"));
            }
        } else if !self
            .reachability_service
            .try_is_chain_ancestor_of(accepting, BlockHash::from_bytes(before.accepting_block.0))
            .map_err(|_| E::Unavailable("original TIR registration reachability"))?
        {
            return Err(E::Invalid("registration is outside the original selected chain"));
        }
        let header = self.headers_store.get_header(accepting).map_err(|_| E::Unavailable("original TIR registration header"))?;
        if header.daa_score != origin.accepted_daa || header.blue_score != origin.accepted_blue_score {
            return Err(E::Invalid("original TIR registration clock"));
        }
        let (block, entry, bytes) = self
            .acceptance_data_store
            .accepted_origin_at_bounded_v2(accepting, origin.accepted_tx_ordinal, *remaining_work / 2)
            .map_err(|error| archive_store_fault_v2(error, "original TIR registration acceptance"))?
            .ok_or(E::Invalid("original TIR registration ordinal"))?;
        if block.as_bytes() != origin.carrier_block.0
            || entry.transaction_id.as_bytes() != origin.carrier_transaction.0
            || entry.index_within_block == 0
        {
            return Err(E::Invalid("original TIR registration acceptance identity"));
        }
        charge_archive_work_v2(remaining_work, bytes * 2)?;
        let archived = self.read_archive_transaction_v2(block, entry.index_within_block, maximum_heap, remaining_work)?;
        let tx = &archived.transaction;
        if tx.is_coinbase()
            || tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE
            || !tx.id_is_current()
            || tx.id() != entry.transaction_id
        {
            return Err(E::Invalid("original TIR registration transaction"));
        }
        let admission = tir_registration_admission_bytes_v2(&tx.payload, descriptor.class, descriptor.artifact)
            .ok_or(E::Invalid("original TIR registration carrier fields"))?;
        charge_archive_work_v2(remaining_work, admission.len() as u64)?;
        if palw_class_admission_bytes_digest_v1(admission).as_bytes() != registration.digest.0 {
            return Err(E::Invalid("original TIR registration admission bytes"));
        }
        Ok(())
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
