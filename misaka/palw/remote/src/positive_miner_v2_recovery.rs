//! A2 process preparation journal. This is neither a native A6 journal nor a
//! computation certificate. It records irreversible local read/preparation cost
//! before effects, and preserves the canonical source-unit union across restart.
//!
//! Integrity hashes identify local records only. A trusted external anchor is
//! required on resume to detect replacement with an older complete journal.
//! Anchors bind the signed case/scope permanently in a protected LOCAL store.
//! Already spent I/O is not undone when native receipt/funding state reorgs.

use super::*;
use kaspa_consensus_core::palw_prompt_ids_v1::prompt_ids_tile_count_v1;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

const MAGIC: &[u8; 8] = b"PC2A2JR1";
const FORMAT_VERSION: u16 = 1;
const HEADER_BODY_BYTES: usize = 8 + 2 + 6 * 64 + 5 * 8;
const HEADER_BYTES: usize = HEADER_BODY_BYTES + 64;
const RECORD_PREFIX_BYTES: usize = 8 + 8 + 64;
const RECORD_OVERHEAD_BYTES: u64 = RECORD_PREFIX_BYTES as u64 + 64;
const STATE_FIXED_BYTES: u64 = 6 * 8 + 8;
const ENTRY_BYTES: u64 = 1 + 64 + 8 + 8;
const HASH_CONTEXT: &[u8] = b"misaka/a2/preparation-journal/v1";

type UnitKey = (SourceKind, Digest, u64);

/// Local record identity, not an admission/completion or payment authority.
/// Resume callers must obtain this from protected case state, not the journal
/// file they are about to verify, a peer, or an untrusted saved `Passed` flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JournalAnchorV2 {
    pub sequence: u64,
    pub hash: Digest,
}

#[derive(Clone, Copy, Debug)]
pub struct JournalLimitsV2 {
    pub max_bytes: u64,
    pub max_records: u64,
}

/// A process snapshot contains costs and prepared identities, never a Node
/// delivery flag, inference verdict, transferable signature or credit permit.
/// Fields stay private; replay must validate them against a fresh authenticated
/// responder and the signed service contract before installing them.
#[derive(Clone)]
pub struct PreparationCheckpointV2 {
    result: MinerResultCommit,
    contract: MinerServiceContract,
    limits: ProcessLimitsV2,
    meter: PreparationMeterV2,
    prepared: BTreeMap<UnitKey, u64>,
}
impl PreparationCheckpointV2 {
    pub fn capture<D>(responder: &MinerResponderV2<'_, D>) -> Self {
        Self {
            result: responder.result.clone(),
            contract: responder.contract,
            limits: responder.limits,
            meter: responder.meter,
            prepared: responder.prepared.clone(),
        }
    }

    fn restore<D>(&self, responder: &mut MinerResponderV2<'_, D>) -> Result<(), MinerErrorV2> {
        if responder.poisoned || responder.meter != PreparationMeterV2::default() || !responder.prepared.is_empty() {
            return Err(journal_error("recovery requires a fresh authenticated responder"));
        }
        let expected = Self::capture(responder);
        if self.header_body() != expected.header_body() {
            return Err(MinerErrorV2::Binding);
        }
        self.validate()?;
        responder.meter = self.meter;
        responder.prepared = self.prepared.clone();
        Ok(())
    }

    fn header_body(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_BODY_BYTES);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.result.id().0.0);
        bytes.extend_from_slice(&self.contract.id().0);
        bytes.extend_from_slice(&self.result.case_id().0.0);
        bytes.extend_from_slice(&self.result.scope.network.0);
        bytes.extend_from_slice(&self.result.scope.ruleset.0);
        bytes.extend_from_slice(&self.result.scope.branch.0);
        for limit in [
            self.limits.max_units_per_request,
            self.limits.max_tracked_units,
            self.limits.max_provider_reads,
            self.limits.max_prepared_bytes,
            self.limits.max_status_checks,
        ] {
            put_u64(&mut bytes, limit);
        }
        debug_assert_eq!(bytes.len(), HEADER_BODY_BYTES);
        bytes
    }

    fn validate(&self) -> Result<(), MinerErrorV2> {
        let signed = self.contract.budget;
        if self.contract.case != self.result.case_id() || self.contract.id() != self.result.service_contract {
            return Err(MinerErrorV2::Binding);
        }
        if self.prepared.len() as u64 > self.limits.max_tracked_units
            || self.meter.rounds > signed.rounds
            || self.meter.unique_source_bytes > signed.unique_source_bytes
            || self.meter.prepared_bytes > signed.transmitted_bytes.min(self.limits.max_prepared_bytes)
            || self.meter.provider_reads > self.limits.max_provider_reads
            || self.meter.status_checks > self.limits.max_status_checks
            || self.meter.additional_work > signed.additional_work
            || self.meter.additional_work < self.meter.provider_reads
            || self.meter.prepared_bytes < self.meter.unique_source_bytes
            || self.meter.provider_reads < self.prepared.len() as u64
            || self.meter.status_checks < self.prepared.len() as u64
        {
            return Err(MinerErrorV2::Budget);
        }
        let mut unique = 0u64;
        for ((kind, source, index), bytes) in &self.prepared {
            let in_scope = match kind {
                SourceKind::ArtifactInventory => *source == self.result.artifact && *index <= u32::MAX as u64,
                SourceKind::Input => {
                    *source == self.result.input
                        && *index < prompt_ids_tile_count_v1(u64::from(self.result.prefill)).unwrap_or(0).max(1)
                }
            };
            if !in_scope {
                return Err(MinerErrorV2::Binding);
            }
            unique = unique.checked_add(*bytes).ok_or(MinerErrorV2::Budget)?;
        }
        if unique != self.meter.unique_source_bytes {
            return Err(journal_error("prepared union does not match unique-byte meter"));
        }
        Ok(())
    }

    fn apply_delta(&mut self, payload: &[u8]) -> Result<(), MinerErrorV2> {
        let mut reader = payload;
        let values = [
            take_u64(&mut reader)?,
            take_u64(&mut reader)?,
            take_u64(&mut reader)?,
            take_u64(&mut reader)?,
            take_u64(&mut reader)?,
            take_u64(&mut reader)?,
        ];
        let meter = PreparationMeterV2 {
            rounds: values[0],
            unique_source_bytes: values[1],
            prepared_bytes: values[2],
            provider_reads: values[3],
            additional_work: values[4],
            status_checks: values[5],
        };
        let count = take_u64(&mut reader)?;
        if count > 1 || count.checked_mul(ENTRY_BYTES) != Some(reader.len() as u64) {
            return Err(MinerErrorV2::Budget);
        }
        let added = if count == 1 {
            let kind = match take_array::<1>(&mut reader)?[0] {
                0 => SourceKind::ArtifactInventory,
                1 => SourceKind::Input,
                _ => return Err(journal_error("unknown source kind in journal")),
            };
            let source = Digest(take_array::<64>(&mut reader)?);
            let index = take_u64(&mut reader)?;
            let bytes = take_u64(&mut reader)?;
            Some(((kind, source, index), bytes))
        } else {
            None
        };
        if !reader.is_empty() {
            return Err(journal_error("trailing journal payload"));
        }
        self.validate_increment(meter, added)?;
        if let Some((key, bytes)) = added {
            self.prepared.insert(key, bytes);
        }
        self.meter = meter;
        Ok(())
    }

    fn validate_increment(&self, meter: PreparationMeterV2, added: Option<(UnitKey, u64)>) -> Result<(), MinerErrorV2> {
        if !monotone(meter, self.meter) {
            return Err(journal_error("journal meter moved backwards"));
        }
        let mut expected_unique = self.meter.unique_source_bytes;
        let mut count = self.prepared.len() as u64;
        if let Some(((kind, source, index), bytes)) = added {
            if self.prepared.contains_key(&(kind, source, index)) {
                return Err(journal_error("journal repeated prepared unit"));
            }
            let valid = match kind {
                SourceKind::ArtifactInventory => source == self.result.artifact && index <= u32::MAX as u64,
                SourceKind::Input => {
                    source == self.result.input && index < prompt_ids_tile_count_v1(u64::from(self.result.prefill)).unwrap_or(0).max(1)
                }
            };
            if !valid {
                return Err(MinerErrorV2::Binding);
            }
            expected_unique = expected_unique.checked_add(bytes).ok_or(MinerErrorV2::Budget)?;
            count = count.checked_add(1).ok_or(MinerErrorV2::Budget)?;
        }
        let signed = self.contract.budget;
        if expected_unique != meter.unique_source_bytes {
            return Err(journal_error("journal unique bytes changed without matching unit"));
        }
        if count > self.limits.max_tracked_units
            || meter.rounds > signed.rounds
            || meter.unique_source_bytes > signed.unique_source_bytes
            || meter.prepared_bytes > signed.transmitted_bytes.min(self.limits.max_prepared_bytes)
            || meter.provider_reads > self.limits.max_provider_reads
            || meter.status_checks > self.limits.max_status_checks
            || meter.additional_work > signed.additional_work
            || meter.additional_work < meter.provider_reads
            || meter.prepared_bytes < meter.unique_source_bytes
            || meter.provider_reads < count
            || meter.status_checks < count
        {
            return Err(MinerErrorV2::Budget);
        }
        Ok(())
    }
}

/// Must durably persist each checkpoint before the associated provider/status
/// read or outward response preparation. Errors poison the owning responder.
pub trait PreparationJournalV2 {
    fn persist<D>(&mut self, responder: &MinerResponderV2<'_, D>, added: Option<(UnitKey, u64)>) -> Result<(), MinerErrorV2>;
}

/// Explicitly non-durable compatibility path for an ephemeral process. It must
/// not be presented as restart protection or substituted for native A6 state.
pub struct EphemeralPreparationJournalV2;
impl PreparationJournalV2 for EphemeralPreparationJournalV2 {
    fn persist<D>(&mut self, _: &MinerResponderV2<'_, D>, _: Option<(UnitKey, u64)>) -> Result<(), MinerErrorV2> {
        Ok(())
    }
}

/// A6/application-owned protected per-case anchor. Implementations must perform
/// a durable compare-and-commit in their protected case namespace; a peer field,
/// sidecar loaded from this same untrusted journal, or unconditional `Ok` is not
/// an implementation. There is intentionally no production default here.
///
/// `None` is accepted only for a genuinely new case. Every successful persist
/// commits this anchor AFTER journal fsync and BEFORE the associated effect.
/// All advances for the case must be made by its lease-holding journal writer;
/// the protected owner must not concurrently advance/reassign this namespace.
/// The lease must exclude a SECOND journal path for the same case as well:
/// serialized/idempotent CAS alone permits two identical writers to repeat I/O.
pub trait ProtectedAnchorStoreV2 {
    fn load(&self) -> Result<Option<JournalAnchorV2>, MinerErrorV2>;
    fn commit(&mut self, expected: Option<JournalAnchorV2>, next: JournalAnchorV2) -> Result<(), MinerErrorV2>;
}

/// Append-only local journal with an exclusive crash-released file lease.
/// The single fd is private; uncertain writes/sync poison it until bounded replay.
pub struct FilePreparationJournalV2<A> {
    session: Arc<()>,
    anchors: A,
    committed_anchor: JournalAnchorV2,
    file: File,
    state: PreparationCheckpointV2,
    anchor: JournalAnchorV2,
    limits: JournalLimitsV2,
    bytes: u64,
    poisoned: bool,
}
impl<A: ProtectedAnchorStoreV2> FilePreparationJournalV2<A> {
    pub fn create<D>(
        path: &Path,
        responder: &MinerResponderV2<'_, D>,
        limits: JournalLimitsV2,
        mut anchors: A,
    ) -> Result<Self, MinerErrorV2> {
        check_journal_limits(limits)?;
        if anchors.load()?.is_some() {
            return Err(journal_error("protected case anchor already exists"));
        }
        let state = PreparationCheckpointV2::capture(responder);
        state.validate()?;
        if responder.poisoned || state.meter != PreparationMeterV2::default() || !state.prepared.is_empty() {
            return Err(journal_error("create requires a fresh responder"));
        }
        let mut options = secure_options();
        let mut file = options.create_new(true).open(path).map_err(io_error)?;
        require_regular(&file)?;
        lock_exclusive(&file)?;
        let mut header = state.header_body();
        let hash = digest(&header);
        header.extend_from_slice(&hash.0);
        file.write_all(&header).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        sync_parent(path)?;
        let anchor = JournalAnchorV2 { sequence: 0, hash };
        anchors.commit(None, anchor)?;
        Ok(Self {
            session: Arc::clone(&responder.session),
            anchors,
            committed_anchor: anchor,
            file,
            state,
            anchor: JournalAnchorV2 { sequence: 0, hash },
            limits,
            bytes: HEADER_BYTES as u64,
            poisoned: false,
        })
    }

    /// Verify the exact protected prefix and restore the supplied fresh session
    /// BEFORE publishing a usable journal. Binding a fresh session without its
    /// saved meter would merge equal Complete-only status counters on restart.
    /// Checksum-valid unanchored suffixes cannot discount future work.
    pub fn open<D>(
        path: &Path,
        responder: &mut MinerResponderV2<'_, D>,
        limits: JournalLimitsV2,
        anchors: A,
    ) -> Result<Self, MinerErrorV2> {
        let journal = Self::open_prefix(path, responder, limits, anchors, false)?;
        journal.state.restore(responder)?;
        Ok(journal)
    }

    /// Explicit crash-window repair. Discard bytes AFTER the exact protected
    /// prefix, durably, without interpreting them or changing the anchor. A
    /// missing/corrupt protected prefix is never repaired from untrusted data.
    pub fn open_discarding_unanchored_tail<D>(
        path: &Path,
        responder: &mut MinerResponderV2<'_, D>,
        limits: JournalLimitsV2,
        anchors: A,
    ) -> Result<Self, MinerErrorV2> {
        let journal = Self::open_prefix(path, responder, limits, anchors, true)?;
        journal.state.restore(responder)?;
        Ok(journal)
    }

    fn open_prefix<D>(
        path: &Path,
        responder: &MinerResponderV2<'_, D>,
        limits: JournalLimitsV2,
        anchors: A,
        discard_tail: bool,
    ) -> Result<Self, MinerErrorV2> {
        check_journal_limits(limits)?;
        let mut file = secure_options().open(path).map_err(io_error)?;
        require_regular(&file)?;
        lock_exclusive(&file)?;
        // A previous writer may commit and release its lease while this open
        // is starting. Load the protected boundary only AFTER owning the lease.
        let trusted_anchor = anchors.load()?.ok_or_else(|| journal_error("protected case anchor is missing"))?;
        if trusted_anchor.sequence > limits.max_records {
            return Err(MinerErrorV2::Budget);
        }
        let bytes = file.metadata().map_err(io_error)?.len();
        if bytes < HEADER_BYTES as u64 || (!discard_tail && bytes > limits.max_bytes) {
            return Err(MinerErrorV2::Budget);
        }
        let mut header = [0u8; HEADER_BYTES];
        file.read_exact(&mut header).map_err(io_error)?;
        let mut state = PreparationCheckpointV2::capture(responder);
        if responder.poisoned || state.meter != PreparationMeterV2::default() || !state.prepared.is_empty() {
            return Err(journal_error("open requires a fresh responder"));
        }
        state.validate()?;
        let expected = state.header_body();
        let head_hash = digest(&expected);
        if header[..HEADER_BODY_BYTES] != expected[..] || header[HEADER_BODY_BYTES..] != head_hash.0 {
            return Err(MinerErrorV2::Binding);
        }
        let mut anchor = JournalAnchorV2 { sequence: 0, hash: head_hash };
        let mut consumed = HEADER_BYTES as u64;
        while anchor.sequence < trusted_anchor.sequence {
            if consumed > bytes || bytes - consumed < RECORD_OVERHEAD_BYTES {
                return Err(MinerErrorV2::Budget);
            }
            let mut prefix = [0u8; RECORD_PREFIX_BYTES];
            file.read_exact(&mut prefix).map_err(io_error)?;
            let sequence = u64::from_le_bytes(prefix[..8].try_into().expect("8"));
            let payload_len = u64::from_le_bytes(prefix[8..16].try_into().expect("8"));
            let previous = Digest(prefix[16..].try_into().expect("64"));
            let max_payload = STATE_FIXED_BYTES + ENTRY_BYTES;
            let record_len = payload_len.checked_add(RECORD_OVERHEAD_BYTES).ok_or(MinerErrorV2::Budget)?;
            if sequence != anchor.sequence.checked_add(1).ok_or(MinerErrorV2::Budget)?
                || previous != anchor.hash
                || payload_len < STATE_FIXED_BYTES
                || payload_len > max_payload
                || record_len > bytes - consumed
                || consumed.checked_add(record_len).filter(|n| *n <= limits.max_bytes).is_none()
            {
                return Err(journal_error("truncated, reordered or oversized journal record"));
            }
            let len = usize::try_from(payload_len).map_err(|_| MinerErrorV2::Budget)?;
            let mut payload = vec![0u8; len];
            file.read_exact(&mut payload).map_err(io_error)?;
            let mut hash = [0u8; 64];
            file.read_exact(&mut hash).map_err(io_error)?;
            let expected = record_digest(&prefix, &payload);
            if hash != expected.0 {
                return Err(journal_error("journal record checksum mismatch"));
            }
            state.apply_delta(&payload)?;
            anchor = JournalAnchorV2 { sequence, hash: expected };
            consumed = consumed.checked_add(record_len).ok_or(MinerErrorV2::Budget)?;
        }
        if anchor != trusted_anchor {
            return Err(journal_error("journal is older than or different from trusted anchor"));
        }
        state.validate()?;
        if consumed != bytes {
            if !discard_tail {
                return Err(journal_error("unanchored journal tail requires explicit discard"));
            }
            // This namespace is advanced only by the lease-holding writer.
            // Recheck before destructive repair; never promote a tail's hash.
            if anchors.load()? != Some(trusted_anchor) {
                return Err(journal_error("protected anchor changed during repair"));
            }
            file.set_len(consumed).map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
        }
        file.seek(SeekFrom::End(0)).map_err(io_error)?;
        Ok(Self {
            session: Arc::clone(&responder.session),
            anchors,
            committed_anchor: anchor,
            file,
            state,
            anchor,
            limits,
            bytes: consumed,
            poisoned: false,
        })
    }

    pub fn anchor(&self) -> JournalAnchorV2 {
        self.committed_anchor
    }

    fn append<D>(&mut self, responder: &MinerResponderV2<'_, D>, added: Option<(UnitKey, u64)>) -> Result<(), MinerErrorV2> {
        if self.poisoned {
            return Err(journal_error("journal is poisoned after uncertain I/O"));
        }
        if !Arc::ptr_eq(&self.session, &responder.session) {
            return Err(journal_error("journal belongs to a different live responder session"));
        }
        // Only fixed-size identity/limit fields and the explicitly changed unit
        // are inspected here. No full union clone or scan occurs per operation.
        if responder.result.id() != self.state.result.id()
            || responder.contract.id() != self.state.contract.id()
            || process_limits(responder.limits) != process_limits(self.state.limits)
            || responder.prepared.len() as u64
                != (self.state.prepared.len() as u64).checked_add(u64::from(added.is_some())).ok_or(MinerErrorV2::Budget)?
        {
            return Err(MinerErrorV2::Binding);
        }
        if let Some((key, bytes)) = added {
            if responder.prepared.get(&key) != Some(&bytes) {
                return Err(MinerErrorV2::Binding);
            }
        }
        self.state.validate_increment(responder.meter, added)?;
        let mut payload = Vec::with_capacity((STATE_FIXED_BYTES + ENTRY_BYTES) as usize);
        for value in meter_values(responder.meter) {
            put_u64(&mut payload, value);
        }
        put_u64(&mut payload, u64::from(added.is_some()));
        if let Some(((kind, source, index), bytes)) = added {
            payload.push(match kind {
                SourceKind::ArtifactInventory => 0,
                SourceKind::Input => 1,
            });
            payload.extend_from_slice(&source.0);
            put_u64(&mut payload, index);
            put_u64(&mut payload, bytes);
        }
        let sequence = self.anchor.sequence.checked_add(1).filter(|n| *n <= self.limits.max_records).ok_or(MinerErrorV2::Budget)?;
        let record_len = (payload.len() as u64).checked_add(RECORD_OVERHEAD_BYTES).ok_or(MinerErrorV2::Budget)?;
        let next_bytes = self.bytes.checked_add(record_len).filter(|n| *n <= self.limits.max_bytes).ok_or(MinerErrorV2::Budget)?;
        if self.file.metadata().map_err(io_error)?.len() != self.bytes {
            return Err(journal_error("journal changed outside its writer"));
        }
        let mut prefix = Vec::with_capacity(RECORD_PREFIX_BYTES);
        put_u64(&mut prefix, sequence);
        put_u64(&mut prefix, payload.len() as u64);
        prefix.extend_from_slice(&self.anchor.hash.0);
        let hash = record_digest(&prefix, &payload);
        let write = self
            .file
            .write_all(&prefix)
            .and_then(|_| self.file.write_all(&payload))
            .and_then(|_| self.file.write_all(&hash.0))
            .and_then(|_| self.file.sync_all());
        if let Err(error) = write {
            self.poisoned = true;
            return Err(io_error(error));
        }
        if let Some((key, bytes)) = added {
            self.state.prepared.insert(key, bytes);
        }
        self.state.meter = responder.meter;
        self.anchor = JournalAnchorV2 { sequence, hash };
        self.bytes = next_bytes;
        self.anchors.commit(Some(self.committed_anchor), self.anchor)?;
        self.committed_anchor = self.anchor;
        Ok(())
    }
}
impl<A: ProtectedAnchorStoreV2> PreparationJournalV2 for FilePreparationJournalV2<A> {
    fn persist<D>(&mut self, responder: &MinerResponderV2<'_, D>, added: Option<(UnitKey, u64)>) -> Result<(), MinerErrorV2> {
        let result = self.append(responder, added);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
}

/// Production local-durability entry point: its source port always performs
/// write-ahead recording. The underlying raw responder/journal are not exposed.
/// This protects process cost accounting only; A4/A6 still own canonical duties,
/// accepted data, branch rollback, money, and the protected external anchor.
pub struct DurableMinerResponderV2<'p, D, A> {
    inner: MinerResponderV2<'p, D>,
    journal: FilePreparationJournalV2<A>,
}
impl<'p, D: CanonicalDataDutyV2, A: ProtectedAnchorStoreV2> DurableMinerResponderV2<'p, D, A> {
    pub fn create(path: &Path, inner: MinerResponderV2<'p, D>, limits: JournalLimitsV2, anchors: A) -> Result<Self, MinerErrorV2> {
        let journal = FilePreparationJournalV2::create(path, &inner, limits, anchors)?;
        Ok(Self { inner, journal })
    }
    pub fn resume(path: &Path, mut fresh: MinerResponderV2<'p, D>, limits: JournalLimitsV2, anchors: A) -> Result<Self, MinerErrorV2> {
        let journal = FilePreparationJournalV2::open(path, &mut fresh, limits, anchors)?;
        Ok(Self { inner: fresh, journal })
    }
    /// Explicit recovery of the fsync-before-protected-CAS crash window.
    /// Discarded preparation had no permitted effect: persist must protect its
    /// anchor before returning to the provider/status/response path.
    pub fn resume_discarding_unanchored_tail(
        path: &Path,
        mut fresh: MinerResponderV2<'p, D>,
        limits: JournalLimitsV2,
        anchors: A,
    ) -> Result<Self, MinerErrorV2> {
        let journal = FilePreparationJournalV2::open_discarding_unanchored_tail(path, &mut fresh, limits, anchors)?;
        Ok(Self { inner: fresh, journal })
    }
    pub fn meter(&self) -> PreparationMeterV2 {
        self.inner.meter()
    }
    pub fn anchor(&self) -> JournalAnchorV2 {
        self.journal.anchor()
    }
}
impl<D: CanonicalDataDutyV2, A: ProtectedAnchorStoreV2> MinerSource for DurableMinerResponderV2<'_, D, A> {
    type AuthenticatedDuty = D;
    type G14Response = PreparedDataV2;
    type Error = MinerErrorV2;
    fn respond(&mut self, duty: &D) -> Result<PreparedDataV2, MinerErrorV2> {
        self.inner.respond_recorded(duty, &mut self.journal)
    }
}

fn check_journal_limits(limits: JournalLimitsV2) -> Result<(), MinerErrorV2> {
    if limits.max_records == 0 || limits.max_bytes < HEADER_BYTES as u64 {
        return Err(MinerErrorV2::Budget);
    }
    Ok(())
}
fn meter_values(meter: PreparationMeterV2) -> [u64; 6] {
    [meter.rounds, meter.unique_source_bytes, meter.prepared_bytes, meter.provider_reads, meter.additional_work, meter.status_checks]
}
fn monotone(next: PreparationMeterV2, previous: PreparationMeterV2) -> bool {
    meter_values(next).into_iter().zip(meter_values(previous)).all(|(a, b)| a >= b)
}
fn put_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
fn take_u64(reader: &mut &[u8]) -> Result<u64, MinerErrorV2> {
    Ok(u64::from_le_bytes(take_array::<8>(reader)?))
}
fn take_array<const N: usize>(reader: &mut &[u8]) -> Result<[u8; N], MinerErrorV2> {
    if reader.len() < N {
        return Err(journal_error("truncated journal payload"));
    }
    let (head, rest) = reader.split_at(N);
    *reader = rest;
    Ok(head.try_into().expect("N"))
}
fn digest(bytes: &[u8]) -> Digest {
    Digest(blake2b_simd::Params::new().hash_length(64).key(HASH_CONTEXT).hash(bytes).as_bytes().try_into().expect("64"))
}
fn record_digest(prefix: &[u8], payload: &[u8]) -> Digest {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(HASH_CONTEXT).to_state();
    state.update(prefix);
    state.update(payload);
    Digest(state.finalize().as_bytes().try_into().expect("64"))
}
fn journal_error(message: &str) -> MinerErrorV2 {
    MinerErrorV2::Journal(message.into())
}
fn io_error(error: std::io::Error) -> MinerErrorV2 {
    MinerErrorV2::Journal(error.to_string())
}
fn secure_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    options
}
fn require_regular(file: &File) -> Result<(), MinerErrorV2> {
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err(journal_error("journal is not a regular file"));
    }
    Ok(())
}
fn process_limits(limits: ProcessLimitsV2) -> [u64; 5] {
    [
        limits.max_units_per_request,
        limits.max_tracked_units,
        limits.max_provider_reads,
        limits.max_prepared_bytes,
        limits.max_status_checks,
    ]
}
fn lock_exclusive(file: &File) -> Result<(), MinerErrorV2> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // The fd is owned for this journal's lifetime. A crash releases the
        // kernel lease; a second live writer must never restore stale counters.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Err(journal_error("durable preparation journals require a supported exclusive kernel lease"))
    }
}
fn sync_parent(path: &Path) -> Result<(), MinerErrorV2> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    File::open(parent).and_then(|file| file.sync_all()).map_err(io_error)
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::tests::{Duty, Fixture, Provider};
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        ffi::CString,
        os::unix::{ffi::OsStrExt, fs::symlink},
        path::PathBuf,
        rc::Rc,
        sync::atomic::{AtomicU64, Ordering},
    };

    // A protected owner fixture, deliberately not a production/native A6 store.
    #[derive(Clone, Default)]
    struct MemoryAnchor(Rc<RefCell<AnchorState>>);
    #[derive(Default)]
    struct AnchorState {
        current: Option<JournalAnchorV2>,
        fail_at: Option<u64>,
    }
    impl ProtectedAnchorStoreV2 for MemoryAnchor {
        fn load(&self) -> Result<Option<JournalAnchorV2>, MinerErrorV2> {
            Ok(self.0.borrow().current)
        }
        fn commit(&mut self, expected: Option<JournalAnchorV2>, next: JournalAnchorV2) -> Result<(), MinerErrorV2> {
            let mut state = self.0.borrow_mut();
            if state.current != expected {
                return Err(journal_error("test anchor compare-and-commit failed"));
            }
            if state.fail_at == Some(next.sequence) {
                return Err(journal_error("injected protected anchor I/O failure"));
            }
            state.current = Some(next);
            Ok(())
        }
    }
    struct TempCase(PathBuf);
    impl TempCase {
        fn new() -> Self {
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);
            let suffix = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("misaka-a2-journal-{}-{now}-{suffix}", std::process::id()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("prepared.journal")
        }
    }
    impl Drop for TempCase {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn limits() -> ProcessLimitsV2 {
        ProcessLimitsV2 {
            max_units_per_request: 8,
            max_tracked_units: 16,
            max_provider_reads: 32,
            max_prepared_bytes: 1 << 20,
            max_status_checks: 1024,
        }
    }
    fn journal_limits() -> JournalLimitsV2 {
        JournalLimitsV2 { max_bytes: 1 << 20, max_records: 1024 }
    }

    #[test]
    fn restart_preserves_union_retries_and_all_preparation_counters() {
        let f = Fixture::new();
        let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
        let case = TempCase::new();
        let anchors = MemoryAnchor::default();
        let duty = f.duty();
        let mut live =
            DurableMinerResponderV2::create(&case.path(), f.responder(&providers, limits()), journal_limits(), anchors.clone())
                .unwrap();
        let first = live.respond(&duty).unwrap();
        let spent = live.meter();
        let anchor = live.anchor();
        assert_eq!(anchors.load().unwrap(), Some(anchor));
        assert_eq!(first.units.len(), 1);
        assert_eq!(spent.unique_source_bytes, 16);
        drop(live);
        let mut recovered =
            DurableMinerResponderV2::resume(&case.path(), f.responder(&providers, limits()), journal_limits(), anchors.clone())
                .unwrap();
        assert_eq!(recovered.meter(), spent);
        assert_eq!(recovered.anchor(), anchor);
        let repeated = recovered.respond(&duty).unwrap();
        assert_eq!(first, repeated);
        assert_eq!(recovered.meter().unique_source_bytes, spent.unique_source_bytes);
        assert_eq!(recovered.meter().rounds, spent.rounds);
        assert_eq!(recovered.meter().provider_reads, spent.provider_reads + 1);
        assert_eq!(recovered.meter().prepared_bytes, spent.prepared_bytes * 2);
        assert_eq!(recovered.meter().additional_work, spent.additional_work * 2);
        assert_eq!(recovered.meter().status_checks, spent.status_checks + 1);
    }

    struct MissingProvider(Cell<u64>);
    impl StoredProviderV2 for MissingProvider {
        fn unit(&self, _: SourceKind, _: Digest, _: u64) -> Result<&StoredUnitV2, String> {
            self.0.set(self.0.get() + 1);
            Err("missing stored unit".into())
        }
    }
    #[test]
    fn unavailable_provider_cost_is_durable_and_read_cap_does_not_reset() {
        let f = Fixture::new();
        let missing = MissingProvider(Cell::new(0));
        let providers: [&dyn StoredProviderV2; 1] = [&missing];
        let case = TempCase::new();
        let anchors = MemoryAnchor::default();
        let duty = f.duty();
        let mut local = limits();
        local.max_provider_reads = 1;
        let mut live =
            DurableMinerResponderV2::create(&case.path(), f.responder(&providers, local), journal_limits(), anchors.clone()).unwrap();
        assert_eq!(live.respond(&duty), Err(MinerErrorV2::Unavailable));
        let spent = live.meter();
        drop(live);
        assert_eq!(missing.0.get(), 1);
        let mut recovered =
            DurableMinerResponderV2::resume(&case.path(), f.responder(&providers, local), journal_limits(), anchors).unwrap();
        assert_eq!(recovered.meter(), spent);
        assert_eq!(recovered.respond(&duty), Err(MinerErrorV2::Budget));
        assert_eq!(missing.0.get(), 1);
    }

    struct InspectedProvider<'a> {
        source: &'a Provider,
        anchor: MemoryAnchor,
    }
    impl StoredProviderV2 for InspectedProvider<'_> {
        fn unit(&self, kind: SourceKind, source: Digest, index: u64) -> Result<&StoredUnitV2, String> {
            assert!(self.anchor.load().unwrap().unwrap().sequence >= 3, "read was not protected before provider I/O");
            self.source.unit(kind, source, index)
        }
    }
    struct InspectedDuty {
        source: Duty,
        anchor: MemoryAnchor,
    }
    impl CanonicalDataDutyV2 for InspectedDuty {
        fn request(&self) -> &SourceRequest {
            self.source.request()
        }
        fn authorize(&self, result: &MinerResultCommit) -> Result<DutyBoundsV2, MinerErrorV2> {
            self.source.authorize(result)
        }
        fn status(&self, index: u64) -> Result<MinerDataStatus, MinerErrorV2> {
            assert!(self.anchor.load().unwrap().unwrap().sequence >= 1, "control read was not protected before I/O");
            self.source.status(index)
        }
    }
    #[test]
    fn protected_anchor_is_committed_before_control_provider_and_response_effects() {
        let f = Fixture::new();
        let anchors = MemoryAnchor::default();
        let case = TempCase::new();
        let source = InspectedProvider { source: &f.provider, anchor: anchors.clone() };
        let providers: [&dyn StoredProviderV2; 1] = [&source];
        let inner = MinerResponderV2::<InspectedDuty>::new(
            &f.admission,
            f.result.clone(),
            f.contract,
            1,
            &f.key,
            &f.signature,
            &providers,
            limits(),
        )
        .unwrap();
        let mut live = DurableMinerResponderV2::create(&case.path(), inner, journal_limits(), anchors.clone()).unwrap();
        let duty = InspectedDuty { source: f.duty(), anchor: anchors.clone() };
        assert_eq!(live.respond(&duty).unwrap().units.len(), 1);
        assert_eq!(anchors.load().unwrap(), Some(live.anchor()));
        assert_eq!(live.anchor().sequence, 4);
    }

    #[test]
    fn failed_protected_anchor_commit_requires_explicit_discard_and_preserves_protected_prefix() {
        let f = Fixture::new();
        let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
        let case = TempCase::new();
        let anchors = MemoryAnchor::default();
        let duty = f.duty();
        anchors.0.borrow_mut().fail_at = Some(3); // after durable read reservation, before actual provider I/O
        let mut live =
            DurableMinerResponderV2::create(&case.path(), f.responder(&providers, limits()), journal_limits(), anchors.clone())
                .unwrap();
        assert!(matches!(live.respond(&duty), Err(MinerErrorV2::Journal(_))));
        assert_eq!(f.provider.calls.get(), 0);
        assert_eq!(live.respond(&duty), Err(MinerErrorV2::Closed));
        assert_eq!(anchors.load().unwrap().unwrap().sequence, 2);
        drop(live);
        anchors.0.borrow_mut().fail_at = None;
        assert!(
            DurableMinerResponderV2::resume(&case.path(), f.responder(&providers, limits()), journal_limits(), anchors.clone())
                .is_err()
        );
        let mut recovered = DurableMinerResponderV2::resume_discarding_unanchored_tail(
            &case.path(),
            f.responder(&providers, limits()),
            journal_limits(),
            anchors.clone(),
        )
        .unwrap();
        // The failed CAS preceded provider I/O. Keep protected seq2, discard
        // the unprotected reservation, and charge the actual retry normally.
        assert_eq!(recovered.meter().provider_reads, 0);
        assert_eq!(recovered.meter().status_checks, 1);
        assert_eq!(anchors.load().unwrap().unwrap().sequence, 2);
        recovered.respond(&duty).unwrap();
        assert_eq!(recovered.meter().provider_reads, 1);
    }

    #[test]
    fn bounded_record_capacity_stops_before_provider_and_poison_live_session() {
        let f = Fixture::new();
        let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
        let case = TempCase::new();
        let anchors = MemoryAnchor::default();
        let duty = f.duty();
        let capped = JournalLimitsV2 { max_records: 1, ..journal_limits() };
        let mut live = DurableMinerResponderV2::create(&case.path(), f.responder(&providers, limits()), capped, anchors).unwrap();
        assert_eq!(live.respond(&duty), Err(MinerErrorV2::Budget));
        assert_eq!(f.provider.calls.get(), 0);
        assert_eq!(live.respond(&duty), Err(MinerErrorV2::Closed));
    }

    #[test]
    fn missing_anchor_wrong_scope_and_complete_rollback_are_not_recovery() {
        let f = Fixture::new();
        let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
        let case = TempCase::new();
        let anchors = MemoryAnchor::default();
        let mut live =
            DurableMinerResponderV2::create(&case.path(), f.responder(&providers, limits()), journal_limits(), anchors.clone())
                .unwrap();
        let header_only = std::fs::read(case.path()).unwrap();
        live.respond(&f.duty()).unwrap();
        drop(live);
        assert!(
            DurableMinerResponderV2::resume(
                &case.path(),
                f.responder(&providers, limits()),
                journal_limits(),
                MemoryAnchor::default()
            )
            .is_err()
        );
        let mut changed = f.responder(&providers, limits());
        changed.result.output = Digest([99; 64]);
        assert!(DurableMinerResponderV2::resume(&case.path(), changed, journal_limits(), anchors.clone()).is_err());
        std::fs::write(case.path(), header_only).unwrap();
        assert!(DurableMinerResponderV2::resume(&case.path(), f.responder(&providers, limits()), journal_limits(), anchors).is_err());
    }

    #[test]
    fn torn_corrupt_oversized_and_reordered_record_bodies_are_refused() {
        let f = Fixture::new();
        let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
        let case = TempCase::new();
        let anchors = MemoryAnchor::default();
        let mut live =
            DurableMinerResponderV2::create(&case.path(), f.responder(&providers, limits()), journal_limits(), anchors.clone())
                .unwrap();
        live.respond(&f.duty()).unwrap();
        drop(live);
        let original = std::fs::read(case.path()).unwrap();
        let mut corrupted = original.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 1;
        let truncated = original[..original.len() - 1].to_vec();
        let mut oversized = original.clone();
        oversized[HEADER_BYTES + 8..HEADER_BYTES + 16].copy_from_slice(&u64::MAX.to_le_bytes());
        let mut reordered = original.clone();
        reordered[HEADER_BYTES..HEADER_BYTES + 8].copy_from_slice(&2u64.to_le_bytes());
        for bad in [corrupted, truncated, oversized, reordered] {
            std::fs::write(case.path(), bad).unwrap();
            assert!(
                DurableMinerResponderV2::resume(&case.path(), f.responder(&providers, limits()), journal_limits(), anchors.clone())
                    .is_err()
            );
        }
    }

    #[test]
    fn second_writer_nonregular_and_symlink_journals_are_refused_without_blocking() {
        let f = Fixture::new();
        let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
        let case = TempCase::new();
        let anchors = MemoryAnchor::default();
        let live = DurableMinerResponderV2::create(&case.path(), f.responder(&providers, limits()), journal_limits(), anchors.clone())
            .unwrap();
        assert!(
            FilePreparationJournalV2::open(&case.path(), &mut f.responder(&providers, limits()), journal_limits(), anchors.clone())
                .is_err()
        );
        let alias = case.0.join("alias");
        symlink(case.path(), &alias).unwrap();
        assert!(
            FilePreparationJournalV2::open(&alias, &mut f.responder(&providers, limits()), journal_limits(), anchors.clone()).is_err()
        );
        let fifo = case.0.join("fifo");
        let name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(FilePreparationJournalV2::open(&fifo, &mut f.responder(&providers, limits()), journal_limits(), anchors).is_err());
        drop(live);
    }

    // A test-only external owner persisted separately from the journal file.
    // This models the owner port across processes; it is not native A6 storage.
    struct TestFileAnchor(PathBuf);
    impl ProtectedAnchorStoreV2 for TestFileAnchor {
        fn load(&self) -> Result<Option<JournalAnchorV2>, MinerErrorV2> {
            let data = match std::fs::read(&self.0) {
                Ok(data) => data,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(io_error(e)),
            };
            if data.len() != 72 {
                return Err(journal_error("bad test protected anchor"));
            }
            Ok(Some(JournalAnchorV2 {
                sequence: u64::from_le_bytes(data[..8].try_into().unwrap()),
                hash: Digest(data[8..].try_into().unwrap()),
            }))
        }
        fn commit(&mut self, expected: Option<JournalAnchorV2>, next: JournalAnchorV2) -> Result<(), MinerErrorV2> {
            if self.load()? != expected {
                return Err(journal_error("test external owner CAS failed"));
            }
            let mut bytes = next.sequence.to_le_bytes().to_vec();
            bytes.extend_from_slice(&next.hash.0);
            let temporary = self.0.with_extension("next");
            let mut file = OpenOptions::new().create(true).truncate(true).write(true).open(&temporary).map_err(io_error)?;
            file.write_all(&bytes).map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
            std::fs::rename(&temporary, &self.0).map_err(io_error)?;
            sync_parent(&self.0)
        }
    }
    const CHILD: &str = "miner::positive_v2::recovery::tests::journal_process_child";
    fn child(root: &Path, phase: &str) -> std::process::Command {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.arg("--exact").arg(CHILD).arg("--nocapture").env("MISAKA_A2_JOURNAL_CHILD", phase).env("MISAKA_A2_JOURNAL_ROOT", root);
        command
    }
    #[test]
    fn journal_process_child() {
        let Ok(phase) = std::env::var("MISAKA_A2_JOURNAL_CHILD") else { return };
        let root = PathBuf::from(std::env::var_os("MISAKA_A2_JOURNAL_ROOT").unwrap());
        let path = root.join("prepared.journal");
        let anchor_path = root.join("protected.anchor");
        let f = Fixture::new();
        // A real file-backed source is reopened in each child process. File
        // preloading is explicit store work, separate from canonical delivery.
        use super::super::store::{FileStoredProviderV2, StoreLimitsV2, StoredEncodingV2, source_file_name_v2};
        let source_path = root.join(source_file_name_v2(f.result.artifact, 0, StoredEncodingV2::Artifact));
        if phase == "first" {
            let StoredUnitV2::Artifact(opening) = f.provider.unit(SourceKind::ArtifactInventory, f.result.artifact, 0).unwrap() else {
                unreachable!()
            };
            let bytes = borsh::to_vec(opening).unwrap();
            let mut file = OpenOptions::new().create_new(true).write(true).open(&source_path).unwrap();
            file.write_all(&bytes).unwrap();
            file.sync_all().unwrap();
            sync_parent(&source_path).unwrap();
        }
        let mut provider = FileStoredProviderV2::open(
            &root,
            StoreLimitsV2 {
                max_unit_bytes: 4096,
                max_cached_bytes: 8192,
                max_cached_units: 2,
                max_file_reads: 2,
                max_reserved_read_bytes: 8192,
            },
        )
        .unwrap();
        provider.load(f.result.artifact, 0, StoredEncodingV2::Artifact).unwrap();
        let providers: [&dyn StoredProviderV2; 1] = [&provider];
        match phase.as_str() {
            "first" => {
                let mut live = DurableMinerResponderV2::create(
                    &path,
                    f.responder(&providers, limits()),
                    journal_limits(),
                    TestFileAnchor(anchor_path),
                )
                .unwrap();
                live.respond(&f.duty()).unwrap();
                assert_eq!(live.meter().provider_reads, 1);
            }
            "resume" => {
                let mut live = DurableMinerResponderV2::resume(
                    &path,
                    f.responder(&providers, limits()),
                    journal_limits(),
                    TestFileAnchor(anchor_path),
                )
                .unwrap();
                assert_eq!(live.meter().provider_reads, 1);
                assert_eq!(live.meter().unique_source_bytes, 16);
                live.respond(&f.duty()).unwrap();
                assert_eq!(live.meter().provider_reads, 2);
                assert_eq!(live.meter().unique_source_bytes, 16);
            }
            "resume-after-kill" => {
                let mut live = DurableMinerResponderV2::resume(
                    &path,
                    f.responder(&providers, limits()),
                    journal_limits(),
                    TestFileAnchor(anchor_path),
                )
                .unwrap();
                assert_eq!(live.meter().provider_reads, 2);
                live.respond(&f.duty()).unwrap();
                assert_eq!(live.meter().provider_reads, 3);
                assert_eq!(live.meter().unique_source_bytes, 16);
            }
            "hold" => {
                let live = DurableMinerResponderV2::resume(
                    &path,
                    f.responder(&providers, limits()),
                    journal_limits(),
                    TestFileAnchor(anchor_path),
                )
                .unwrap();
                println!("A2_LEASE_HELD");
                std::io::stdout().flush().unwrap();
                std::io::stdin().read_exact(&mut [0u8; 1]).unwrap();
                drop(live);
            }
            "lease-denied" => {
                assert!(
                    DurableMinerResponderV2::resume(
                        &path,
                        f.responder(&providers, limits()),
                        journal_limits(),
                        TestFileAnchor(anchor_path)
                    )
                    .is_err()
                );
            }
            _ => panic!("unknown recovery test child phase"),
        }
    }
    #[test]
    fn separate_process_restart_preserves_counters_and_live_lease() {
        use std::io::BufRead;
        let case = TempCase::new();
        for phase in ["first", "resume"] {
            let output = child(&case.0, phase).output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        }
        let mut held =
            child(&case.0, "hold").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
        let mut output = std::io::BufReader::new(held.stdout.take().unwrap());
        let mut line = String::new();
        loop {
            assert!(output.read_line(&mut line).unwrap() > 0, "holder stopped before lease");
            if line.contains("A2_LEASE_HELD") {
                break;
            }
            line.clear();
        }
        let denied = child(&case.0, "lease-denied").output().unwrap();
        held.kill().unwrap();
        assert!(!held.wait().unwrap().success());
        assert!(denied.status.success(), "{}", String::from_utf8_lossy(&denied.stderr));
        let recovered = child(&case.0, "resume-after-kill").output().unwrap();
        assert!(recovered.status.success(), "{}", String::from_utf8_lossy(&recovered.stderr));
    }
}

#[cfg(all(test, unix))]
#[path = "positive_miner_v2_recovery_tail_tests.rs"]
mod tail_tests;
