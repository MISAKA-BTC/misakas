//! Canonical native DATA preimages retained by assigned Case journal aux.
//! These bind actual populated-input data and exact executed-prefix boundaries;
//! they never encode Full=true, a computation verdict, or an admission permit.
//! The native loader still retrieves the actual accepted carrier and applies
//! the real parent delta prefix. Ordinary transaction scripts remain mandatory.
use crate::{
    Hash64, TransactionId,
    palw_positive_funding_relay_v2::canonical_equal,
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::{Transaction, UtxoEntry},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    ContractError,
    identity::{CaseId, Digest},
    journal::{CaseJournalEventKind, CaseJournalRecordView},
    rows::{AcceptedMaterial, AcceptedOrigin, NativeHash64},
};

/// Match the exact accepted native carrier to a Case journal event. The
/// first event must carry the complete op9 transaction. Later events carry
/// the original literal body; matching either form grants no Case authority.
pub fn assigned_case_native_carrier_matches_v2(tx: &Transaction, record: CaseJournalRecordView<'_>) -> bool {
    if tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE || tx.payload.get(..3) != Some(&[1, 0, 176]) {
        return false;
    }
    let header = record.header();
    match header.kind {
        CaseJournalEventKind::NativeBody => {
            header.sequence > 1 && matches!(record.payload().get(4), Some(1..=7)) && tx.payload.get(3..) == Some(record.payload())
        }
        CaseJournalEventKind::NativeTransaction => {
            header.sequence == 1 && tx.payload.get(7) == Some(&9) && canonical_equal(tx, record.payload())
        }
    }
}

pub const ASSIGNED_CASE_BOUNDARY_DOMAIN_V2: &[u8] = b"misaka/positive/assigned-case-native-boundary/v2/r1";
pub const ASSIGNED_CASE_INPUTS_DOMAIN_V2: &[u8] = b"misaka/positive/assigned-case-populated-inputs/v2/r1";
pub const ASSIGNED_CASE_BOUNDARY_BYTES_V2: usize = 496;
fn hash<T: BorshSerialize>(domain: &[u8], value: &T) -> Digest {
    struct Writer(blake2b_simd::State);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    value.serialize(&mut writer).expect("native bounded canonical data");
    Digest(writer.0.finalize().as_bytes().try_into().expect("64-byte hash"))
}
/// Exact canonical DATA recipe is Borsh `(2u16,1u16,actual_txid,[entries])`.
/// The slice encodes its ordinary u32 length. Entries are the actual populated
/// inputs in transaction-input order, retaining amounts/scripts/coinbase/DAA.
pub fn assigned_case_inputs_id_v2(transaction: TransactionId, entries: &[UtxoEntry]) -> Result<Digest, ContractError> {
    u32::try_from(entries.len()).map_err(|_| ContractError::Budget)?;
    Ok(hash(ASSIGNED_CASE_INPUTS_DOMAIN_V2, &(2u16, 1u16, transaction, entries)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentCaseBoundaryV2 {
    pub version: u16,
    pub revision: u16,
    pub case: CaseId,
    pub origin: AcceptedOrigin,
    pub selected_parent: NativeHash64,
    /// Actual native builder state immediately BEFORE this event. Root comes
    /// from the executed prefix, never from the later current P/Case snapshot.
    pub pre_event_root: Digest,
    /// Length of the native transition delta BEFORE this event checkpoint.
    pub before_delta_end: u64,
    pub populated_inputs: Digest,
    pub input_count: u32,
    pub fee: u64,
}
impl AssignmentCaseBoundaryV2 {
    pub fn from_actual_data_v2(
        case: CaseId,
        origin: AcceptedOrigin,
        selected_parent: Hash64,
        pre_event_root: Hash64,
        before_delta_end: usize,
        entries: &[UtxoEntry],
        fee: u64,
    ) -> Result<Self, ContractError> {
        Ok(Self {
            version: 2,
            revision: 1,
            case,
            origin,
            selected_parent: NativeHash64(selected_parent.as_bytes()),
            pre_event_root: Digest(pre_event_root.as_bytes()),
            before_delta_end: u64::try_from(before_delta_end).map_err(|_| ContractError::Budget)?,
            populated_inputs: assigned_case_inputs_id_v2(TransactionId::from_bytes(origin.carrier_transaction.0), entries)?,
            input_count: u32::try_from(entries.len()).map_err(|_| ContractError::Budget)?,
            fee,
        })
    }
    pub fn id(&self) -> Digest {
        hash(ASSIGNED_CASE_BOUNDARY_DOMAIN_V2, self)
    }
    pub fn material(&self) -> AcceptedMaterial {
        AcceptedMaterial { origin: self.origin, digest: self.id() }
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, ContractError> {
        if bytes.len() != ASSIGNED_CASE_BOUNDARY_BYTES_V2 {
            return Err(ContractError::NonCanonical);
        }
        let value: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        if value.version != 2 || value.revision != 1 {
            return Err(ContractError::Version);
        }
        if value.input_count == 0 || value.pre_event_root == Digest([0; 64]) || value.populated_inputs == Digest([0; 64]) {
            return Err(ContractError::Identity);
        }
        Ok(value)
    }
    pub fn check_populated_data(
        &self,
        transaction: &crate::tx::Transaction,
        entries: &[UtxoEntry],
        fee: u64,
    ) -> Result<(), ContractError> {
        check_native_populated_inputs_v2(self.origin, self.input_count, self.populated_inputs, self.fee, transaction, entries, fee)
    }
}

/// Same canonical input/fee check for genuine Assignment and Case boundaries.
/// This authenticates DATA equality only; ordinary Full and accepted ancestry
/// remain the concrete native loader's separate responsibilities.
pub(crate) fn check_native_populated_inputs_v2(
    origin: AcceptedOrigin,
    input_count: u32,
    populated_inputs: Digest,
    recorded_fee: u64,
    transaction: &crate::tx::Transaction,
    entries: &[UtxoEntry],
    fee: u64,
) -> Result<(), ContractError> {
    if !transaction.id_is_current()
        || crate::hashing::tx::id(transaction).as_bytes() != origin.carrier_transaction.0
        || transaction.inputs.len() != entries.len()
        || input_count as usize != entries.len()
        || recorded_fee != fee
        || populated_inputs != assigned_case_inputs_id_v2(transaction.id(), entries)?
    {
        return Err(ContractError::Identity);
    }
    let inputs = entries.iter().try_fold(0u64, |n, e| n.checked_add(e.amount).ok_or(ContractError::Budget))?;
    let outputs = transaction.outputs.iter().try_fold(0u64, |n, o| n.checked_add(o.value).ok_or(ContractError::Budget))?;
    if outputs.checked_add(fee) != Some(inputs) {
        return Err(ContractError::Budget);
    }
    Ok(())
}

/// The first Case record binds the existing native prefix boundary and the
/// exact Assignment BindResult frame. These links carry no replay verdict.
pub(crate) fn initial_assigned_case_bind_header_v2(
    record: misaka_palw_positive_contract::journal::CaseJournalRecordView<'_>,
    boundary: AssignmentCaseBoundaryV2,
    use_row: &misaka_palw_positive_contract::assignment::rows::AssignmentUseRow,
) -> Result<misaka_palw_positive_contract::assignment::journal::AssignmentJournalHeaderV2, ContractError> {
    use misaka_palw_positive_contract::{
        assignment::journal::{ASSIGNMENT_JOURNAL_HEADER_BYTES_V2, AssignmentJournalEventKindV2, AssignmentJournalHeaderV2},
        journal::CaseJournalEventKind,
    };

    let header = record.header();
    if header.sequence != 1
        || header.kind != CaseJournalEventKind::NativeTransaction
        || header.case != use_row.case
        || header.result != use_row.result
        || header.origin != use_row.origin
        || use_row.binding.origin != use_row.origin
        || boundary.case != use_row.case
        || boundary.origin != use_row.origin
        || record.aux_count() != 2
    {
        return Err(ContractError::Identity);
    }
    let mut bind = None;
    let mut found_boundary = false;
    for aux in record.aux() {
        if aux.material() == use_row.binding && aux.preimage().len() == ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 {
            let candidate = AssignmentJournalHeaderV2::try_from_slice(aux.preimage()).map_err(|_| ContractError::NonCanonical)?;
            candidate.validate()?;
            if candidate.kind != AssignmentJournalEventKindV2::BindResult
                || candidate.assignment != use_row.assignment
                || candidate.origin != use_row.origin
                || candidate.id() != use_row.binding.digest
            {
                return Err(ContractError::Identity);
            }
            bind = Some(candidate);
        } else if aux.material() == boundary.material() && AssignmentCaseBoundaryV2::decode(aux.preimage())? == boundary {
            found_boundary = true;
        } else {
            return Err(ContractError::Identity);
        }
    }
    bind.filter(|_| found_boundary).ok_or(ContractError::Identity)
}

pub const ASSIGNED_RECORD_HEADER_DOMAIN_V2: &[u8] = b"misaka/positive/assigned-record-header/v2/r1";
pub const ASSIGNED_GENERATED_IDS_DOMAIN_V2: &[u8] = b"misaka/positive/assigned-generated-ids/v2/r1";

/// Purpose-specific native DATA references. This does not change the A0
/// ExecutionRecord/Witness/Result/output identities or authenticate acceptance.
/// Expected Result/decode count must come from the independently derived job.
pub fn assigned_witness_record_materials_v2(
    header: &misaka_palw_positive_contract::record::RecordHeader,
    generated_bytes: &[u8],
    expected_result: misaka_palw_positive_contract::identity::ResultId,
    expected_decode: u32,
    origin: AcceptedOrigin,
) -> Result<[AcceptedMaterial; 2], ContractError> {
    header.validate_version()?;
    if header.result != expected_result
        || expected_decode == 0
        || u64::from(expected_decode).checked_mul(4) != Some(generated_bytes.len() as u64)
    {
        return Err(ContractError::Identity);
    }
    let generated = blake2b_simd::Params::new().hash_length(64).key(ASSIGNED_GENERATED_IDS_DOMAIN_V2).hash(generated_bytes);
    Ok([
        AcceptedMaterial { origin, digest: hash(ASSIGNED_RECORD_HEADER_DOMAIN_V2, header) },
        AcceptedMaterial { origin, digest: Digest(generated.as_bytes().try_into().expect("64-byte hash")) },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn origin() -> AcceptedOrigin {
        AcceptedOrigin {
            accepting_block: NativeHash64([2; 64]),
            carrier_block: NativeHash64([3; 64]),
            carrier_transaction: NativeHash64([4; 64]),
            accepted_tx_ordinal: 2,
            event_ordinal: 0,
            accepted_daa: 10,
            accepted_blue_score: 11,
        }
    }
    #[test]
    fn witness_material_requires_independent_count_and_result_and_exact_raw_generated_bytes() {
        use misaka_palw_positive_contract::{identity::ResultId, record::RecordHeader};
        let result = ResultId(Digest([1; 64]));
        let header = RecordHeader {
            version: 2,
            revision: 1,
            result,
            artifact: Digest([2; 64]),
            full_logits_trace_root: Digest([3; 64]),
            step_leaf_count: 4,
            step_merkle_root: Digest([5; 64]),
            committed_execution_root: Digest([6; 64]),
        };
        let generated = [17u32.to_le_bytes(), 29u32.to_le_bytes()].concat();
        let refs = assigned_witness_record_materials_v2(&header, &generated, result, 2, origin()).unwrap();
        assert_ne!(refs[0], refs[1]);
        assert!(assigned_witness_record_materials_v2(&header, &generated, result, 1, origin()).is_err());
        assert!(assigned_witness_record_materials_v2(&header, &generated[..7], result, 2, origin()).is_err());
        assert!(assigned_witness_record_materials_v2(&header, &generated, ResultId(Digest([7; 64])), 2, origin()).is_err());
        let mut changed = generated.clone();
        changed[0] ^= 1;
        let other = assigned_witness_record_materials_v2(&header, &changed, result, 2, origin()).unwrap();
        assert_eq!(other[0], refs[0]);
        assert_ne!(other[1], refs[1]);
        // Raw generated bytes have no extra Borsh Vec length prefix.
        assert_ne!(refs[1].digest, hash(ASSIGNED_GENERATED_IDS_DOMAIN_V2, &generated));
    }
    #[test]
    fn initial_case_requires_the_exact_rooted_bind_and_prefix_material() {
        use misaka_palw_positive_contract::{
            assignment::{
                AssignmentId,
                journal::{AssignmentJournalEventKindV2, AssignmentJournalHeaderV2},
                rows::AssignmentUseRow,
            },
            identity::{ChainScope, ResultId},
            journal::{
                CaseJournalAux, CaseJournalEventKind, CaseJournalHeader, CaseJournalLimits, CaseJournalRecord, CaseJournalRecordView,
            },
        };

        let d = |n| Digest([n; 64]);
        let case = CaseId(d(1));
        let result = ResultId(d(2));
        let boundary = AssignmentCaseBoundaryV2 {
            version: 2,
            revision: 1,
            case,
            origin: origin(),
            selected_parent: NativeHash64(d(3).0),
            pre_event_root: d(4),
            before_delta_end: 5,
            populated_inputs: d(6),
            input_count: 1,
            fee: 7,
        };
        let bind = AssignmentJournalHeaderV2 {
            version: 2,
            revision: 1,
            assignment: AssignmentId(d(8)),
            ordinal: 1,
            previous: d(9),
            kind: AssignmentJournalEventKindV2::BindResult,
            origin: origin(),
            payload_digest: d(10),
            payload_bytes: 11,
        };
        let use_row = AssignmentUseRow {
            assignment: bind.assignment,
            case,
            result,
            origin: origin(),
            binding: AcceptedMaterial { origin: origin(), digest: bind.id() },
        };
        let mut record = CaseJournalRecord {
            header: CaseJournalHeader {
                version: 2,
                revision: 1,
                scope: ChainScope { network: d(11), ruleset: d(12), branch: d(13) },
                case,
                result,
                sequence: 1,
                previous: d(0),
                origin: origin(),
                kind: CaseJournalEventKind::NativeTransaction,
            },
            payload: vec![1],
            aux: vec![
                CaseJournalAux { material: boundary.material(), preimage: borsh::to_vec(&boundary).unwrap() },
                CaseJournalAux { material: use_row.binding, preimage: borsh::to_vec(&bind).unwrap() },
            ],
        };
        record.aux.sort_unstable_by_key(|aux| aux.material.digest.0);
        let limits = CaseJournalLimits {
            max_record_bytes: 4096,
            max_payload_bytes: 4096,
            max_aux_entries: 2,
            max_aux_preimage_bytes: 512,
            max_total_preimage_bytes: 4096,
            max_logical_heap_bytes: 4096,
        };
        let check = |record: &CaseJournalRecord, use_row: &AssignmentUseRow| {
            let bytes = record.encode(limits).unwrap();
            initial_assigned_case_bind_header_v2(CaseJournalRecordView::decode(&bytes, limits).unwrap(), boundary, use_row)
        };
        assert_eq!(check(&record, &use_row), Ok(bind));
        let wrong = AssignmentUseRow { assignment: AssignmentId(d(14)), ..use_row };
        assert!(check(&record, &wrong).is_err());
        let pos = record.aux.iter().position(|aux| aux.material == use_row.binding).unwrap();
        record.aux[pos].preimage[70] ^= 1;
        assert!(check(&record, &use_row).is_err());
        record.aux.remove(pos);
        assert!(check(&record, &use_row).is_err());
    }

    #[test]
    fn boundary_fixed_width_is_checked_before_borsh() {
        assert!(AssignmentCaseBoundaryV2::decode(&[0; ASSIGNED_CASE_BOUNDARY_BYTES_V2 - 1]).is_err());
        assert!(AssignmentCaseBoundaryV2::decode(&[0; ASSIGNED_CASE_BOUNDARY_BYTES_V2 + 1]).is_err());
        let boundary = AssignmentCaseBoundaryV2 {
            version: 2,
            revision: 1,
            case: CaseId(Digest([1; 64])),
            origin: AcceptedOrigin {
                accepting_block: NativeHash64([2; 64]),
                carrier_block: NativeHash64([3; 64]),
                carrier_transaction: NativeHash64([4; 64]),
                accepted_tx_ordinal: 2,
                event_ordinal: 0,
                accepted_daa: 10,
                accepted_blue_score: 11,
            },
            selected_parent: NativeHash64([5; 64]),
            pre_event_root: Digest([6; 64]),
            before_delta_end: 7,
            populated_inputs: Digest([8; 64]),
            input_count: 1,
            fee: 9,
        };
        let encoded = borsh::to_vec(&boundary).unwrap();
        assert_eq!(encoded.len(), ASSIGNED_CASE_BOUNDARY_BYTES_V2);
        assert_eq!(AssignmentCaseBoundaryV2::decode(&encoded).unwrap(), boundary);
        let mut changed = boundary;
        changed.before_delta_end += 1;
        assert_ne!(changed.id(), boundary.id());
        changed = boundary;
        changed.pre_event_root.0[0] ^= 1;
        assert_ne!(changed.id(), boundary.id());
    }
}
