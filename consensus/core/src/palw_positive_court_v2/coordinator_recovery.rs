//! Private, bounded recovery of the WHOLE coordinator. No native wire IDs,
//! activation, credit, or coin transfers are introduced here. An adapter must
//! bind the expected head and EVERY event's origin to its selected history.
//! Hashes authenticate journal identity only; all computations are replayed.
use super::*;
use crate::palw_native_resource_v1::wire::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Read, Write};
use std::path::Path;

// Schema 1 remains readable byte-for-byte. New private input variants are
// appended at tags 7..9; tags 0..6 and all existing archive/hash identities are
// unchanged. Older readers fail closed on an unknown appended event. This is
// not a native codec/tag assignment and does not rewrite stored generations.
const SCHEMA: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EventOriginV2 {
    pub carrier: Hash64,
    pub daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SavedPartV2 {
    pub cone: Vec<u8>,
    pub trace: Option<Vec<u8>>,
}
/// Inputs, including inputs which consume budgets before returning an error.
/// There are deliberately no ACK, cursor, meter, payment, or Pass setters.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum RecoveryEventV2 {
    Request {
        panel: Hash64,
        first: u64,
        count: u64,
        signature: Vec<u8>,
    },
    Response {
        index: u64,
        bytes: Vec<u8>,
        opening: PalwStepOpeningV1,
    },
    Candidate {
        panel: Hash64,
        witness: PanelWitnessV2,
        signature: Vec<u8>,
    },
    Batch {
        witness: Hash64,
        first: u64,
        parts: Vec<SavedPartV2>,
    },
    Finish,
    Expire,
    Revoke,
    ConfigureTransport {
        max_chunk_bytes: u64,
        max_buffer_bytes: u64,
        max_wire_bytes: u64,
        max_packets: u64,
    },
    MaterialChunk {
        witness: Hash64,
        index: u64,
        cone_bytes: u64,
        trace_bytes: Option<u64>,
        payload: Hash64,
        offset: u64,
        bytes: Vec<u8>,
    },
    DiscardTransport,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Record {
    origin: EventOriginV2,
    event: RecoveryEventV2,
}
#[derive(BorshSerialize, BorshDeserialize)]
struct Image {
    schema: u16,
    admission: Hash64,
    records: Vec<Record>,
}
/// Must be resolved from the selected local/native transaction, not supplied by
/// an IBD peer together with its archive. A missing or shorter tail is an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryHeadV2 {
    pub records: u64,
    pub digest: Hash64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryLimitsV2 {
    pub max_records: u64,
    pub max_encoded_bytes: u64,
    pub max_decoded_heap: u64,
}
#[derive(Debug)]
pub enum RecoveryErrorV2 {
    Invalid(ErrorV2),
    Persistence(io::Error),
    /// A failed persistence call might have committed. Resolve its head against
    /// the durable store before resuming; never keep spending the old budgets.
    Indeterminate,
}
impl From<ErrorV2> for RecoveryErrorV2 {
    fn from(value: ErrorV2) -> Self {
        Self::Invalid(value)
    }
}

fn fields<T: NativeWireScanV1, R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
    T::scan_native_wire_v1(r)
}
macro_rules! scan_fields {
    ($r:expr; $($t:ty),+ $(,)?) => {{
        let mut sum = NativeWireShapeV1::default();
        $(sum = sum.merge(fields::<$t, _>($r)?)?;)+
        Ok(sum)
    }};
}
impl NativeWireScanV1 for EventOriginV2 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        scan_fields!(r; Hash64, u64)
    }
}
impl NativeWireScanV1 for SavedPartV2 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        scan_fields!(r; Vec<u8>, Option<Vec<u8>>)
    }
}
impl NativeWireScanV1 for RecoveryEventV2 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        match r.byte()? {
            0 => scan_fields!(r; Hash64, u64, u64, Vec<u8>),
            1 => scan_fields!(r; u64, Vec<u8>, PalwStepOpeningV1),
            2 => scan_fields!(r; Hash64, u16, Hash64, Hash64, u32, crate::palw_tir_step_v1::PalwTirStepBindingV1, Vec<u32>, Vec<u8>),
            3 => scan_fields!(r; Hash64, u64, Vec<SavedPartV2>),
            4..=6 | 9 => Ok(NativeWireShapeV1::default()),
            7 => scan_fields!(r; u64, u64, u64, u64),
            8 => scan_fields!(r; Hash64, u64, u64, Option<u64>, Hash64, u64, Vec<u8>),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "unknown recovery event")),
        }
    }
}
impl NativeWireScanV1 for Record {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        scan_fields!(r; EventOriginV2, RecoveryEventV2)
    }
}
impl NativeWireScanV1 for Image {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        scan_fields!(r; u16, Hash64, Vec<Record>)
    }
}

fn head(bytes: &[u8], records: u64) -> RecoveryHeadV2 {
    RecoveryHeadV2 { records, digest: digest(b"misaka/private-pc-recovery/v1", &bytes) }
}
fn preflight(bytes: &[u8], limits: RecoveryLimitsV2) -> Result<(), ErrorV2> {
    if bytes.len() as u64 > limits.max_encoded_bytes {
        return Err(ErrorV2::Budget);
    }
    // The count is checked before even scanning records; all nested vectors are
    // allocation-free scanned before Borsh is allowed to allocate anything.
    let count = bytes.get(66..70).ok_or(ErrorV2::Wire)?;
    if u32::from_le_bytes(count.try_into().unwrap()) as u64 > limits.max_records {
        return Err(ErrorV2::Budget);
    }
    if super::super::scan_heap::<Image>(bytes)? > limits.max_decoded_heap {
        return Err(ErrorV2::Budget);
    }
    Ok(())
}
fn pristine(c: &PositiveCoordinatorV2) -> bool {
    c.transport.is_none()
        && c.current.is_none()
        && c.history.is_empty()
        && c.meter.attempts == 0
        && c.service_meter().rounds == 0
        && c.service_meter().transmitted_bytes == 0
        && c.claim == ClaimStatusV2::Checking
}
fn admission(c: &PositiveCoordinatorV2, admitted_at: u64, limits: RecoveryLimitsV2) -> Hash64 {
    let l = c.limits;
    // Limits, roster and prompt form cannot change between restart/reorg/IBD.
    digest(
        b"misaka/private-pc-admission/v1",
        &(
            c.result.id(),
            c.policy,
            &c.registered,
            &c.miner_key,
            &c.roster.0,
            c.prompt_form,
            admitted_at,
            (limits.max_records, limits.max_encoded_bytes, limits.max_decoded_heap),
            (
                l.max_attempts,
                l.max_reserved_elements,
                l.max_reserved_terms,
                l.max_reserved_witness_bytes,
                l.max_history_bytes,
                l.max_batch_parts,
                l.max_batch_bytes,
            ),
        ),
    )
}
fn apply(c: &mut PositiveCoordinatorV2, event: &RecoveryEventV2, now: u64) -> Result<(), ErrorV2> {
    match event {
        RecoveryEventV2::Request { panel, first, count, signature } => c
            .request_source(*panel, WorkRequestV2::Source { first: *first, count: *count }, &SignatureV2(signature.clone()), now)
            .map(|_| ()),
        RecoveryEventV2::Response { index, bytes, opening } => c.respond_source(*index, bytes, opening, now),
        RecoveryEventV2::Candidate { panel, witness, signature } => {
            c.open_candidate(*panel, witness.clone(), &SignatureV2(signature.clone()), now).map(|_| ())
        }
        RecoveryEventV2::Batch { witness, first, parts } => {
            let parts: Vec<_> = parts.iter().map(|p| EncodedPartV2 { cone: &p.cone, trace: p.trace.as_deref() }).collect();
            c.submit_batch(*witness, *first, &parts, now).map(|_| ())
        }
        RecoveryEventV2::Finish => c.finish(now).map(|_| ()),
        RecoveryEventV2::Expire => {
            c.expire(now);
            Ok(())
        }
        RecoveryEventV2::Revoke => {
            c.revoke();
            Ok(())
        }
        RecoveryEventV2::ConfigureTransport { max_chunk_bytes, max_buffer_bytes, max_wire_bytes, max_packets } => {
            c.expire(now);
            c.configure_material_transport(transport::MaterialTransportLimitsV2 {
                max_chunk_bytes: *max_chunk_bytes,
                max_buffer_bytes: *max_buffer_bytes,
                max_wire_bytes: *max_wire_bytes,
                max_packets: *max_packets,
            })
        }
        RecoveryEventV2::MaterialChunk { witness, index, cone_bytes, trace_bytes, payload, offset, bytes } => c
            .submit_material_chunk(
                transport::MaterialDescriptorV2 {
                    witness: *witness,
                    index: *index,
                    cone_bytes: *cone_bytes,
                    trace_bytes: *trace_bytes,
                    payload: *payload,
                },
                *offset,
                bytes,
                now,
            )
            .map(|_| ()),
        RecoveryEventV2::DiscardTransport => {
            c.expire(now);
            c.discard_material_buffer();
            Ok(())
        }
    }
}

/// Owns the coordinator: there is no mutable escape hatch around persistence.
/// Every attempted operation is durably recorded BEFORE its possibly partial
/// effects. The sink must commit the image AND head atomically with A4 resource
/// reservations / A5 coin effects in native integration. Local archives below
/// are private recovery artifacts, not such a native DB transaction.
pub struct JournaledCoordinatorV2 {
    coordinator: PositiveCoordinatorV2,
    image: Image,
    bytes: Vec<u8>,
    limits: RecoveryLimitsV2,
    last_daa: u64,
    indeterminate: bool,
}
impl JournaledCoordinatorV2 {
    pub fn new(c: PositiveCoordinatorV2, admitted_at: u64, limits: RecoveryLimitsV2) -> Result<Self, ErrorV2> {
        if !pristine(&c) || admitted_at >= c.policy.deadline {
            return Err(ErrorV2::Order);
        }
        let image = Image { schema: SCHEMA, admission: admission(&c, admitted_at, limits), records: Vec::new() };
        let bytes = borsh::to_vec(&image).map_err(|_| ErrorV2::Wire)?;
        preflight(&bytes, limits)?;
        Ok(Self { coordinator: c, image, bytes, limits, last_daa: admitted_at, indeterminate: false })
    }
    pub fn state(&self) -> Result<&PositiveCoordinatorV2, RecoveryErrorV2> {
        if self.indeterminate {
            return Err(RecoveryErrorV2::Indeterminate);
        }
        Ok(&self.coordinator)
    }
    pub fn head(&self) -> Result<RecoveryHeadV2, RecoveryErrorV2> {
        self.state()?;
        Ok(head(&self.bytes, self.image.records.len() as u64))
    }
    pub fn encoded(&self) -> Result<&[u8], RecoveryErrorV2> {
        self.state()?;
        Ok(&self.bytes)
    }
    /// The clock last applied to this journal. Expiry is re-derived from it;
    /// it is not a peer-supplied record.
    pub fn observed_daa(&self) -> Result<u64, RecoveryErrorV2> {
        self.state()?;
        Ok(self.last_daa)
    }
    /// Project the native clock without consuming a journal record. A full
    /// input archive cannot pin an unrefunded hold past an authenticated
    /// deadline. The encoded head stays the input image; restore derives the
    /// same expiry from the receiving clock.
    pub fn advance_clock(&mut self, now: u64, authenticate: impl FnOnce(u64) -> bool) -> Result<(), RecoveryErrorV2> {
        if self.indeterminate {
            return Err(RecoveryErrorV2::Indeterminate);
        }
        if now < self.last_daa || !authenticate(now) {
            return Err(ErrorV2::Binding.into());
        }
        self.coordinator.expire(now);
        self.last_daa = now;
        Ok(())
    }
    /// Clock projection separated from the store write. The archive head is
    /// unchanged; publication records the authenticated clock beside it.
    pub fn prepare_clock(mut self, now: u64, authenticate: impl FnOnce(u64) -> bool) -> Result<PreparedRecoveryUpdateV2, RecoveryErrorV2> {
        let previous = self.head()?;
        self.advance_clock(now, authenticate)?;
        Ok(PreparedRecoveryUpdateV2 { journal: self, previous, outcome: Ok(()) })
    }

    /// `authenticate` must check selected carrier membership, its accepted DAA,
    /// and the event's canonical order (including trusted expiry/revocation).
    /// A hash selected by the submitting peer is insufficient.
    pub fn record(
        &mut self,
        origin: EventOriginV2,
        event: RecoveryEventV2,
        authenticate: impl FnOnce(EventOriginV2, &RecoveryEventV2) -> bool,
        persist: impl FnOnce(&[u8], RecoveryHeadV2) -> io::Result<()>,
    ) -> Result<Result<(), ErrorV2>, RecoveryErrorV2> {
        if self.indeterminate {
            return Err(RecoveryErrorV2::Indeterminate);
        }
        if origin.daa < self.last_daa || !authenticate(origin, &event) {
            return Err(ErrorV2::Binding.into());
        }
        if self.image.records.len() as u64 >= self.limits.max_records {
            return Err(ErrorV2::Budget.into());
        }
        let record = Record { origin, event };
        let len = borsh::object_length(&record).map_err(|_| ErrorV2::Wire)? as u64;
        if (self.bytes.len() as u64).checked_add(len).filter(|n| *n <= self.limits.max_encoded_bytes).is_none() {
            return Err(ErrorV2::Budget.into());
        }
        self.image.records.push(record);
        let bytes = match borsh::to_vec(&self.image) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.image.records.pop();
                return Err(ErrorV2::Wire.into());
            }
        };
        if let Err(error) = preflight(&bytes, self.limits) {
            self.image.records.pop();
            return Err(error.into());
        }
        let next_head = head(&bytes, self.image.records.len() as u64);
        if let Err(error) = persist(&bytes, next_head) {
            self.indeterminate = true;
            return Err(RecoveryErrorV2::Persistence(error));
        }
        self.bytes = bytes;
        self.last_daa = origin.daa;
        // Retain failures: source validation and candidate admission can charge
        // work/attempts or reject a witness before returning Err.
        Ok(apply(&mut self.coordinator, &self.image.records.last().unwrap().event, origin.daa))
    }
    /// Restore into a NEW admission. On any malformed suffix, wrong head or
    /// origin failure, the partially replayed coordinator is dropped entirely.
    /// At the end use the receiving node's clock, so downtime extends no duty.
    pub fn restore(
        c: PositiveCoordinatorV2,
        admitted_at: u64,
        bytes: &[u8],
        expected: RecoveryHeadV2,
        limits: RecoveryLimitsV2,
        now: u64,
        authenticate: impl Fn(EventOriginV2, &RecoveryEventV2) -> bool,
    ) -> Result<Self, ErrorV2> {
        preflight(bytes, limits)?;
        let image: Image = borsh::from_slice(bytes).map_err(|_| ErrorV2::Wire)?;
        if image.schema != SCHEMA
            || head(bytes, image.records.len() as u64) != expected
            || !pristine(&c)
            || image.admission != admission(&c, admitted_at, limits)
            || now < admitted_at
        {
            return Err(ErrorV2::Binding);
        }
        let mut restored = Self::new(c, admitted_at, limits)?;
        for record in &image.records {
            if record.origin.daa < restored.last_daa || record.origin.daa > now || !authenticate(record.origin, &record.event) {
                return Err(ErrorV2::Binding);
            }
            // Recompute the outcome, never deserialize a saved outcome flag.
            let _outcome = apply(&mut restored.coordinator, &record.event, record.origin.daa);
            restored.last_daa = record.origin.daa;
        }
        restored.coordinator.expire(now);
        // The expiry is derived from `now` and is not an invented historical
        // carrier. Subsequent events must not move backwards from this clock.
        restored.last_daa = now;
        restored.image = image;
        restored.bytes = bytes.to_vec();
        Ok(restored)
    }
}

/// A clock projection that has been checked and not yet published. It carries
/// no store write and no Gate C authority.
pub struct PreparedRecoveryUpdateV2 {
    journal: JournaledCoordinatorV2,
    previous: RecoveryHeadV2,
    outcome: Result<(), ErrorV2>,
}
impl PreparedRecoveryUpdateV2 {
    pub fn observed_daa(&self) -> u64 {
        self.journal.last_daa
    }
    pub fn checked_state(&self) -> &PositiveCoordinatorV2 {
        &self.journal.coordinator
    }
    pub fn commit(
        self,
        persist: impl FnOnce(RecoveryHeadV2, RecoveryHeadV2, &[u8], &PositiveCoordinatorV2) -> io::Result<()>,
    ) -> Result<(JournaledCoordinatorV2, Result<(), ErrorV2>), RecoveryErrorV2> {
        let PreparedRecoveryUpdateV2 { journal, previous, outcome } = self;
        let state = journal.state()?;
        let head = journal.head()?;
        let archive = journal.encoded()?;
        persist(previous, head, archive, state).map_err(RecoveryErrorV2::Persistence)?;
        Ok((journal, outcome))
    }
}

/// Resolved from the caller's trusted selected-history store. This is an
/// explicit adapter over the PRIVATE coordinator, not an alias or decoder for
/// A0 result/witness wire objects. Native ingress must supply its own binding.
pub struct RecoveryAdmissionV2 {
    pub coordinator: PositiveCoordinatorV2,
    pub admitted_at: u64,
    pub expected: RecoveryHeadV2,
}
pub trait RecoveryContextV2 {
    type TrustedAnchor;
    fn resolve(&self, anchor: &Self::TrustedAnchor) -> Result<RecoveryAdmissionV2, ErrorV2>;
    fn authenticate(&self, anchor: &Self::TrustedAnchor, origin: EventOriginV2, event: &RecoveryEventV2) -> bool;
    fn current_daa(&self) -> u64;
}
pub struct RecoveryAdapterV2<C> {
    context: C,
    limits: RecoveryLimitsV2,
}
impl<C> RecoveryAdapterV2<C> {
    pub fn new(context: C, limits: RecoveryLimitsV2) -> Self {
        Self { context, limits }
    }
}
impl<C: RecoveryContextV2> misaka_palw_positive_contract::ports::Recovery for RecoveryAdapterV2<C> {
    type TrustedAnchor = C::TrustedAnchor;
    type BoundedJournal = Vec<u8>;
    type RestoredState = JournaledCoordinatorV2;
    type Error = ErrorV2;
    fn replay(&self, anchor: &Self::TrustedAnchor, journal: Vec<u8>) -> Result<JournaledCoordinatorV2, ErrorV2> {
        let admitted = self.context.resolve(anchor)?;
        JournaledCoordinatorV2::restore(
            admitted.coordinator,
            admitted.admitted_at,
            &journal,
            admitted.expected,
            self.limits,
            self.context.current_daa(),
            |origin, event| self.context.authenticate(anchor, origin, event),
        )
    }
}

/// Bounded archive input for disk/IBD adapters. The archive still needs a
/// separately trusted head and selected-history authentication in `restore`.
pub fn read_generation<R: Read>(reader: R, limits: RecoveryLimitsV2) -> Result<Vec<u8>, ErrorV2> {
    let cap = limits.max_encoded_bytes.checked_add(1).ok_or(ErrorV2::Budget)?;
    let mut bytes = Vec::new();
    reader.take(cap).read_to_end(&mut bytes).map_err(|_| ErrorV2::Wire)?;
    preflight(&bytes, limits)?;
    Ok(bytes)
}

/// Write an immutable archive generation, then sync its directory. The native
/// caller selects a generation by the expected head in its own atomic journal.
/// A crash leaves an unselected generation or a temp file, never a replacement
/// for a previously selected generation. This does not create a "current" flag.
#[cfg(not(target_arch = "wasm32"))]
pub fn persist_generation(directory: &Path, bytes: &[u8], expected: RecoveryHeadV2) -> io::Result<std::path::PathBuf> {
    if head(bytes, expected.records) != expected {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "recovery head mismatch"));
    }
    let final_path = directory.join(format!("{}-{}.pcj", expected.records, expected.digest));
    // Do not truncate any existing generation or follow a peer-provided path.
    let temp = directory.join(format!(".pcj-{}-{}", std::process::id(), rand::random::<u64>()));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        match std::fs::hard_link(&temp, &final_path) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let old = std::fs::File::open(&final_path)?;
                if old.metadata()?.len() != bytes.len() as u64 {
                    return Err(e);
                }
                let mut reader = old.take(bytes.len() as u64 + 1);
                let mut stored = Vec::new();
                reader.read_to_end(&mut stored)?;
                if stored != bytes {
                    return Err(e);
                }
            }
            Err(e) => return Err(e),
        }
        std::fs::File::open(directory)?.sync_all()?;
        Ok(final_path)
    })();
    let _ = std::fs::remove_file(temp);
    result
}

/// Pruning is controlled by rooted horizons, never by a saved Positive flag.
/// When no authenticated archive remains, full-history self-verification must
/// remain unavailable; an explicit pruning-point trust import is a separate
/// native policy and cannot call `restore` with a flag-only snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetentionV2 {
    pub pending: bool,
    pub evidence_until: u64,
    pub liability_until: u64,
    pub fee_until: u64,
    pub reorg_until: u64,
    pub archive_available: bool,
}
impl RetentionV2 {
    pub fn may_prune_for_self_verifying_import(&self, now: u64) -> bool {
        !self.pending
            && self.archive_available
            && [self.evidence_until, self.liability_until, self.fee_until, self.reorg_until].into_iter().all(|t| now > t)
    }
}

#[cfg(test)]
#[path = "coordinator_recovery_tests.rs"]
mod tests;
