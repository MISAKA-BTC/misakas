//! Append-only shared journal data in the native PALW database. The selected
//! aux151 head remains authority; orphan raw records survive reorg/reindex.
//! This store establishes data identity/availability, never accepted provenance,
//! a court result, transaction qualification, UTXOs, payment, or credit.
use super::{DbPalwStateV2Store, NativeStoreWriteTicketV1, native_writer_is_strict_v1};
use kaspa_consensus_core::{
    palw_native_resource_v1::{actual_hard_database_quota_v1, preflight_native_value_v1, reserve_native_requests_v1},
    palw_positive_court_v2::coordinator::assignment_body::PositiveAssignmentNativeBodyV2,
    palw_positive_journal_chain_v2::CaseJournalSourceV2,
    palw_state_v2::{PalwDeltaEntryV2, PalwStateDeltaV2},
    subnets::{SUBNETWORK_ID_PALW_LIFECYCLE, SUBNETWORK_ID_SIZE},
};
use kaspa_database::{
    prelude::{DbKey, StoreError, StoreResult},
    registry::DatabaseStorePrefixes,
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::{CaseId, Digest},
    journal::{CaseJournalEventKind, CaseJournalHeader, CaseJournalLimits, CaseJournalRecordView},
    native::{KERNEL_AUX_TABLE, NativeRowKey},
    rows::{AcceptedMaterial, CaseRow, JournalHeadRow, MAX_NATIVE_KEY_BYTES, MAX_NATIVE_ROW_BYTES, NativeRowBody, NativeRowValue},
};
use rocksdb::WriteBatch;
use std::mem::size_of;

const JOURNAL_SUFFIX: u8 = 0x4a;
fn journal_key(head: Digest) -> [u8; 66] {
    let mut key = [0; 66];
    key[0] = u8::from(DatabaseStorePrefixes::PalwStateV2Tip);
    key[1] = JOURNAL_SUFFIX;
    key[2..].copy_from_slice(&head.0);
    key
}
fn corrupt(why: &str) -> StoreError {
    StoreError::DataInconsistency(format!("positive journal: {why}"))
}
fn capacity(why: &str) -> StoreError {
    StoreError::LocalResourceUnavailable(format!("positive journal: {why}"))
}
fn add(a: u64, b: u64) -> StoreResult<u64> {
    a.checked_add(b).ok_or_else(|| capacity("checked sum overflow"))
}
fn mul(a: u64, b: u64) -> StoreResult<u64> {
    a.checked_mul(b).ok_or_else(|| capacity("checked product overflow"))
}
fn bounded(value: u64, limit: u64, what: &str) -> StoreResult<u64> {
    if value > limit { Err(capacity(what)) } else { Ok(value) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveJournalStoreLimitsV2 {
    pub max_records: u64,
    pub max_delta_entries: u64,
    pub record_limits: CaseJournalLimits,
    /// New input records plus conservatively bounded predecessor DB reads.
    pub max_total_raw_bytes: u64,
    pub max_hash_bytes: u64,
    pub max_index_bytes: u64,
    /// Complete caller batch, including operations staged before this call.
    pub max_batch_bytes: u64,
    /// Raw borrowed records and aux151 delta buffers, both metadata arrays,
    /// fixed decode workspace and the complete caller batch can coexist; all
    /// are included. Other delta families retain their native owner's budget.
    pub max_retained_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveJournalStoreQuoteV2 {
    pub records: u64,
    pub delta_entries: u64,
    pub total_raw_bytes: u64,
    /// Conservative predecessor-read allowance, checked before pinned DB reads.
    pub predecessor_read_bound_bytes: u64,
    /// Actually read and checked predecessor bytes, not another reservation.
    pub predecessor_record_bytes: u64,
    /// Both domains over new and predecessor raw records, actually checked.
    /// This is already contained in `hash_bytes`, and must not be added again.
    pub raw_identity_hash_bytes: u64,
    /// Reservation upper bound, including fixed aux151 identity subhashes and
    /// the bounded predecessor reads; distinct from actual raw hash work.
    pub hash_bytes: u64,
    pub index_bytes: u64,
    pub decode_workspace_bytes: u64,
    pub batch_added_bytes: u64,
    pub total_batch_bytes: u64,
    /// Simultaneous LOCAL memory reservation, including caller buffers and
    /// predecessor reads. This is NOT canonical Case.usage.retained_bytes and
    /// must never be added to a Case fee/retention meter. The native owner
    /// prices canonical new material bytes separately from capacities/caches.
    pub retained_bytes: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowState {
    Case(CaseRow),
    Head(JournalHeadRow),
}
impl RowState {
    fn case(self) -> CaseId {
        match self {
            Self::Case(row) => row.case,
            Self::Head(row) => row.case,
        }
    }
    fn kind(self) -> u8 {
        match self {
            Self::Case(_) => 0,
            Self::Head(_) => 1,
        }
    }
}
struct RowSlot {
    case: CaseId,
    kind: u8,
    order: usize,
    old: Option<RowState>,
    new: Option<RowState>,
}
struct RecordSlot<'a> {
    raw: &'a [u8],
    header: CaseJournalHeader,
    head: Digest,
    history: AcceptedMaterial,
    valid_initial: bool,
}
const FIXED_ROW_WORKSPACE: usize =
    MAX_NATIVE_ROW_BYTES + MAX_NATIVE_KEY_BYTES + 8 * size_of::<NativeRowValue>() + 2 * size_of::<NativeRowKey>();

fn decode_row(key: &[u8], bytes: &[u8]) -> StoreResult<Option<RowState>> {
    let value = NativeRowValue::decode(key, bytes).map_err(|_| corrupt("aux151 old/new key/value is invalid"))?;
    Ok(match value.body {
        NativeRowBody::Case(row) => Some(RowState::Case(row)),
        NativeRowBody::JournalHead(row) => Some(RowState::Head(row)),
        _ => None,
    })
}

/// Borrow the canonical Borsh Transaction's payload and cached identity without
/// allocating any inputs, outputs, or scripts from a stored journal record.
fn initial_assignment_transaction(raw: &[u8]) -> Option<(&[u8], &[u8; 64])> {
    fn read_count(raw: &mut &[u8]) -> Option<usize> {
        let (count, remaining) = raw.split_first_chunk::<4>()?;
        *raw = remaining;
        usize::try_from(u32::from_le_bytes(*count)).ok()
    }
    fn read_vec<'a>(raw: &mut &'a [u8]) -> Option<&'a [u8]> {
        let count = read_count(raw)?;
        let (value, remaining) = raw.split_at_checked(count)?;
        *raw = remaining;
        Some(value)
    }

    let mut rest = raw.get(2..)?; // version
    let inputs = read_count(&mut rest)?;
    if inputs == 0 {
        return None;
    }
    for _ in 0..inputs {
        rest = rest.get(64 + 4..)?; // previous outpoint
        read_vec(&mut rest)?; // signature script
        rest = rest.get(8 + 1..)?; // sequence and sigop count
    }
    let outputs = read_count(&mut rest)?;
    for _ in 0..outputs {
        rest = rest.get(8 + 2..)?; // amount and script version
        read_vec(&mut rest)?;
    }
    let (subnetwork, tail) = rest.get(8..)?.split_at_checked(SUBNETWORK_ID_SIZE)?; // lock time and subnetwork
    let lifecycle: &[u8] = SUBNETWORK_ID_PALW_LIFECYCLE.as_ref();
    if subnetwork != lifecycle {
        return None;
    }
    rest = tail.get(8..)?; // gas
    let payload = read_vec(&mut rest)?;
    Some((payload, rest.get(8..)?.try_into().ok()?)) // mass and exact cached txid
}

fn valid_initial_case_record(record: CaseJournalRecordView<'_>) -> bool {
    let header = record.header();
    match header.kind {
        CaseJournalEventKind::NativeBody => record.payload().get(4) == Some(&0),
        CaseJournalEventKind::NativeTransaction => {
            let Some((payload, id)) = initial_assignment_transaction(record.payload()) else { return false };
            header.origin.event_ordinal == 0
                && id == &header.origin.carrier_transaction.0
                && payload.get(..8) == Some(&[1, 0, 176, 2, 0, 1, 0, 9])
                && preflight_native_value_v1::<PositiveAssignmentNativeBodyV2>(&payload[3..], (payload.len() - 3) as u64).is_ok()
        }
    }
}

fn varint_bytes(mut n: u64) -> u64 {
    let mut width = 1;
    while n >= 128 {
        width += 1;
        n >>= 7;
    }
    width
}
fn put_bytes(value: u64) -> StoreResult<u64> {
    // RocksDB WriteBatch Put: tag, varint32 key length, key, varint32 value
    // length, value. The existing batch already owns its 12-byte header.
    if value > u32::MAX as u64 {
        return Err(capacity("RocksDB value length exceeds varint32"));
    }
    add(add(1 + 1 + 66, varint_bytes(value))?, value)
}

impl DbPalwStateV2Store {
    /// Stages the separate original Case journal from the SAME retained native
    /// transition output. Assignment records are never re-labelled Case IDs.
    pub fn stage_positive_case_material_outcome_v2(
        &mut self,
        batch: &mut WriteBatch,
        delta: &PalwStateDeltaV2,
        outcome: &kaspa_consensus_core::palw_state_v2::PositiveAssignmentMaterialOutcomeV2,
    ) -> StoreResult<Option<(PositiveJournalStoreQuoteV2, NativeStoreWriteTicketV1)>> {
        if outcome.case_records().is_empty() {
            for entry in &delta.entries {
                let PalwDeltaEntryV2::KernelRouteRow { table, key, new: Some(_), .. } = entry else {
                    continue;
                };
                if *table == KERNEL_AUX_TABLE && matches!(key.get(4), Some(2 | 9)) {
                    return Err(corrupt("Case mutation has no retained native Case material outcome"));
                }
            }
            return Ok(None);
        }
        let count = outcome.case_records().len() as u64;
        let _refs_lease =
            reserve_native_requests_v1(mul(count, 2 * size_of::<&[u8]>() as u64)?).map_err(|e| capacity(&e.to_string()))?;
        let mut refs = Vec::new();
        refs.try_reserve_exact(count as usize).map_err(|_| capacity("typed Case references"))?;
        let mut raw = 0;
        let mut added = 0;
        let mut bounds = outcome.case_records()[0].limits();
        for record in outcome.case_records() {
            let cap = record.limits();
            bounds.max_record_bytes = bounds.max_record_bytes.max(cap.max_record_bytes);
            bounds.max_payload_bytes = bounds.max_payload_bytes.max(cap.max_payload_bytes);
            bounds.max_aux_entries = bounds.max_aux_entries.max(cap.max_aux_entries);
            bounds.max_aux_preimage_bytes = bounds.max_aux_preimage_bytes.max(cap.max_aux_preimage_bytes);
            bounds.max_total_preimage_bytes = bounds.max_total_preimage_bytes.max(cap.max_total_preimage_bytes);
            bounds.max_logical_heap_bytes = bounds.max_logical_heap_bytes.max(cap.max_logical_heap_bytes);
            raw = add(raw, record.raw().len() as u64)?;
            added = add(added, put_bytes(record.raw().len() as u64)?)?;
            refs.push(record.raw());
        }
        let mut row_bytes = 0;
        let mut row_count = 0;
        let mut old_heads = 0;
        for entry in &delta.entries {
            let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry else {
                continue;
            };
            if *table != KERNEL_AUX_TABLE {
                continue;
            }
            row_count = add(row_count, 1)?;
            if key.get(4) == Some(&9) && old.is_some() {
                old_heads = add(old_heads, 1)?;
            }
            for value in [old, new].into_iter().flatten() {
                row_bytes = add(row_bytes, add(key.len() as u64, value.len() as u64)?)?;
            }
        }
        let total_raw = add(raw, mul(old_heads, bounds.max_record_bytes)?)?;
        let hash = add(mul(total_raw, 2)?, mul(row_bytes, 4)?)?;
        let index = add(mul(count, size_of::<RecordSlot<'_>>() as u64)?, mul(row_count, size_of::<RowSlot>() as u64)?)?;
        let batch_bytes = add(batch.size_in_bytes() as u64, added)?;
        let workspace = add(
            FIXED_ROW_WORKSPACE as u64,
            kaspa_consensus_core::palw_native_resource_v1::positive_journal::POSITIVE_JOURNAL_DECODE_WORKSPACE_BYTES_V1 as u64,
        )?;
        let retained = add(add(add(add(total_raw, row_bytes)?, index)?, batch_bytes)?, workspace)?;
        let limits = PositiveJournalStoreLimitsV2 {
            max_records: count,
            max_delta_entries: delta.entries.len() as u64,
            record_limits: bounds,
            max_total_raw_bytes: total_raw,
            max_hash_bytes: hash,
            max_index_bytes: index,
            max_batch_bytes: batch_bytes,
            max_retained_bytes: retained,
        };
        self.stage_positive_case_journal_batch_v2(batch, delta, &refs, limits).map(Some)
    }

    /// Caller must hold the SAME native PALW write guard from state preparation
    /// through the owning DB batch commit, and retain the returned ticket until
    /// commit/abandonment. `&mut self` and this allocation ticket are not a global
    /// exclusion guard. No DB write or cache publication occurs here.
    ///
    /// All validation, reads and quota checks precede the first batch mutation.
    /// Native aux151 Case/JournalHead old/new and undo remain in `delta`, staged
    /// by the ordinary native writer in this SAME batch. Raw records are never
    /// deleted on rollback; they cannot select their own active case head.
    /// Stage the complete native transition once: its initial old heads must
    /// already be in the DB; intermediate heads supplied in this call need not.
    pub fn stage_positive_case_journal_batch_v2(
        &mut self,
        batch: &mut WriteBatch,
        delta: &PalwStateDeltaV2,
        records: &[&[u8]],
        limits: PositiveJournalStoreLimitsV2,
    ) -> StoreResult<(PositiveJournalStoreQuoteV2, NativeStoreWriteTicketV1)> {
        limits.record_limits.validate().map_err(|_| capacity("record limits are not explicit/nonzero"))?;
        if [
            limits.max_records,
            limits.max_delta_entries,
            limits.max_total_raw_bytes,
            limits.max_hash_bytes,
            limits.max_index_bytes,
            limits.max_batch_bytes,
            limits.max_retained_bytes,
        ]
        .contains(&0)
        {
            return Err(capacity("store limits must be finite and nonzero"));
        }
        let count = bounded(records.len() as u64, limits.max_records, "record count limit")?;
        let entries = bounded(delta.entries.len() as u64, limits.max_delta_entries, "delta entry count limit")?;
        let mut raw_bytes = 0;
        let mut added_batch = 0;
        for raw in records {
            bounded(raw.len() as u64, limits.record_limits.max_record_bytes, "record frame limit")?;
            raw_bytes = bounded(add(raw_bytes, raw.len() as u64)?, limits.max_total_raw_bytes, "total raw record limit")?;
            added_batch = add(added_batch, put_bytes(raw.len() as u64)?)?;
        }
        let mut row_bytes = 0;
        let mut row_count = 0;
        let mut old_head_rows = 0;
        for entry in &delta.entries {
            if let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry {
                if *table != KERNEL_AUX_TABLE {
                    continue;
                }
                if key.len() > MAX_NATIVE_KEY_BYTES {
                    return Err(corrupt("aux151 key exceeds codec cap"));
                }
                row_count = add(row_count, 1)?;
                // A raw key-byte classification only supplies an upper bound;
                // complete key/value version/kind validation follows reservation.
                if key.get(4) == Some(&9) && old.is_some() {
                    old_head_rows = add(old_head_rows, 1)?;
                }
                for value in [old, new].into_iter().flatten() {
                    if value.len() > MAX_NATIVE_ROW_BYTES {
                        return Err(corrupt("aux151 VALUE exceeds codec cap"));
                    }
                    row_bytes = add(row_bytes, add(key.len() as u64, value.len() as u64)?)?;
                }
            }
        }
        // Each raw record uses both independent domains. Shared fixed row
        // identity validation hashes at most four canonical subpreimages, each
        // no longer than its input key+VALUE; this is work, not heap scaling.
        let predecessor_bound = mul(old_head_rows, limits.record_limits.max_record_bytes)?;
        bounded(add(raw_bytes, predecessor_bound)?, limits.max_total_raw_bytes, "new/predecessor raw limit")?;
        let hash_bytes =
            bounded(add(mul(add(raw_bytes, predecessor_bound)?, 2)?, mul(row_bytes, 4)?)?, limits.max_hash_bytes, "hash-work limit")?;
        let index_bytes = bounded(
            add(mul(count, size_of::<RecordSlot<'_>>() as u64)?, mul(row_count, size_of::<RowSlot>() as u64)?)?,
            limits.max_index_bytes,
            "metadata/index limit",
        )?;
        let total_batch = bounded(add(batch.size_in_bytes() as u64, added_batch)?, limits.max_batch_bytes, "complete batch limit")?;
        let workspace = add(
            FIXED_ROW_WORKSPACE as u64,
            kaspa_consensus_core::palw_native_resource_v1::positive_journal::POSITIVE_JOURNAL_DECODE_WORKSPACE_BYTES_V1 as u64,
        )?;
        let retained = bounded(
            add(add(add(add(raw_bytes, predecessor_bound)?, row_bytes)?, index_bytes)?, add(total_batch, workspace)?)?,
            limits.max_retained_bytes,
            "simultaneous retained memory limit",
        )?;
        // Reserve before either metadata Vec or any typed fixed-row decoder.
        let lease = if native_writer_is_strict_v1() {
            reserve_native_requests_v1(retained).map_err(|error| capacity(&error.to_string()))?
        } else {
            None
        };
        if native_writer_is_strict_v1() {
            let path =
                self.db.native_single_directory_quota_path_v1().ok_or_else(|| capacity("database layout lacks quota provenance"))?;
            let quota = actual_hard_database_quota_v1(path).map_err(|error| capacity(&error.to_string()))?;
            bounded(mul(added_batch, 2)?, quota.headroom_bytes(), "database WAL/write headroom")?;
        }
        let mut row_slots = Vec::new();
        row_slots
            .try_reserve_exact(usize::try_from(row_count).map_err(|_| capacity("row count address space"))?)
            .map_err(|_| capacity("row metadata reservation"))?;
        let mut record_slots = Vec::new();
        record_slots.try_reserve_exact(records.len()).map_err(|_| capacity("record metadata reservation"))?;
        let actual_index = add(
            mul(row_slots.capacity() as u64, size_of::<RowSlot>() as u64)?,
            mul(record_slots.capacity() as u64, size_of::<RecordSlot<'_>>() as u64)?,
        )?;
        if actual_index > index_bytes {
            return Err(capacity("metadata capacity exceeded reserved exact slots"));
        }
        for (order, entry) in delta.entries.iter().enumerate() {
            let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry else { continue };
            if *table != KERNEL_AUX_TABLE {
                continue;
            }
            let old_state = old.as_deref().map(|v| decode_row(key, v)).transpose()?.flatten();
            let new_state = new.as_deref().map(|v| decode_row(key, v)).transpose()?.flatten();
            if old.is_none() && new.is_none() {
                return Err(corrupt("empty aux151 delta"));
            }
            if let Some(row) = old_state.or(new_state) {
                if new_state.is_some_and(|n| n.kind() != row.kind() || n.case() != row.case()) {
                    return Err(corrupt("old/new aux151 row kind/case changed"));
                }
                row_slots.push(RowSlot { case: row.case(), kind: row.kind(), order, old: old_state, new: new_state });
            }
        }
        for &raw in records {
            let view = CaseJournalRecordView::decode(raw, limits.record_limits).map_err(|error| {
                if error == ContractError::Budget { capacity("record policy limit") } else { corrupt("record codec invalid") }
            })?;
            let head = view.head();
            let history = view.history();
            if head == history.digest {
                return Err(corrupt("head/history domains were aliased"));
            }
            let header = view.header();
            let valid_initial = header.sequence == 1 && valid_initial_case_record(view);
            record_slots.push(RecordSlot { raw, header, head, history, valid_initial });
        }
        row_slots.sort_unstable_by_key(|row| (row.case, row.kind, row.order));
        record_slots.sort_unstable_by_key(|record| (record.header.case, record.header.sequence));
        validate_delta_links(&row_slots, &record_slots)?;
        let predecessor_bytes = self.validate_positive_journal_predecessors_v2(&row_slots, &record_slots, limits.record_limits)?;
        bounded(predecessor_bytes, predecessor_bound, "predecessor read allowance exceeded")?;
        let raw_identity_hash_bytes = mul(add(raw_bytes, predecessor_bytes)?, 2)?;
        // Existing content is immutable. Equal bytes are an idempotent append;
        // unequal bytes at this same content address are disk corruption.
        for record in &record_slots {
            if let Some(existing) = self.db.get_pinned(journal_key(record.head))?
                && (existing.len() != record.raw.len() || existing.as_ref() != record.raw)
            {
                return Err(corrupt("content-addressed key already has different bytes"));
            }
        }
        let quote = PositiveJournalStoreQuoteV2 {
            records: count,
            delta_entries: entries,
            total_raw_bytes: raw_bytes,
            predecessor_read_bound_bytes: predecessor_bound,
            predecessor_record_bytes: predecessor_bytes,
            raw_identity_hash_bytes,
            hash_bytes,
            index_bytes: actual_index,
            decode_workspace_bytes: workspace,
            batch_added_bytes: added_batch,
            total_batch_bytes: total_batch,
            retained_bytes: retained,
        };
        // Infallible staging phase: no fallible validation/read/quota operation
        // may be added after this point. The caller's existing bytes survive
        // every returned error unchanged.
        for record in record_slots {
            batch.put(journal_key(record.head), record.raw);
        }
        debug_assert_eq!(batch.size_in_bytes() as u64, total_batch);
        Ok((quote, NativeStoreWriteTicketV1 { _lease: lease }))
    }

    fn validate_positive_journal_predecessors_v2(
        &self,
        rows: &[RowSlot],
        records: &[RecordSlot<'_>],
        limits: CaseJournalLimits,
    ) -> StoreResult<u64> {
        let mut offset = 0;
        let mut actual_bytes = 0;
        while offset < rows.len() {
            let case = rows[offset].case;
            let end = offset + rows[offset..].partition_point(|row| row.case == case);
            let head_row = rows[offset..end].iter().find(|row| row.kind == 1);
            if let Some(Some(RowState::Head(old))) = head_row.map(|row| row.old) {
                let record_offset = records.partition_point(|record| record.header.case < case);
                let next = records.get(record_offset).ok_or_else(|| corrupt("missing append record for predecessor"))?;
                let bytes =
                    self.db.get_pinned(journal_key(old.head))?.ok_or_else(|| corrupt("selected predecessor raw record is missing"))?;
                if bytes.len() as u64 > limits.max_record_bytes {
                    return Err(capacity("selected predecessor exceeds record bound"));
                }
                actual_bytes = add(actual_bytes, bytes.len() as u64)?;
                let view = CaseJournalRecordView::decode(&bytes, limits).map_err(|error| {
                    if error == ContractError::Budget {
                        capacity("selected predecessor record policy")
                    } else {
                        corrupt("selected predecessor codec is invalid")
                    }
                })?;
                let header = view.header();
                // Both hashes were conservatively reserved before this DB read.
                if view.head() != old.head
                    || view.history() != old.history
                    || header.previous != old.previous
                    || header.sequence != old.sequence
                    || header.case != case
                    || header.scope != next.header.scope
                    || header.result != next.header.result
                {
                    return Err(corrupt("selected predecessor differs from old native JournalHead/Case"));
                }
                if header.sequence == 1 && !valid_initial_case_record(view) {
                    return Err(corrupt("selected predecessor initial event is invalid"));
                }
            }
            offset = end;
        }
        Ok(actual_bytes)
    }

    /// Lend raw pinned bytes only, checking the bound before copying (there is
    /// no copy). The callback MUST pass them through the bounded chain/replay
    /// checker, which reserves hash work before verifying BOTH identities.
    /// This reader confirms no identity, provenance or active-head status;
    /// an available raw value or cache is never authority.
    pub fn with_positive_journal_record_v2<T>(&self, head: Digest, max_bytes: u64, visit: impl FnOnce(&[u8]) -> T) -> StoreResult<T> {
        if max_bytes == 0 {
            return Err(capacity("zero raw read limit"));
        }
        let key = journal_key(head);
        let bytes = self.db.get_pinned(key)?.ok_or_else(|| StoreError::KeyNotFound(DbKey::new(&key[..2], head.0)))?;
        if bytes.len() as u64 > max_bytes {
            return Err(capacity("raw read exceeds caller bound"));
        }
        Ok(visit(&bytes))
    }
}
impl CaseJournalSourceV2 for DbPalwStateV2Store {
    type Error = StoreError;
    fn with_record<T>(&self, head: Digest, max_bytes: u64, visit: impl FnOnce(&[u8]) -> T) -> StoreResult<T> {
        self.with_positive_journal_record_v2(head, max_bytes, visit)
    }
}

fn validate_delta_links(rows: &[RowSlot], records: &[RecordSlot<'_>]) -> StoreResult<()> {
    let mut next_row = 0;
    let mut next_record = 0;
    while next_row < rows.len() {
        let case = rows[next_row].case;
        let row_end = next_row + rows[next_row..].partition_point(|row| row.case == case);
        if next_record < records.len() && records[next_record].header.case < case {
            return Err(corrupt("unreferenced raw record"));
        }
        let record_end = next_record + records[next_record..].partition_point(|record| record.header.case == case);
        let group = &rows[next_row..row_end];
        let split = group.partition_point(|row| row.kind == 0);
        if split < group.len() {
            validate_case(&group[..split], &group[split..], &records[next_record..record_end])?;
        } else if record_end != next_record {
            return Err(corrupt("raw record has no JournalHead delta"));
        }
        next_row = row_end;
        next_record = record_end;
    }
    if next_record != records.len() {
        return Err(corrupt("unreferenced raw record"));
    }
    Ok(())
}

fn validate_case(cases: &[RowSlot], heads: &[RowSlot], records: &[RecordSlot<'_>]) -> StoreResult<()> {
    if cases.is_empty() || records.is_empty() {
        return Err(corrupt("JournalHead delta requires Case delta and raw records"));
    }
    for pair in cases.windows(2) {
        if pair[0].new != pair[1].old {
            return Err(corrupt("Case intermediate delta is discontinuous"));
        }
    }
    for pair in heads.windows(2) {
        if pair[0].new != pair[1].old {
            return Err(corrupt("JournalHead intermediate delta is discontinuous"));
        }
    }
    let first = &records[0];
    let old_head = match heads[0].old {
        None => None,
        Some(RowState::Head(row)) => Some(row),
        _ => return Err(corrupt("wrong head row")),
    };
    let old_case = match cases[0].old {
        None => None,
        Some(RowState::Case(row)) => Some(row),
        _ => return Err(corrupt("wrong Case row")),
    };
    if old_head.is_some() != old_case.is_some() {
        return Err(corrupt("old Case/JournalHead presence mismatch"));
    }
    if let (Some(case), Some(head)) = (old_case, old_head)
        && (case.result != first.header.result
            || case.coverage.scope != first.header.scope
            || case.last_event != head.history.origin
            || head.sequence == 0
            || head.head == head.history.digest)
    {
        return Err(corrupt("old Case/JournalHead identity mismatch"));
    }
    let mut previous = old_head.map_or(Digest([0; 64]), |row| row.head);
    let mut sequence = old_head.map_or(0, |row| row.sequence);
    let mut offset = 0;
    for row in heads {
        let Some(RowState::Head(new)) = row.new else {
            return Err(corrupt("append cannot delete JournalHead"));
        };
        if new.sequence <= sequence {
            return Err(corrupt("JournalHead sequence must advance"));
        }
        while sequence < new.sequence {
            let record = records.get(offset).ok_or_else(|| corrupt("new JournalHead has no raw record"))?;
            sequence = sequence.checked_add(1).ok_or_else(|| corrupt("journal sequence overflow"))?;
            if record.header.sequence != sequence
                || record.header.previous != previous
                || record.header.scope != first.header.scope
                || record.header.result != first.header.result
            {
                return Err(corrupt("raw record sequence/scope/result/previous mismatch"));
            }
            if sequence == 1 && !record.valid_initial {
                return Err(corrupt("invalid initial Case journal event"));
            }
            previous = record.head;
            offset += 1;
        }
        let last = &records[offset - 1];
        if new.case != last.header.case || new.previous != last.header.previous || new.head != last.head || new.history != last.history
        {
            return Err(corrupt("new JournalHead differs from final literal record"));
        }
    }
    if offset != records.len() {
        return Err(corrupt("extra raw records beyond final JournalHead"));
    }
    let Some(RowState::Case(final_case)) = cases.last().and_then(|row| row.new) else {
        return Err(corrupt("append requires final Case"));
    };
    let last = &records[offset - 1];
    if final_case.case != last.header.case
        || final_case.result != last.header.result
        || final_case.coverage.scope != last.header.scope
        || final_case.last_event != last.header.origin
    {
        return Err(corrupt("final Case differs from final literal record"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
