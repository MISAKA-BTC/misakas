//! **RFC-0037 / ADR-0204 (both Proposed — not adopted): a bounded, authenticated row stream of the PALW pruning-point
//! state, verified incrementally against the header-committed root.**
//!
//! IBD decodes one `PruningPointPalwRowStream` generation and feeds those bytes through
//! [`palw_accept_row_stream_generation_v1`] before the existing pruning-point import. A stream this verifier refuses
//! is not imported. Nothing here changes what a header commits, and the state-root format is untouched: the stream
//! is checked against the SAME flat commitment the chain already makes
//! (ADR-0043 §2, [`crate::palw_state_proof_v1`]):
//!
//! ```text
//!   expected root (from the witness header, never from the peer)
//!     ◀── palw_state_root_of_preimage_v1 ── preimage            checked FIRST, before a single row is accepted
//!   preimage[offset .. offset+64]
//!     ◀── collection root (streamed) ────── that collection's rows   checked at the END of each collection
//! ```
//!
//! So a forged row is refused at the end of the collection it sits in, and the stream aborts there: the caller drops
//! its spill and promotes nothing. The rows go to a caller-supplied sink (the capped IBD spill file) as they arrive;
//! the verifier never holds a collection in memory except for the one non-standard collection kind (see
//! [`PalwRowStreamVerifierV1::begin_collection`]).
//!
//! **What this does NOT establish, and why the evidence type is two steps.** A verified stream proves that each
//! streamed collection is the committed one. It does not prove the stream named EVERY collection the preimage commits
//! (the preimage's layout depends on fences and on which tables exist, so the receiver cannot list its slots without
//! the state), and it does not run the whole-state consistency checks `into_state` runs today
//! (`assert_internal_consistency*`, the deadline/vesting/capacity index rebuild-and-compare). Both are done by the
//! mandatory second step, [`PalwRowStreamVerifiedV1::whole_state_checked`]: the caller assembles the typed state from
//! the spill and runs the existing import gate (`restore_native_carriage_v1` with the expected root — which recomputes
//! the full root, so an omitted collection fails there) or replays from an already-verified base. Only its output,
//! [`PalwWholeStateVerifiedV1`], is evidence a storage generation may be marked verified with. **Receiving a root
//! alone — or a stream that matched it — is never "verified".**

use crate::Hash64;
use crate::palw_state_proof_v1::PalwCollectionOpeningV1;
use crate::palw_state_v2::{PALW_STATE_V2_DOMAIN_COLLECTION, palw_state_root_of_preimage_v1};
use std::collections::BTreeSet;
use std::io::Write;

mod spill_reader;
pub use spill_reader::{PalwRowSpillEventV1, PalwRowSpillLimitsV1, palw_visit_row_spill_v1};

/// Upper bound on the preimage a peer may send (it is every collection's 64-byte root plus small scalar blocks; the
/// shipped preimage is a few KiB).
pub const PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1: usize = 1 << 20;

/// Spill framing tags (node-local file format of the receiver's spill; not a wire format).
pub const PALW_ROW_SPILL_COLLECTION_V1: u8 = b'C';
pub const PALW_ROW_SPILL_ROW_V1: u8 = b'R';
pub const PALW_ROW_SPILL_END_V1: u8 = b'E';

/// One current pruning-point generation. A peer cannot name another generation or a history range.
pub const PALW_ROW_STREAM_CURRENT_GENERATION_V1: u64 = 1;
/// Same total as the existing IBD spill and `PALW_PRUNING_STATE_MAX_BYTES` (384 MiB).
pub const PALW_ROW_STREAM_MAX_GENERATION_BYTES_V1: usize = 384 << 20;
pub const PALW_ROW_STREAM_GENERATION_MAGIC_V1: u32 = 0x3752_5357;
pub const PALW_ROW_STREAM_GENERATION_VERSION_V1: u16 = 1;
const PALW_ROW_STREAM_MAX_LABEL_BYTES_V1: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwRowStreamErrorV1 {
    #[error("preimage of {len} bytes exceeds {max}")]
    PreimageTooLarge { len: usize, max: usize },
    #[error("the preimage does not hash to the expected state root {0}")]
    PreimageDoesNotMatchRoot(Hash64),
    #[error("the stream already failed; nothing more is accepted")]
    Poisoned,
    #[error("a collection is already open")]
    CollectionAlreadyOpen,
    #[error("no collection is open")]
    NoOpenCollection,
    #[error("collection {0:?} appears twice")]
    DuplicateCollection(Vec<u8>),
    #[error("collection {label:?} names preimage offset {offset}, outside the preimage or overlapping another collection")]
    BadOffset { label: Vec<u8>, offset: usize },
    #[error("collection {label:?} declared {declared} rows and sent more")]
    TooManyRows { label: Vec<u8>, declared: u64 },
    #[error("collection {label:?} declared {declared} rows and sent {seen}")]
    RowCountMismatch { label: Vec<u8>, declared: u64, seen: u64 },
    #[error("collection {0:?} does not hash to the root the preimage commits at its offset")]
    CollectionMismatch(Vec<u8>),
    #[error("row bytes exceed the stream budget of {0}")]
    ByteBudget(u64),
    #[error("local spill: {0}")]
    Sink(String),
    #[error("whole-state check refused the streamed state: {0}")]
    WholeState(String),
    #[error("collection {0:?}: a row does not decode under its rooting rule: {1}")]
    Projection(Vec<u8>, String),
    #[error("generation {0} was already accepted")]
    DuplicateGeneration(u64),
    #[error("row stream generation is truncated")]
    TruncatedGeneration,
    #[error("generation {got} is not the single current generation {current}")]
    UnexpectedGeneration { got: u64, current: u64 },
    #[error("generation of {len} bytes exceeds {max}")]
    GenerationTooLarge { len: usize, max: usize },
}

/// One collection inside a row-stream generation.
pub struct PalwRowStreamCollectionV1 {
    pub label: Vec<u8>,
    pub offset: u64,
    pub rows: Vec<(Vec<u8>, Vec<u8>)>,
}

/// The verifier accepted this generation. [`Self::import_suffix`] is the existing snapshot envelope, and it is
/// only available after [`palw_accept_row_stream_generation_v1`] returns `Ok`.
pub struct PalwAcceptedRowStreamGenerationV1<W> {
    generation: u64,
    verified: PalwRowStreamVerifiedV1<W>,
    import_suffix: Vec<u8>,
}

impl<W> PalwAcceptedRowStreamGenerationV1<W> {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn import_suffix(&self) -> &[u8] {
        &self.import_suffix
    }
    /// Run the caller's whole-state check. IBD uses this only after the verifier has accepted the stream, then
    /// hands the suffix to the existing pruning-point import.
    pub fn into_whole_state(
        self,
        check: impl FnOnce(&mut W, &[u8], Hash64) -> Result<(), String>,
    ) -> Result<PalwWholeStateVerifiedV1<W>, PalwRowStreamErrorV1> {
        self.verified.whole_state_checked(check)
    }
}

/// A collection whose root hashes a PROJECTION of each record (e.g. `gen_classes` / `tir_classes`, rooted over
/// `rooted_bytes_v1()`): the wire and the spill carry the full record, and the root is computed over
/// `project(full record bytes)`. A record that does not decode is refused as a projection error.
pub type PalwRowProjectionV1 = Box<dyn Fn(&[u8]) -> Result<Vec<u8>, String> + Send>;

/// [`crate::palw_state_v2::palw_collection_root_of_entries_v1`], one row at a time — byte-identical by construction
/// (same key, same length-prefixed fields, same order) and by test.
#[derive(Clone)]
pub struct PalwCollectionRootStreamV1 {
    state: blake2b_simd::State,
    label: Vec<u8>,
    declared: u64,
    seen: u64,
}

impl PalwCollectionRootStreamV1 {
    pub fn new(label: &[u8], declared_rows: u64) -> Self {
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_STATE_V2_DOMAIN_COLLECTION).to_state();
        state.update(&(label.len() as u64).to_le_bytes());
        state.update(label);
        state.update(&declared_rows.to_le_bytes());
        Self { state, label: label.to_vec(), declared: declared_rows, seen: 0 }
    }
    pub fn row(&mut self, key: &[u8], value: &[u8]) -> Result<(), PalwRowStreamErrorV1> {
        if self.seen == self.declared {
            return Err(PalwRowStreamErrorV1::TooManyRows { label: self.label.clone(), declared: self.declared });
        }
        self.state.update(&(key.len() as u64).to_le_bytes());
        self.state.update(key);
        self.state.update(&(value.len() as u64).to_le_bytes());
        self.state.update(value);
        self.seen += 1;
        Ok(())
    }
    pub fn finish(self) -> Result<Hash64, PalwRowStreamErrorV1> {
        if self.seen != self.declared {
            return Err(PalwRowStreamErrorV1::RowCountMismatch { label: self.label, declared: self.declared, seen: self.seen });
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(self.state.finalize().as_bytes());
        Ok(Hash64::from_bytes(out))
    }
}

/// Collections whose root is not the standard entry hash (see `PalwCollectionOpeningV1::root`). Their rows are
/// buffered — inside the same byte budget — and rooted by that very function.
fn is_non_standard(label: &[u8]) -> bool {
    label == crate::palw_native_audit_proof_v32::NATIVE_AUX_LABEL_V32 || label == crate::palw_native_audit_proof_v32::NATIVE_PAYOUT_LABEL_V32
}

enum Rooter {
    Standard(PalwCollectionRootStreamV1),
    Buffered { declared: u64, rows: Vec<(Vec<u8>, Vec<u8>)> },
}

struct Open {
    label: Vec<u8>,
    offset: usize,
    rooter: Rooter,
    project: Option<PalwRowProjectionV1>,
}

/// The incremental verifier. Feed it `begin_collection` / `row`* / `end_collection` per collection, then `finish`.
/// Any error poisons it.
pub struct PalwRowStreamVerifierV1<W: Write> {
    expected_root: Hash64,
    preimage: Vec<u8>,
    sink: W,
    max_row_bytes: u64,
    row_bytes: u64,
    open: Option<Open>,
    done: Vec<(Vec<u8>, u64)>,
    labels: BTreeSet<Vec<u8>>,
    used: Vec<usize>,
    poisoned: bool,
}

impl<W: Write> PalwRowStreamVerifierV1<W> {
    /// `expected_root` must come from the witness header the import already trusts (the selected-chain child of the
    /// pruning point), never from the peer. The preimage is checked here, before any row is accepted.
    pub fn new(expected_root: Hash64, preimage: Vec<u8>, sink: W, max_row_bytes: u64) -> Result<Self, PalwRowStreamErrorV1> {
        if preimage.len() > PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 {
            return Err(PalwRowStreamErrorV1::PreimageTooLarge { len: preimage.len(), max: PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 });
        }
        if palw_state_root_of_preimage_v1(&preimage) != expected_root {
            return Err(PalwRowStreamErrorV1::PreimageDoesNotMatchRoot(expected_root));
        }
        Ok(Self {
            expected_root,
            preimage,
            sink,
            max_row_bytes,
            row_bytes: 0,
            open: None,
            done: Vec::new(),
            labels: BTreeSet::new(),
            used: Vec::new(),
            poisoned: false,
        })
    }

    pub fn preimage(&self) -> &[u8] {
        &self.preimage
    }
    pub fn row_bytes(&self) -> u64 {
        self.row_bytes
    }

    fn guard<T>(&mut self, r: Result<T, PalwRowStreamErrorV1>) -> Result<T, PalwRowStreamErrorV1> {
        if r.is_err() {
            self.poisoned = true;
        }
        r
    }

    fn sink_write(&mut self, bytes: &[u8]) -> Result<(), PalwRowStreamErrorV1> {
        self.sink.write_all(bytes).map_err(|e| PalwRowStreamErrorV1::Sink(e.to_string()))
    }

    /// Open the next collection: its label, the row count it will send, and the preimage offset of its 64-byte root.
    pub fn begin_collection(&mut self, label: &[u8], rows: u64, offset: usize) -> Result<(), PalwRowStreamErrorV1> {
        let r = self.begin_inner(label, rows, offset, None);
        self.guard(r)
    }
    /// [`Self::begin_collection`] for a collection rooted over a projection of its records.
    pub fn begin_projected_collection(
        &mut self,
        label: &[u8],
        rows: u64,
        offset: usize,
        project: PalwRowProjectionV1,
    ) -> Result<(), PalwRowStreamErrorV1> {
        let r = self.begin_inner(label, rows, offset, Some(project));
        self.guard(r)
    }
    fn begin_inner(
        &mut self,
        label: &[u8],
        rows: u64,
        offset: usize,
        project: Option<PalwRowProjectionV1>,
    ) -> Result<(), PalwRowStreamErrorV1> {
        if self.poisoned {
            return Err(PalwRowStreamErrorV1::Poisoned);
        }
        if self.open.is_some() {
            return Err(PalwRowStreamErrorV1::CollectionAlreadyOpen);
        }
        if !self.labels.insert(label.to_vec()) {
            return Err(PalwRowStreamErrorV1::DuplicateCollection(label.to_vec()));
        }
        let in_bounds = offset.checked_add(64).is_some_and(|end| end <= self.preimage.len());
        let overlaps = self.used.iter().any(|u| offset < u + 64 && *u < offset + 64);
        if !in_bounds || overlaps {
            return Err(PalwRowStreamErrorV1::BadOffset { label: label.to_vec(), offset });
        }
        self.used.push(offset);
        let mut header = vec![PALW_ROW_SPILL_COLLECTION_V1];
        header.extend_from_slice(&(label.len() as u32).to_le_bytes());
        header.extend_from_slice(label);
        header.extend_from_slice(&rows.to_le_bytes());
        self.sink_write(&header)?;
        let rooter = if is_non_standard(label) {
            Rooter::Buffered { declared: rows, rows: Vec::new() }
        } else {
            Rooter::Standard(PalwCollectionRootStreamV1::new(label, rows))
        };
        self.open = Some(Open { label: label.to_vec(), offset, rooter, project });
        Ok(())
    }

    /// One row of the open collection: hashed, counted against the byte budget, then written to the spill.
    pub fn row(&mut self, key: &[u8], value: &[u8]) -> Result<(), PalwRowStreamErrorV1> {
        let r = self.row_inner(key, value);
        self.guard(r)
    }
    fn row_inner(&mut self, key: &[u8], value: &[u8]) -> Result<(), PalwRowStreamErrorV1> {
        if self.poisoned {
            return Err(PalwRowStreamErrorV1::Poisoned);
        }
        let added = (key.len() + value.len()) as u64 + 9;
        let next = self.row_bytes.saturating_add(added);
        if next > self.max_row_bytes {
            return Err(PalwRowStreamErrorV1::ByteBudget(self.max_row_bytes));
        }
        let open = self.open.as_mut().ok_or(PalwRowStreamErrorV1::NoOpenCollection)?;
        let projected;
        let rooted: &[u8] = match &open.project {
            None => value,
            Some(project) => {
                projected = project(value).map_err(|why| PalwRowStreamErrorV1::Projection(open.label.clone(), why))?;
                &projected
            }
        };
        match &mut open.rooter {
            Rooter::Standard(h) => h.row(key, rooted)?,
            Rooter::Buffered { declared, rows } => {
                if rows.len() as u64 == *declared {
                    return Err(PalwRowStreamErrorV1::TooManyRows { label: open.label.clone(), declared: *declared });
                }
                rows.push((key.to_vec(), rooted.to_vec()));
            }
        }
        self.row_bytes = next;
        let mut frame = Vec::with_capacity(added as usize);
        frame.push(PALW_ROW_SPILL_ROW_V1);
        frame.extend_from_slice(&(key.len() as u32).to_le_bytes());
        frame.extend_from_slice(key);
        frame.extend_from_slice(&(value.len() as u32).to_le_bytes());
        frame.extend_from_slice(value);
        self.sink_write(&frame)
    }

    /// Close the open collection and compare its root with the preimage's 64 bytes at its offset. A mismatch here is
    /// the incremental abort: the stream stops at the first collection that is not the committed one.
    pub fn end_collection(&mut self) -> Result<Hash64, PalwRowStreamErrorV1> {
        let r = self.end_inner();
        self.guard(r)
    }
    fn end_inner(&mut self) -> Result<Hash64, PalwRowStreamErrorV1> {
        if self.poisoned {
            return Err(PalwRowStreamErrorV1::Poisoned);
        }
        let open = self.open.take().ok_or(PalwRowStreamErrorV1::NoOpenCollection)?;
        let (root, rows) = match open.rooter {
            Rooter::Standard(h) => {
                let rows = h.seen;
                (h.finish()?, rows)
            }
            Rooter::Buffered { declared, rows } => {
                if rows.len() as u64 != declared {
                    return Err(PalwRowStreamErrorV1::RowCountMismatch { label: open.label, declared, seen: rows.len() as u64 });
                }
                let n = rows.len() as u64;
                (PalwCollectionOpeningV1 { label: open.label.clone(), rows }.root(), n)
            }
        };
        // A default (all-zero) root is what the non-standard rooter returns for an opening it cannot parse; it is
        // never a match, whatever bytes the preimage holds there.
        if root == Hash64::default() || self.preimage[open.offset..open.offset + 64] != *root.as_byte_slice() {
            return Err(PalwRowStreamErrorV1::CollectionMismatch(open.label));
        }
        self.sink_write(&[PALW_ROW_SPILL_END_V1])?;
        self.done.push((open.label, rows));
        Ok(root)
    }

    /// All collections the stream named are closed and verified. Returns the stream-level token (NOT yet evidence).
    pub fn finish(mut self) -> Result<PalwRowStreamVerifiedV1<W>, PalwRowStreamErrorV1> {
        if self.poisoned {
            return Err(PalwRowStreamErrorV1::Poisoned);
        }
        if self.open.is_some() {
            self.poisoned = true;
            return Err(PalwRowStreamErrorV1::CollectionAlreadyOpen);
        }
        self.sink.flush().map_err(|e| PalwRowStreamErrorV1::Sink(e.to_string()))?;
        Ok(PalwRowStreamVerifiedV1 {
            root: self.expected_root,
            preimage: self.preimage,
            sink: self.sink,
            collections: self.done,
            row_bytes: self.row_bytes,
        })
    }
}

/// Every streamed collection matched the committed root. Not evidence by itself — see the module docs.
pub struct PalwRowStreamVerifiedV1<W> {
    root: Hash64,
    preimage: Vec<u8>,
    sink: W,
    collections: Vec<(Vec<u8>, u64)>,
    row_bytes: u64,
}

impl<W> PalwRowStreamVerifiedV1<W> {
    pub fn root(&self) -> Hash64 {
        self.root
    }
    pub fn preimage(&self) -> &[u8] {
        &self.preimage
    }
    pub fn collections(&self) -> &[(Vec<u8>, u64)] {
        &self.collections
    }
    pub fn row_bytes(&self) -> u64 {
        self.row_bytes
    }
    /// Drop the token and keep the spill — for a caller that abandons the import.
    pub fn into_sink(self) -> W {
        self.sink
    }

    /// The mandatory second step: `check` reads the spill (`&mut W`), the authenticated preimage and the expected root,
    /// assembles the typed state and runs the whole-state gate (the existing `restore_native_carriage_v1` with the
    /// expected root, or a replay from a verified base). Only `Ok` yields evidence.
    pub fn whole_state_checked(
        mut self,
        check: impl FnOnce(&mut W, &[u8], Hash64) -> Result<(), String>,
    ) -> Result<PalwWholeStateVerifiedV1<W>, PalwRowStreamErrorV1> {
        check(&mut self.sink, &self.preimage, self.root).map_err(PalwRowStreamErrorV1::WholeState)?;
        Ok(PalwWholeStateVerifiedV1 { root: self.root, sink: self.sink })
    }
}

/// Evidence that the streamed state is the committed state AND passed the whole-state checks. The only input a
/// storage generation's "verified" marker is built from.
pub struct PalwWholeStateVerifiedV1<W> {
    root: Hash64,
    sink: W,
}

impl<W> PalwWholeStateVerifiedV1<W> {
    pub fn root(&self) -> Hash64 {
        self.root
    }
    pub fn sink(&mut self) -> &mut W {
        &mut self.sink
    }
    pub fn into_sink(self) -> W {
        self.sink
    }
}

/// Read back one trusted local spill written by the verifier: `(label, rows)` per collection, in stream order.
/// This convenience collector retains the whole spill and trusts its length prefixes. For capped staging or
/// potentially damaged spill files, use [`palw_visit_row_spill_v1`] with explicit local limits instead.
pub fn palw_read_row_spill_v1(mut r: impl std::io::Read) -> std::io::Result<Vec<(Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>)>> {
    use std::io::{Error, ErrorKind};
    fn u32_of(r: &mut impl std::io::Read) -> std::io::Result<u32> {
        let mut b = [0u8; 4];
        r.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }
    fn bytes(r: &mut impl std::io::Read, n: u32) -> std::io::Result<Vec<u8>> {
        let mut v = vec![0u8; n as usize];
        r.read_exact(&mut v)?;
        Ok(v)
    }
    let mut out = Vec::new();
    let mut tag = [0u8; 1];
    loop {
        if r.read(&mut tag)? == 0 {
            return Ok(out);
        }
        if tag[0] != PALW_ROW_SPILL_COLLECTION_V1 {
            return Err(Error::new(ErrorKind::InvalidData, "spill: expected a collection header"));
        }
        let n = u32_of(&mut r)?;
        let label = bytes(&mut r, n)?;
        let mut count = [0u8; 8];
        r.read_exact(&mut count)?;
        let count = u64::from_le_bytes(count);
        let mut rows = Vec::new();
        for _ in 0..count {
            r.read_exact(&mut tag)?;
            if tag[0] != PALW_ROW_SPILL_ROW_V1 {
                return Err(Error::new(ErrorKind::InvalidData, "spill: expected a row"));
            }
            let k = u32_of(&mut r)?;
            let key = bytes(&mut r, k)?;
            let v = u32_of(&mut r)?;
            let value = bytes(&mut r, v)?;
            rows.push((key, value));
        }
        r.read_exact(&mut tag)?;
        if tag[0] != PALW_ROW_SPILL_END_V1 {
            return Err(Error::new(ErrorKind::InvalidData, "spill: collection not closed"));
        }
        out.push((label, rows));
    }
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn generation_frame_len(preimage: &[u8], collections: &[PalwRowStreamCollectionV1], suffix: &[u8]) -> Option<usize> {
    let mut total = 4usize + 2 + 8 + 8 + 4 + preimage.len() + 4 + 1 + 8 + suffix.len();
    for collection in collections {
        total = total.checked_add(4)?.checked_add(collection.label.len())?.checked_add(16)?;
        for (key, value) in &collection.rows {
            total = total.checked_add(8)?.checked_add(key.len())?.checked_add(value.len())?;
        }
    }
    Some(total)
}

/// Encode one current generation. The suffix is the existing snapshot envelope the IBD import decodes after the
/// verifier accepts the stream.
pub fn encode_row_stream_generation_v1(
    preimage: &[u8],
    collections: &[PalwRowStreamCollectionV1],
    import_suffix: &[u8],
) -> Result<Vec<u8>, PalwRowStreamErrorV1> {
    if preimage.len() > PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 {
        return Err(PalwRowStreamErrorV1::PreimageTooLarge { len: preimage.len(), max: PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 });
    }
    let len = generation_frame_len(preimage, collections, import_suffix).filter(|len| *len <= PALW_ROW_STREAM_MAX_GENERATION_BYTES_V1).ok_or(
        PalwRowStreamErrorV1::GenerationTooLarge {
            len: usize::MAX,
            max: PALW_ROW_STREAM_MAX_GENERATION_BYTES_V1,
        },
    )?;
    let mut stream = Vec::new();
    push_u32(&mut stream, preimage.len() as u32);
    stream.extend_from_slice(preimage);
    push_u32(&mut stream, collections.len() as u32);
    for collection in collections {
        if collection.label.len() > PALW_ROW_STREAM_MAX_LABEL_BYTES_V1 {
            return Err(PalwRowStreamErrorV1::TruncatedGeneration);
        }
        push_u32(&mut stream, collection.label.len() as u32);
        stream.extend_from_slice(&collection.label);
        push_u64(&mut stream, collection.rows.len() as u64);
        push_u64(&mut stream, collection.offset);
        for (key, value) in &collection.rows {
            if key.len() > u32::MAX as usize || value.len() > u32::MAX as usize {
                return Err(PalwRowStreamErrorV1::GenerationTooLarge { len, max: PALW_ROW_STREAM_MAX_GENERATION_BYTES_V1 });
            }
            push_u32(&mut stream, key.len() as u32);
            stream.extend_from_slice(key);
            push_u32(&mut stream, value.len() as u32);
            stream.extend_from_slice(value);
        }
    }
    stream.push(PALW_ROW_SPILL_END_V1);
    let mut out = Vec::with_capacity(len);
    push_u32(&mut out, PALW_ROW_STREAM_GENERATION_MAGIC_V1);
    push_u16(&mut out, PALW_ROW_STREAM_GENERATION_VERSION_V1);
    push_u64(&mut out, PALW_ROW_STREAM_CURRENT_GENERATION_V1);
    push_u64(&mut out, stream.len() as u64);
    out.extend_from_slice(&stream);
    push_u64(&mut out, import_suffix.len() as u64);
    out.extend_from_slice(import_suffix);
    if out.len() != len {
        return Err(PalwRowStreamErrorV1::TruncatedGeneration);
    }
    Ok(out)
}

fn read_exact<'a>(input: &mut &'a [u8], len: usize) -> Result<&'a [u8], PalwRowStreamErrorV1> {
    if input.len() < len {
        return Err(PalwRowStreamErrorV1::TruncatedGeneration);
    }
    let (head, tail) = input.split_at(len);
    *input = tail;
    Ok(head)
}
fn read_u8(input: &mut &[u8]) -> Result<u8, PalwRowStreamErrorV1> {
    Ok(read_exact(input, 1)?[0])
}
fn read_u16(input: &mut &[u8]) -> Result<u16, PalwRowStreamErrorV1> {
    let bytes = read_exact(input, 2)?;
    Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
}
fn read_u32(input: &mut &[u8]) -> Result<u32, PalwRowStreamErrorV1> {
    let bytes = read_exact(input, 4)?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}
fn read_u64(input: &mut &[u8]) -> Result<u64, PalwRowStreamErrorV1> {
    let bytes = read_exact(input, 8)?;
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}

/// Decode one generation and run it through [`PalwRowStreamVerifierV1`].
///
/// `expected_root` is the witness header's root, never a value taken from the peer. `already_imported` refuses a
/// second copy of the same generation. A short frame, a generation id other than [`PALW_ROW_STREAM_CURRENT_GENERATION_V1`],
/// or any verifier error returns `Err` and does not yield an import suffix.
pub fn palw_accept_row_stream_generation_v1<W: Write>(
    expected_root: Hash64,
    bytes: &[u8],
    sink: W,
    max_bytes: u64,
    already_imported: Option<u64>,
) -> Result<PalwAcceptedRowStreamGenerationV1<W>, PalwRowStreamErrorV1> {
    let cap = (PALW_ROW_STREAM_MAX_GENERATION_BYTES_V1 as u64).min(max_bytes);
    if bytes.len() as u64 > cap {
        return Err(PalwRowStreamErrorV1::GenerationTooLarge { len: bytes.len(), max: cap as usize });
    }
    let mut rest = bytes;
    if read_u32(&mut rest)? != PALW_ROW_STREAM_GENERATION_MAGIC_V1 || read_u16(&mut rest)? != PALW_ROW_STREAM_GENERATION_VERSION_V1 {
        return Err(PalwRowStreamErrorV1::TruncatedGeneration);
    }
    let generation = read_u64(&mut rest)?;
    if generation != PALW_ROW_STREAM_CURRENT_GENERATION_V1 {
        return Err(PalwRowStreamErrorV1::UnexpectedGeneration { got: generation, current: PALW_ROW_STREAM_CURRENT_GENERATION_V1 });
    }
    if already_imported == Some(generation) {
        return Err(PalwRowStreamErrorV1::DuplicateGeneration(generation));
    }
    let stream_len = usize::try_from(read_u64(&mut rest)?).map_err(|_| PalwRowStreamErrorV1::TruncatedGeneration)?;
    let mut stream = read_exact(&mut rest, stream_len)?;
    let preimage_len = usize::try_from(read_u32(&mut stream)?).map_err(|_| PalwRowStreamErrorV1::TruncatedGeneration)?;
    if preimage_len > PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 {
        return Err(PalwRowStreamErrorV1::PreimageTooLarge { len: preimage_len, max: PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 });
    }
    let preimage = read_exact(&mut stream, preimage_len)?.to_vec();
    let collection_count = read_u32(&mut stream)?;
    if collection_count as usize > preimage.len() / 64 {
        return Err(PalwRowStreamErrorV1::TruncatedGeneration);
    }
    let mut verifier = PalwRowStreamVerifierV1::new(expected_root, preimage, sink, cap)?;
    for _ in 0..collection_count {
        let label_len = usize::try_from(read_u32(&mut stream)?).map_err(|_| PalwRowStreamErrorV1::TruncatedGeneration)?;
        if label_len > PALW_ROW_STREAM_MAX_LABEL_BYTES_V1 {
            return Err(PalwRowStreamErrorV1::TruncatedGeneration);
        }
        let label = read_exact(&mut stream, label_len)?.to_vec();
        let rows = read_u64(&mut stream)?;
        let offset = usize::try_from(read_u64(&mut stream)?).map_err(|_| PalwRowStreamErrorV1::BadOffset {
            label: label.clone(),
            offset: usize::MAX,
        })?;
        if rows.saturating_mul(8) > stream.len() as u64 {
            return Err(PalwRowStreamErrorV1::TruncatedGeneration);
        }
        verifier.begin_collection(&label, rows, offset)?;
        for _ in 0..rows {
            let key_len = usize::try_from(read_u32(&mut stream)?).map_err(|_| PalwRowStreamErrorV1::TruncatedGeneration)?;
            let key = read_exact(&mut stream, key_len)?.to_vec();
            let value_len = usize::try_from(read_u32(&mut stream)?).map_err(|_| PalwRowStreamErrorV1::TruncatedGeneration)?;
            let value = read_exact(&mut stream, value_len)?.to_vec();
            verifier.row(&key, &value)?;
        }
        verifier.end_collection()?;
    }
    if read_u8(&mut stream)? != PALW_ROW_SPILL_END_V1 || !stream.is_empty() {
        return Err(PalwRowStreamErrorV1::TruncatedGeneration);
    }
    let suffix_len = usize::try_from(read_u64(&mut rest)?).map_err(|_| PalwRowStreamErrorV1::TruncatedGeneration)?;
    if suffix_len != rest.len() {
        return Err(PalwRowStreamErrorV1::TruncatedGeneration);
    }
    let verified = verifier.finish()?;
    Ok(PalwAcceptedRowStreamGenerationV1 { generation, verified, import_suffix: rest.to_vec() })
}

#[cfg(test)]
mod generation_tests {
    use super::*;
    use crate::palw_state_v2::{palw_collection_root_of_entries_v1, palw_state_root_of_preimage_v1};

    fn sample() -> (Hash64, Vec<u8>, PalwRowStreamCollectionV1) {
        let rows = vec![(b"k".to_vec(), b"value".to_vec())];
        let root = palw_collection_root_of_entries_v1(b"bonds", rows.len(), rows.iter().cloned());
        let mut preimage = vec![9, 8, 7];
        let offset = preimage.len() as u64;
        preimage.extend_from_slice(root.as_byte_slice());
        let state = palw_state_root_of_preimage_v1(&preimage);
        (state, preimage, PalwRowStreamCollectionV1 { label: b"bonds".to_vec(), offset, rows })
    }

    #[test]
    fn a_matching_generation_reaches_whole_state_and_keeps_the_import_suffix() {
        let (root, preimage, collection) = sample();
        let suffix = b"existing-snapshot-envelope".to_vec();
        let bytes = encode_row_stream_generation_v1(&preimage, &[collection], &suffix).unwrap();
        let accepted = palw_accept_row_stream_generation_v1(root, &bytes, Vec::<u8>::new(), bytes.len() as u64, None).unwrap();
        assert_eq!(accepted.generation(), PALW_ROW_STREAM_CURRENT_GENERATION_V1);
        assert_eq!(accepted.import_suffix(), suffix.as_slice());
        let evidence = accepted.into_whole_state(|_, preimage, root| {
            if palw_state_root_of_preimage_v1(preimage) == root { Ok(()) } else { Err("preimage".into()) }
        });
        assert!(evidence.is_ok());
    }

    #[test]
    fn a_stream_the_verifier_refuses_does_not_yield_an_import_suffix() {
        let (root, preimage, mut collection) = sample();
        collection.rows[0].1[0] ^= 1;
        let bytes = encode_row_stream_generation_v1(&preimage, &[collection], b"suffix").unwrap();
        let refused = palw_accept_row_stream_generation_v1(root, &bytes, Vec::<u8>::new(), u64::MAX, None);
        assert!(matches!(refused, Err(PalwRowStreamErrorV1::CollectionMismatch(_))));
    }

    #[test]
    fn duplicate_truncated_and_non_current_generations_are_refused() {
        let (root, preimage, collection) = sample();
        let bytes = encode_row_stream_generation_v1(&preimage, &[collection], b"suffix").unwrap();
        assert!(matches!(
            palw_accept_row_stream_generation_v1(root, &bytes, Vec::<u8>::new(), u64::MAX, Some(PALW_ROW_STREAM_CURRENT_GENERATION_V1)),
            Err(PalwRowStreamErrorV1::DuplicateGeneration(_))
        ));
        assert!(matches!(
            palw_accept_row_stream_generation_v1(root, &bytes[..bytes.len() - 1], Vec::<u8>::new(), u64::MAX, None),
            Err(PalwRowStreamErrorV1::TruncatedGeneration)
        ));
        let mut other = bytes.clone();
        assert_eq!(other[6], 1, "generation id is the first byte after the 4-byte magic and 2-byte version");
        other[6] = 2;
        assert!(matches!(
            palw_accept_row_stream_generation_v1(root, &other, Vec::<u8>::new(), u64::MAX, None),
            Err(PalwRowStreamErrorV1::UnexpectedGeneration { .. })
        ));
        assert!(matches!(
            palw_accept_row_stream_generation_v1(root, &bytes, Vec::<u8>::new(), 8, None),
            Err(PalwRowStreamErrorV1::GenerationTooLarge { .. })
        ));
    }
}
