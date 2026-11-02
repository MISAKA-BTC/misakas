//! Immutable assignment material in the actual PALW RocksDB. The current
//! rooted aux151 head selects history; orphan bytes cannot establish ancestry,
//! successful replay, a lock, computation or credit. Same-batch state undo is
//! owned by the ordinary native writer; raw material survives a reorg.
use super::{DbPalwStateV2Store, NativeStoreWriteTicketV1, native_writer_is_strict_v1};
use kaspa_consensus_core::{
    palw_native_resource_v1::{actual_hard_database_quota_v1, reserve_native_requests_v1},
    palw_state_v2::{PalwDeltaEntryV2, PalwStateDeltaV2},
};
use kaspa_database::{
    prelude::{DbKey, StoreError, StoreResult},
    registry::DatabaseStorePrefixes,
};
use misaka_palw_positive_contract::{
    assignment::journal::*,
    identity::Digest,
    native::{KERNEL_AUX_TABLE, NativeRowKey},
    rows::{MAX_NATIVE_KEY_BYTES, MAX_NATIVE_ROW_BYTES, NativeRowBody, NativeRowValue},
};
use rocksdb::WriteBatch;
use std::mem::size_of;
const ASSIGNMENT_JOURNAL_SUFFIX: u8 = 0x4b;
fn key(id: Digest) -> [u8; 66] {
    let mut k = [0; 66];
    k[0] = u8::from(DatabaseStorePrefixes::PalwStateV2Tip);
    k[1] = ASSIGNMENT_JOURNAL_SUFFIX;
    k[2..].copy_from_slice(&id.0);
    k
}
fn corrupt(s: &str) -> StoreError {
    StoreError::DataInconsistency(format!("assignment journal: {s}"))
}
fn capacity(s: &str) -> StoreError {
    StoreError::LocalResourceUnavailable(format!("assignment journal: {s}"))
}
fn add(a: u64, b: u64) -> StoreResult<u64> {
    a.checked_add(b).ok_or_else(|| capacity("sum overflow"))
}
fn mul(a: u64, b: u64) -> StoreResult<u64> {
    a.checked_mul(b).ok_or_else(|| capacity("product overflow"))
}
fn bound(a: u64, b: u64) -> StoreResult<u64> {
    if a > b { Err(capacity("finite budget exceeded")) } else { Ok(a) }
}
fn varint(mut n: u64) -> u64 {
    let mut v = 1;
    while n >= 128 {
        n >>= 7;
        v += 1;
    }
    v
}
fn put_bytes(n: u64) -> StoreResult<u64> {
    bound(n, u32::MAX as u64)?;
    add(68 + varint(n), n)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentJournalStoreLimitsV2 {
    pub max_records: u64,
    pub max_delta_entries: u64,
    pub max_record_bytes: u64,
    pub max_total_raw_bytes: u64,
    pub max_work: u64,
    pub max_retained_bytes: u64,
    pub max_batch_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentJournalStoreQuoteV2 {
    pub records: u64,
    pub raw_bytes: u64,
    pub predecessor_read_bound: u64,
    pub work_bound: u64,
    pub retained_bytes: u64,
    pub batch_added_bytes: u64,
    pub total_batch_bytes: u64,
}
fn decode(key: &[u8], value: &[u8]) -> StoreResult<NativeRowBody> {
    NativeRowValue::decode(key, value).map(|r| r.body).map_err(|_| corrupt("malformed aux151 key/value"))
}
impl DbPalwStateV2Store {
    /// Hold the native write guard through this staging and the batch commit.
    /// All reads, limits and checks finish before this function touches batch.
    /// Actual provenance is independently checked by the private native reader.
    pub fn stage_positive_assignment_journal_batch_v2(
        &mut self,
        batch: &mut WriteBatch,
        delta: &PalwStateDeltaV2,
        records: &[&[u8]],
        limits: AssignmentJournalStoreLimitsV2,
    ) -> StoreResult<(AssignmentJournalStoreQuoteV2, NativeStoreWriteTicketV1)> {
        if [
            limits.max_records,
            limits.max_delta_entries,
            limits.max_record_bytes,
            limits.max_total_raw_bytes,
            limits.max_work,
            limits.max_retained_bytes,
            limits.max_batch_bytes,
        ]
        .into_iter()
        .any(|n| n == 0 || n == u64::MAX)
        {
            return Err(capacity("limits must be explicit finite nonzero"));
        }
        bound(records.len() as u64, limits.max_records)?;
        bound(delta.entries.len() as u64, limits.max_delta_entries)?;
        let mut raw_bytes = 0;
        let mut added = 0;
        let mut row_bytes = 0;
        let mut heads = 0;
        for raw in records {
            bound(raw.len() as u64, limits.max_record_bytes)?;
            raw_bytes = add(raw_bytes, raw.len() as u64)?;
            added = add(added, put_bytes(raw.len() as u64)?)?;
        }
        for entry in &delta.entries {
            if let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry {
                if *table != KERNEL_AUX_TABLE {
                    continue;
                }
                bound(key.len() as u64, MAX_NATIVE_KEY_BYTES as u64)?;
                if key.get(4) == Some(&16) {
                    heads = add(heads, 1)?;
                }
                for bytes in [old, new].into_iter().flatten() {
                    bound(bytes.len() as u64, MAX_NATIVE_ROW_BYTES as u64)?;
                    row_bytes = add(row_bytes, add(key.len() as u64, bytes.len() as u64)?)?;
                }
            }
        }
        let predecessor = mul(heads, limits.max_record_bytes)?;
        bound(add(raw_bytes, predecessor)?, limits.max_total_raw_bytes)?;
        // Explicitly reserve bounded nested delta matching and all raw hashes.
        let work = add(mul(add(raw_bytes, predecessor)?, 2)?, mul(mul(add(records.len() as u64, 1)?, row_bytes)?, 4)?)?;
        bound(work, limits.max_work)?;
        let total_batch = bound(add(batch.size_in_bytes() as u64, added)?, limits.max_batch_bytes)?;
        let scratch =
            (MAX_NATIVE_ROW_BYTES + MAX_NATIVE_KEY_BYTES + 8 * size_of::<NativeRowValue>() + 2 * size_of::<NativeRowKey>()) as u64;
        let retained = bound(
            add(
                add(add(raw_bytes, predecessor)?, row_bytes)?,
                add(add(total_batch, scratch)?, mul(records.len() as u64, size_of::<AssignmentJournalRecordViewV2<'_>>() as u64)?)?,
            )?,
            limits.max_retained_bytes,
        )?;
        let lease = if native_writer_is_strict_v1() {
            reserve_native_requests_v1(retained).map_err(|e| capacity(&e.to_string()))?
        } else {
            None
        };
        if native_writer_is_strict_v1() {
            let path = self.db.native_single_directory_quota_path_v1().ok_or_else(|| capacity("database quota provenance missing"))?;
            let quota = actual_hard_database_quota_v1(path).map_err(|e| capacity(&e.to_string()))?;
            bound(mul(added, 2)?, quota.headroom_bytes())?;
        }
        let mut views = Vec::new();
        views.try_reserve_exact(records.len()).map_err(|_| capacity("record metadata"))?;
        if views.capacity() > records.len() {
            return Err(capacity("record metadata capacity exceeded reservation"));
        }
        for &raw in records {
            views.push(
                AssignmentJournalRecordViewV2::decode(raw, limits.max_record_bytes)
                    .map_err(|_| corrupt("invalid bounded literal record"))?,
            );
        }
        for (i, view) in views.iter().enumerate() {
            let frame = view.frame();
            let header = view.header();
            let id = header.id();
            if views[..i]
                .iter()
                .any(|v| v.header().id() == id || (v.header().assignment == header.assignment && v.header().ordinal == header.ordinal))
            {
                return Err(corrupt("duplicate immutable frame"));
            }
            let mut matched_frame = false;
            let mut matched_head = false;
            let mut matched_assignment = false;
            for entry in &delta.entries {
                let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry else { continue };
                if *table != KERNEL_AUX_TABLE {
                    continue;
                }
                if key == &(NativeRowKey::AssignmentJournalFrame { assignment: header.assignment, ordinal: header.ordinal }).encode() {
                    if matched_frame || old.is_some() {
                        return Err(corrupt("frame overwrite/repeated delta"));
                    }
                    let Some(bytes) = new else { return Err(corrupt("frame deleted")) };
                    if decode(key, bytes)? != NativeRowBody::AssignmentJournalFrame(frame) {
                        return Err(corrupt("frame differs from literal material"));
                    }
                    matched_frame = true;
                }
                if key == &NativeRowKey::AssignmentJournalHead(header.assignment).encode() {
                    let Some(bytes) = new else { return Err(corrupt("head deleted")) };
                    let NativeRowBody::AssignmentJournalHead(next) = decode(key, bytes)? else { return Err(corrupt("head kind")) };
                    if next.next_ordinal != header.ordinal + 1 {
                        continue;
                    }
                    if matched_head {
                        return Err(corrupt("repeated head target"));
                    }
                    let prior = match old {
                        None => None,
                        Some(bytes) => match decode(key, bytes)? {
                            NativeRowBody::AssignmentJournalHead(h) => Some(h),
                            _ => return Err(corrupt("old head kind")),
                        },
                    };
                    if frame.append_to(prior.as_ref(), next.retention_until_daa).map_err(|_| corrupt("stale/discontinuous head"))?
                        != next
                    {
                        return Err(corrupt("head differs from frame"));
                    }
                    if let Some(prior) = prior {
                        if let Some(pre) = views.iter().find(|v| v.header().id() == prior.last_frame) {
                            if pre.header().assignment != prior.assignment
                                || pre.header().ordinal + 1 != prior.next_ordinal
                                || pre.header().origin != prior.origin
                            {
                                return Err(corrupt("in-batch predecessor differs"));
                            }
                        } else {
                            self.with_positive_assignment_journal_record_v2(prior.last_frame, limits.max_record_bytes, |raw| {
                                let pre = AssignmentJournalRecordViewV2::decode(raw, limits.max_record_bytes)
                                    .map_err(|_| corrupt("stored predecessor malformed"))?;
                                if pre.header().id() != prior.last_frame
                                    || pre.header().assignment != prior.assignment
                                    || pre.header().ordinal + 1 != prior.next_ordinal
                                    || pre.header().origin != prior.origin
                                {
                                    return Err(corrupt("stored predecessor differs"));
                                }
                                Ok(())
                            })??;
                        }
                    }
                    matched_head = true;
                }
                if key == &NativeRowKey::Assignment(header.assignment).encode()
                    && let Some(bytes) = new
                {
                    let NativeRowBody::Assignment(a) = decode(key, bytes)? else { return Err(corrupt("assignment kind")) };
                    if a.last_event == header.origin {
                        matched_assignment = true;
                    }
                }
            }
            if !matched_frame || !matched_head || !matched_assignment {
                return Err(corrupt("raw frame missing atomic frame/head/assignment transition"));
            }
            if let Some(bytes) = self.db.get_pinned(key(id))?
                && bytes.as_ref() != records[i]
            {
                return Err(corrupt("content address has different bytes"));
            }
        }
        // Conversely, a new head/frame cannot escape literal retention.
        for entry in &delta.entries {
            let PalwDeltaEntryV2::KernelRouteRow { table, key, old: _, new: Some(bytes) } = entry else { continue };
            if *table != KERNEL_AUX_TABLE {
                continue;
            }
            match decode(key, bytes)? {
                NativeRowBody::AssignmentJournalFrame(f) if !views.iter().any(|v| v.frame() == f) => {
                    return Err(corrupt("frame has no literal record"));
                }
                NativeRowBody::AssignmentJournalHead(h) if !views.iter().any(|v| v.header().id() == h.last_frame) => {
                    return Err(corrupt("head has no literal record"));
                }
                _ => {}
            }
        }
        let quote = AssignmentJournalStoreQuoteV2 {
            records: records.len() as u64,
            raw_bytes,
            predecessor_read_bound: predecessor,
            work_bound: work,
            retained_bytes: retained,
            batch_added_bytes: added,
            total_batch_bytes: total_batch,
        };
        // No fallible operations after the first put.
        for (view, raw) in views.iter().zip(records) {
            batch.put(key(view.header().id()), raw);
        }
        debug_assert_eq!(batch.size_in_bytes() as u64, total_batch);
        Ok((quote, NativeStoreWriteTicketV1 { _lease: lease }))
    }
    /// Real pinned DB bytes, bounded before borrowing; never an active-head or
    /// accepted-lineage capability. Native recovery rechecks selected state.
    pub fn with_positive_assignment_journal_record_v2<T>(
        &self,
        id: Digest,
        max_bytes: u64,
        visit: impl FnOnce(&[u8]) -> T,
    ) -> StoreResult<T> {
        if max_bytes == 0 || max_bytes == u64::MAX {
            return Err(capacity("nonfinite read limit"));
        }
        let k = key(id);
        let bytes = self.db.get_pinned(k)?.ok_or_else(|| StoreError::KeyNotFound(DbKey::new(&k[..2], id.0)))?;
        bound(bytes.len() as u64, max_bytes)?;
        Ok(visit(&bytes))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentJournalReadLimitsV2 {
    pub max_frames: u64,
    pub max_record_bytes: u64,
    pub max_total_raw_bytes: u64,
    pub max_work: u64,
    pub max_heap_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentJournalReadReportV2 {
    pub frames: u64,
    pub raw_bytes: u64,
    pub hash_work: u64,
}
impl DbPalwStateV2Store {
    /// Visit actual persisted bytes selected by the supplied CURRENT rooted
    /// state, in event order. This is data-integrity/availability diagnostics;
    /// the private native session must additionally authenticate its state
    /// generation, accepted ancestry, full literal bodies/signatures, funds and
    /// reservations. This return type cannot become recovered-credit authority.
    pub fn visit_positive_assignment_journal_v2(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        assignment: misaka_palw_positive_contract::assignment::AssignmentId,
        limits: AssignmentJournalReadLimitsV2,
        mut visit: impl FnMut(AssignmentJournalHeaderV2, &[u8]) -> StoreResult<()>,
    ) -> StoreResult<AssignmentJournalReadReportV2> {
        if [limits.max_frames, limits.max_record_bytes, limits.max_total_raw_bytes, limits.max_work, limits.max_heap_bytes]
            .into_iter()
            .any(|n| n == 0 || n == u64::MAX)
        {
            return Err(capacity("read limits must be finite nonzero"));
        }
        let fixed_workspace =
            (8 * size_of::<NativeRowValue>() + 2 * size_of::<NativeRowKey>() + MAX_NATIVE_ROW_BYTES + MAX_NATIVE_KEY_BYTES) as u64;
        bound(fixed_workspace, limits.max_heap_bytes)?;
        let _lease = if native_writer_is_strict_v1() {
            reserve_native_requests_v1(limits.max_heap_bytes).map_err(|e| capacity(&e.to_string()))?
        } else {
            None
        };
        let rows = kaspa_consensus_core::palw_positive_rows_v2::PositiveRowsV2::new(state).assignment(assignment).map_err(
            |error| match error {
                kaspa_consensus_core::palw_positive_rows_v2::PositiveRowErrorV2::MissingKernelRoute
                | kaspa_consensus_core::palw_positive_rows_v2::PositiveRowErrorV2::MissingRequiredRow => {
                    capacity("current rooted assignment/head unavailable")
                }
                _ => corrupt("current rooted assignment/head invalid"),
            },
        )?;
        let head = rows.journal;
        bound(head.next_ordinal, limits.max_frames)?;
        // Two complete reads/hashes per frame: reverse validation then forward
        // delivery. Reserve before metadata allocation or the first DB read.
        let raw_bound = mul(mul(head.next_ordinal, limits.max_record_bytes)?, 2)?;
        bound(raw_bound, limits.max_total_raw_bytes)?;
        bound(mul(raw_bound, 2)?, limits.max_work)?;
        let index = mul(head.next_ordinal, size_of::<Digest>() as u64)?;
        bound(add(add(index, limits.max_record_bytes)?, fixed_workspace)?, limits.max_heap_bytes)?;
        let count = usize::try_from(head.next_ordinal).map_err(|_| capacity("frame count address space"))?;
        let mut ids = Vec::new();
        ids.try_reserve_exact(count).map_err(|_| capacity("bounded frame index"))?;
        if ids.capacity() > count {
            return Err(capacity("frame index capacity exceeds reservation"));
        }
        let mut expected = head.last_frame;
        let mut raw_bytes = 0;
        for ordinal in (0..head.next_ordinal).rev() {
            let (previous, origin, kind) =
                self.with_positive_assignment_journal_record_v2(expected, limits.max_record_bytes, |raw| {
                    raw_bytes = add(raw_bytes, raw.len() as u64)?;
                    let view = AssignmentJournalRecordViewV2::decode(raw, limits.max_record_bytes)
                        .map_err(|_| corrupt("persisted frame identity/codec"))?;
                    let h = view.header();
                    if h.id() != expected || h.assignment != assignment || h.ordinal != ordinal {
                        return Err(corrupt("persisted frame chain identity"));
                    }
                    Ok((h.previous, h.origin, h.kind))
                })??;
            if ordinal + 1 == head.next_ordinal && origin != head.origin {
                return Err(corrupt("current head origin differs"));
            }
            if ordinal == 0
                && (origin != rows.assignment.origin || kind != AssignmentJournalEventKindV2::Open || previous != Digest([0; 64]))
            {
                return Err(corrupt("initial accepted origin differs"));
            }
            ids.push(expected);
            expected = previous;
        }
        for id in ids.into_iter().rev() {
            self.with_positive_assignment_journal_record_v2(id, limits.max_record_bytes, |raw| {
                raw_bytes = add(raw_bytes, raw.len() as u64)?;
                let view = AssignmentJournalRecordViewV2::decode(raw, limits.max_record_bytes)
                    .map_err(|_| corrupt("persisted frame changed during read"))?;
                if view.header().id() != id {
                    return Err(corrupt("immutable frame identity changed"));
                }
                visit(view.header(), view.payload())
            })??;
        }
        Ok(AssignmentJournalReadReportV2 { frames: head.next_ordinal, raw_bytes, hash_work: mul(raw_bytes, 2)? })
    }
}
#[cfg(test)]
mod tests;
