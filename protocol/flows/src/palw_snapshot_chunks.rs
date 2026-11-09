//! Protocol-106 PALW pruning snapshot spill/reassembly. Existing total byte policy is
//! preserved; chunk hashes authenticate a fixed point/stream and native import still
//! verifies the consensus state root. No decoded snapshot is installed by this module.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
use kaspa_hashes::{Hash64, MerkleBranchHash64};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Native quota exhaustion happens after structural authentication, before
/// reconstruction; it cannot be encoded as malformed peer Borsh.
#[derive(Debug, thiserror::Error)]
pub(crate) enum NativeSnapshotDecodeErrorV1 {
    #[error("{0}")]
    Encoding(io::Error),
    #[error("{0}")]
    Capacity(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1),
    #[error("local snapshot spill: {0}")]
    LocalStorage(io::Error),
}

impl From<io::Error> for NativeSnapshotDecodeErrorV1 {
    fn from(error: io::Error) -> Self {
        // Nested File -> BufReader -> zlib -> Borsh errors retain their OS
        // cause. Format/CRC/count failures use synthetic InvalidData/EOF.
        if error.raw_os_error().is_some() {
            Self::LocalStorage(error)
        } else if matches!(error.kind(), io::ErrorKind::Unsupported | io::ErrorKind::TimedOut | io::ErrorKind::OutOfMemory) {
            Self::Capacity(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<PalwStateCarriageV2>(
                "native snapshot local processing",
                &error.to_string(),
            ))
        } else {
            Self::Encoding(error)
        }
    }
}

fn row_failure_v1(error: kaspa_consensus_core::palw_row_stream_v1::PalwRowStreamErrorV1) -> NativeSnapshotDecodeErrorV1 {
    match error {
        kaspa_consensus_core::palw_row_stream_v1::PalwRowStreamErrorV1::NativeResource(e) => NativeSnapshotDecodeErrorV1::Capacity(e),
        kaspa_consensus_core::palw_row_stream_v1::PalwRowStreamErrorV1::Sink(e) => {
            NativeSnapshotDecodeErrorV1::LocalStorage(io::Error::other(e))
        }
        other => NativeSnapshotDecodeErrorV1::Encoding(io::Error::new(io::ErrorKind::InvalidData, other.to_string())),
    }
}

pub(crate) const CHUNK_BYTES: usize = 4 << 20;
const COMPRESSION_INPUT_BYTES: usize = 64 << 10;
pub(crate) const SESSION_TTL: Duration = Duration::from_secs(600);
/// Two sessions across all peers and receive/serve flows: at most 768 MiB spill files
/// and two 4 MiB chunk buffers. Decoded/native state memory is additional, not a RAM proof.
const GLOBAL_SESSIONS: usize = 2;
/// Experimental canonical ENCODED cap for protocol107. This is not a decoded RAM
/// bound or approval of all native/model state. The 100k/four-population lower
/// bound is 424,900,000 bytes; this profile leaves 111,970,912 bytes above it.
pub(crate) const DECODED_SNAPSHOT_MAX_BYTES: u64 = 512 << 20;
pub(crate) const SNAPSHOT_CODEC_ZLIB_V1: u32 = 1;
pub(crate) const SNAPSHOT_CODEC_ROWS_V1: u32 = 2;
pub(crate) fn compression_request_nonce(protocol: u32, codec: u32, nonce: &[u8]) -> io::Result<Option<[u8; 32]>> {
    if codec == 0 && nonce.is_empty() {
        return Ok(None);
    }
    let negotiated = (codec == SNAPSHOT_CODEC_ZLIB_V1 && protocol >= crate::flow_context::PROTOCOL_VERSION_PALW_SNAPSHOT_COMPRESSED)
        || (codec == SNAPSHOT_CODEC_ROWS_V1 && crate::flow_context::palw_row_stream_negotiated_v1(protocol));
    if !negotiated {
        return Err(invalid("snapshot compression was not negotiated"));
    }
    let nonce: [u8; 32] = nonce.try_into().map_err(|_| invalid("snapshot nonce width differs"))?;
    if nonce == [0; 32] {
        return Err(invalid("snapshot nonce is empty"));
    }
    Ok(Some(nonce))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotCompressionV1 {
    pub codec: u32,
    pub nonce: [u8; 32],
    pub decoded: u64,
}
impl SnapshotCompressionV1 {
    pub(crate) fn from_wire(codec: u32, nonce: &[u8], decoded: u64) -> io::Result<Option<Self>> {
        if codec == 0 && nonce.is_empty() && decoded == 0 {
            return Ok(None);
        }
        let cap = if codec == SNAPSHOT_CODEC_ROWS_V1 {
            crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64
        } else {
            DECODED_SNAPSHOT_MAX_BYTES
        };
        if ![SNAPSHOT_CODEC_ZLIB_V1, SNAPSHOT_CODEC_ROWS_V1].contains(&codec) || decoded == 0 || decoded > cap {
            return Err(invalid("snapshot compression codec/canonical length differs"));
        }
        let nonce: [u8; 32] = nonce.try_into().map_err(|_| invalid("snapshot nonce width differs"))?;
        if nonce == [0; 32] {
            return Err(invalid("snapshot nonce is empty"));
        }
        Ok(Some(Self { codec, nonce, decoded }))
    }
    fn bind(&self, hash: &mut MerkleBranchHash64) {
        hash.write(self.codec.to_le_bytes());
        hash.write(self.nonce);
        hash.write(self.decoded.to_le_bytes());
    }
}

fn session_budget() -> Arc<Semaphore> {
    static BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();
    Arc::clone(BUDGET.get_or_init(|| Arc::new(Semaphore::new(GLOBAL_SESSIONS))))
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn stream_hasher(point: Hash64) -> MerkleBranchHash64 {
    stream_hasher_profile(point, None)
}
fn stream_hasher_profile(point: Hash64, profile: Option<SnapshotCompressionV1>) -> MerkleBranchHash64 {
    let mut h = MerkleBranchHash64::new();
    h.write(if profile.is_some() { b"misaka-palw/pruning-spill/v2" } else { b"misaka-palw/pruning-spill/v1" });
    h.write(point.as_byte_slice());
    if let Some(profile) = profile {
        profile.bind(&mut h);
    }
    h
}
#[cfg(test)] // The live raw106 path uses the same profile helper with None.
fn chunk_hash(point: Hash64, root: Hash64, total: u64, count: u32, index: u32, bytes: &[u8]) -> Hash64 {
    chunk_hash_profile(point, root, total, count, index, bytes, None)
}
pub(crate) fn chunk_hash_profile(
    point: Hash64,
    root: Hash64,
    total: u64,
    count: u32,
    index: u32,
    bytes: &[u8],
    profile: Option<SnapshotCompressionV1>,
) -> Hash64 {
    let mut h = MerkleBranchHash64::new();
    h.write(if profile.is_some() { b"misaka-palw/pruning-chunk/v2" } else { b"misaka-palw/pruning-chunk/v1" });
    h.write(point.as_byte_slice());
    if let Some(profile) = profile {
        profile.bind(&mut h);
    }
    h.write(root.as_byte_slice());
    h.write(total.to_le_bytes());
    h.write(count.to_le_bytes());
    h.write(index.to_le_bytes());
    h.write((bytes.len() as u64).to_le_bytes());
    h.write(bytes);
    h.finalize()
}
fn chunk_count(total: u64) -> u32 {
    total.div_ceil(CHUNK_BYTES as u64) as u32
}

/// RAII lease owns a private spill and the global session permit. Every error, cancel,
/// connection close and completed decode removes only this session's file.
pub(crate) struct SnapshotSpill {
    file: File,
    path: PathBuf,
    _permit: OwnedSemaphorePermit,
    created: Instant,
    native_resource_lease: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
    /// G14 capacity lane: the spill's disk reservation (the policy maximum, 384 MiB) under
    /// `--storage-cap-bytes`; released with the session. `None` on an uncapped node.
    _storage: Option<kaspa_utils::storage_budget::StorageReservation>,
}
impl SnapshotSpill {
    pub(crate) fn new() -> io::Result<Self> {
        Self::with_budget(session_budget())
    }
    pub(crate) fn hold_native_resource_lease(
        &mut self,
        lease: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
    ) {
        self.native_resource_lease = lease;
    }
    fn with_budget(budget: Arc<Semaphore>) -> io::Result<Self> {
        let permit = budget
            .try_acquire_owned()
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "PALW snapshot session budget is busy"))?;
        // Reserve the disk before the file exists (after reclaiming obsolete temp/log data), so two
        // sessions never count the same free bytes. A refusal is local capacity, not a peer fault.
        let storage = kaspa_utils::storage_budget::reserve_global_after_reclaim(
            kaspa_utils::storage_budget::StorageCategory::IbdSpill,
            crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64,
        )
        .map_err(|refusal| io::Error::new(io::ErrorKind::WouldBlock, refusal.to_string()))?;
        // Under a storage budget the spill lives in the managed root's temp dir (counted, reclaimed);
        // otherwise in the OS temp dir as before.
        let path =
            kaspa_utils::storage_budget::managed_temp_dir().join(format!("misaka-palw-snapshot-{}.spill", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // The OS removes this private file when the last handle closes, including a crash.
            options.custom_flags(0x04000000); // FILE_FLAG_DELETE_ON_CLOSE
        }
        let file = options.open(&path)?;
        #[cfg(unix)]
        std::fs::remove_file(&path)?; // Keep only the FD: process death cannot orphan a 384 MiB spill.
        Ok(Self { file, path, _permit: permit, created: Instant::now(), native_resource_lease: None, _storage: storage })
    }
    pub(crate) fn expired(&self) -> bool {
        self.created.elapsed() >= SESSION_TTL
    }
}
impl Drop for SnapshotSpill {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Bound preflight/serialization CPU to the lease as well as continuation I/O.
/// Check after at most one fixed input buffer, avoiding a clock syscall per
/// Borsh scalar while a 100k native state emits millions of fields.
struct LeaseDeadlineWriter<'a, W> {
    inner: &'a mut W,
    created: Instant,
    checked_bytes: usize,
}
impl<W: Write> Write for LeaseDeadlineWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.checked_bytes == 0 || self.checked_bytes >= COMPRESSION_INPUT_BYTES {
            if self.created.elapsed() > SESSION_TTL {
                return Err(invalid("snapshot serialization lease expired"));
            }
            self.checked_bytes = 1;
        }
        let count = self.inner.write(bytes)?;
        self.checked_bytes = self.checked_bytes.saturating_add(count);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.created.elapsed() > SESSION_TTL {
            return Err(invalid("snapshot serialization lease expired"));
        }
        self.inner.flush()
    }
}

struct BoundedHashWriter<'a> {
    file: &'a mut dyn Write,
    hash: MerkleBranchHash64,
    total: u64,
}
impl Write for BoundedHashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self.total.checked_add(bytes.len() as u64).ok_or_else(|| invalid("snapshot byte overflow"))?;
        if next > crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64 {
            return Err(invalid("snapshot exceeds existing total byte cap"));
        }
        let n = self.file.write(bytes)?;
        self.hash.write(&bytes[..n]);
        self.total += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

const SNAPSHOT_FORMAT: u16 = 1;
pub(crate) type Snapshot = (PalwStateCarriageV2, Vec<(Hash64, Vec<u8>)>);

/// One size policy for chunked and legacy snapshots. Count the canonical envelope's
/// version/state-length/count (14 bytes) and each declaration's class id/length (68
/// bytes), even when the legacy protobuf carries those fields separately. Otherwise
/// an accepted legacy snapshot can deterministically exceed the chunked peer's cap.
pub(crate) fn snapshot_payload_size(state_bytes: u64, declarations: impl IntoIterator<Item = u64>) -> io::Result<u64> {
    snapshot_payload_size_with_cap(state_bytes, declarations, crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64)
}
fn snapshot_payload_size_with_cap(state_bytes: u64, declarations: impl IntoIterator<Item = u64>, cap: u64) -> io::Result<u64> {
    if state_bytes == 0 {
        return Err(invalid("empty snapshot state"));
    }
    let mut total = state_bytes.checked_add(14).ok_or_else(|| invalid("snapshot length overflow"))?;
    let mut count = 0u32;
    for bytes in declarations {
        if bytes > u32::MAX as u64 {
            return Err(invalid("snapshot declaration exceeds Borsh length"));
        }
        count = count.checked_add(1).ok_or_else(|| invalid("snapshot declaration count overflow"))?;
        total = total.checked_add(68).and_then(|n| n.checked_add(bytes)).ok_or_else(|| invalid("snapshot length overflow"))?;
        if total > cap {
            return Err(invalid("snapshot exceeds existing total byte cap"));
        }
    }
    if total > cap {
        return Err(invalid("snapshot exceeds existing total byte cap"));
    }
    Ok(total)
}

/// Preflight canonical serialization without allocating the encoded state. The
/// existing bounded SizeWriter also stops oversized carriages during their walk.
pub(crate) fn snapshot_encoded_size(carriage: &PalwStateCarriageV2, declarations: &[(Hash64, Vec<u8>)]) -> io::Result<u64> {
    let state_bytes = crate::snapshot_memory::encoded_size(carriage, crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64)?;
    snapshot_payload_size(state_bytes, declarations.iter().map(|(_, bytes)| bytes.len() as u64))
}

pub(crate) fn encode_legacy_snapshot(carriage: &PalwStateCarriageV2, declarations: &[(Hash64, Vec<u8>)]) -> io::Result<Vec<u8>> {
    let state_bytes = crate::snapshot_memory::encoded_size(carriage, crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64)?;
    snapshot_payload_size(state_bytes, declarations.iter().map(|(_, bytes)| bytes.len() as u64))?;
    crate::snapshot_memory::encode_exact(carriage, state_bytes)
}

// PalwStateCarriageV2 reads its optional tagged tails to EOF. A direct tuple is
// undecodable: the declaration count is mistaken for another tail. This envelope
// gives that decoder an exact EOF without allocating another whole state Vec.
struct SnapshotEnvelope<'a> {
    carriage: &'a PalwStateCarriageV2,
    declarations: &'a [(Hash64, Vec<u8>)],
}
struct SizeWriter(u64, u64);
impl Write for SizeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self.0.checked_add(bytes.len() as u64).ok_or_else(|| invalid("snapshot length overflow"))?;
        if self.0 > self.1 {
            return Err(invalid("snapshot exceeds existing total byte cap"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl BorshSerialize for SnapshotEnvelope<'_> {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        serialize_envelope(self.carriage, self.declarations, writer, crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64)
    }
}
struct CompressedSnapshotEnvelope<'a> {
    snapshot: SnapshotEnvelope<'a>,
    state_bytes: u64,
    decoded_bytes: u64,
}
impl BorshSerialize for CompressedSnapshotEnvelope<'_> {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        // The caller preflights this immutable borrowed native snapshot once.
        // Check the actual stream against that exact count without walking the
        // 100k state again just to emit its already-known frame header.
        let mut exact = ExactCanonicalWriter { writer, remaining: self.decoded_bytes };
        SNAPSHOT_FORMAT.serialize(&mut exact)?;
        self.state_bytes.serialize(&mut exact)?;
        self.snapshot.carriage.serialize(&mut exact)?;
        self.snapshot.declarations.serialize(&mut exact)?;
        if exact.remaining != 0 {
            return Err(invalid("canonical snapshot differs from its preflight count"));
        }
        Ok(())
    }
}
struct ExactCanonicalWriter<'a, W> {
    writer: &'a mut W,
    remaining: u64,
}
impl<W: Write> Write for ExactCanonicalWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(invalid("canonical snapshot exceeds its preflight count"));
        }
        let written = self.writer.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
fn serialize_envelope<W: Write>(
    carriage: &PalwStateCarriageV2,
    declarations: &[(Hash64, Vec<u8>)],
    writer: &mut W,
    cap: u64,
) -> io::Result<()> {
    let mut size = SizeWriter(0, cap);
    carriage.serialize(&mut size)?;
    snapshot_payload_size_with_cap(size.0, declarations.iter().map(|(_, bytes)| bytes.len() as u64), cap)?;
    SNAPSHOT_FORMAT.serialize(writer)?;
    size.0.serialize(writer)?;
    carriage.serialize(writer)?;
    declarations.serialize(writer)
}

// Isolated legacy-envelope boundary tests; negotiated109 uses its streaming decoder.
#[cfg(test)]
fn decode_row_snapshot_with_admission_v1(
    point: Hash64,
    bytes: &[u8],
    enabled: bool,
    reserve: impl FnOnce(
        Hash64,
        &kaspa_consensus_core::palw_native_resource_v1::NativeWirePreflightV1,
    ) -> Result<
        Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
        kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1,
    >,
) -> Result<(Snapshot, Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>), NativeSnapshotDecodeErrorV1> {
    if bytes.is_empty() || bytes.len() > crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES {
        return Err(invalid("row snapshot exceeds existing total byte cap").into());
    }
    // Structural preflight is mandatory even on an uncapped node. A hostile suffix
    // has not passed the header-root import gate yet and must not drive Borsh allocation.
    let shape = kaspa_consensus_core::palw_native_resource_v1::preflight_native_snapshot_v1(bytes, bytes.len() as u64)?;
    let lease = if enabled { reserve(point, &shape).map_err(NativeSnapshotDecodeErrorV1::Capacity)? } else { None };
    let snapshot = decode_snapshot(&mut io::Cursor::new(bytes), bytes.len() as u64, true)?;
    Ok((snapshot, lease))
}

fn decode_snapshot(reader: &mut impl Read, total: u64, complete_material: bool) -> io::Result<Snapshot> {
    if u16::deserialize_reader(reader)? != SNAPSHOT_FORMAT {
        return Err(invalid("unknown snapshot framing version"));
    }
    let state_bytes = u64::deserialize_reader(reader)?;
    if state_bytes == 0 || state_bytes > total.saturating_sub(14) {
        return Err(invalid("snapshot state frame exceeds stream"));
    }
    let mut state_frame = reader.take(state_bytes);
    let carriage = PalwStateCarriageV2::deserialize_reader(&mut state_frame)?;
    if state_frame.limit() != 0 {
        return Err(invalid("snapshot state frame ended early"));
    }
    let reader = state_frame.into_inner();
    let mut remaining = total - 10 - state_bytes;
    let offered = u32::deserialize_reader(reader)? as u64;
    remaining -= 4;
    // Every entry costs at least a 64-byte class id and 4-byte byte length. Check
    // count before Borsh could preallocate a Vec from an untrusted advertised length.
    if offered > remaining / 68 {
        return Err(invalid("snapshot declaration count exceeds remaining stream"));
    }
    let registered: BTreeSet<Hash64> = carriage.classes.keys().copied().collect();
    let mut seen = BTreeSet::new();
    let mut declarations = Vec::new();
    for _ in 0..offered {
        if remaining < 68 {
            return Err(invalid("snapshot declaration header truncated"));
        }
        let class = Hash64::deserialize_reader(reader)?;
        let bytes = u32::deserialize_reader(reader)? as u64;
        remaining -= 68;
        if bytes > remaining {
            return Err(invalid("snapshot declaration length exceeds remaining stream"));
        }
        // Unknown and repeated class declarations keep the legacy filtering semantics,
        // but are discarded from the reader without first creating attacker-sized Vecs.
        if registered.contains(&class) && seen.insert(class) {
            let mut data = vec![0; bytes as usize];
            reader.read_exact(&mut data)?;
            declarations.push((class, data));
        } else {
            if complete_material {
                return Err(invalid("complete snapshot has unknown or duplicate declaration"));
            }
            if io::copy(&mut (&mut *reader).take(bytes), &mut io::sink())? != bytes {
                return Err(invalid("snapshot declaration truncated"));
            }
        }
        remaining -= bytes;
    }
    if remaining != 0 {
        return Err(invalid("snapshot has trailing declaration bytes"));
    }
    Ok((carriage, declarations))
}

// Bridge Borsh's push serializer to the row exporter's pull suffix without
// reserving a second whole-generation spill. At most one queued64KiB piece,
// one writer piece, one reader piece and one serialization BufWriter exist
// concurrently, beside the generation output BufWriter. Reserve this scratch
// and the explicitly bounded worker stack before spawning.
// Dropping the reader on an early export refusal unblocks and joins the writer.
struct SnapshotSuffixWriterV1(std::sync::mpsc::SyncSender<Vec<u8>>);
impl Write for SnapshotSuffixWriterV1 {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for piece in bytes.chunks(COMPRESSION_INPUT_BYTES) {
            self.0.send(piece.to_vec()).map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "row suffix reader closed"))?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct SnapshotSuffixReaderV1 {
    incoming: std::sync::mpsc::Receiver<Vec<u8>>,
    piece: io::Cursor<Vec<u8>>,
}
impl Read for SnapshotSuffixReaderV1 {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        loop {
            let n = self.piece.read(bytes)?;
            if n != 0 {
                return Ok(n);
            }
            self.piece = io::Cursor::new(Vec::new());
            match self.incoming.recv() {
                Ok(piece) => self.piece = io::Cursor::new(piece),
                Err(_) => return Ok(0),
            }
        }
    }
}
fn with_snapshot_suffix_reader_v1<T>(
    carriage: &PalwStateCarriageV2,
    declarations: &[(Hash64, Vec<u8>)],
    consume: impl FnOnce(&mut dyn Read, u64) -> io::Result<T>,
) -> io::Result<T> {
    let suffix_len = snapshot_encoded_size(carriage, declarations)?;
    const STACK_BYTES: usize = 2 << 20;
    let _scratch = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(
        (5 * COMPRESSION_INPUT_BYTES + STACK_BYTES + 4096 + std::mem::size_of::<SnapshotSuffixReaderV1>()) as u64,
    )
    .map_err(|why| io::Error::new(io::ErrorKind::OutOfMemory, why.to_string()))?;
    std::thread::scope(|scope| {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let writer =
            std::thread::Builder::new().name("palw-row-suffix".into()).stack_size(STACK_BYTES).spawn_scoped(scope, move || {
                let mut output = BufWriter::with_capacity(COMPRESSION_INPUT_BYTES, SnapshotSuffixWriterV1(send));
                SnapshotEnvelope { carriage, declarations }.serialize(&mut output)?;
                output.flush()
            })?;
        let mut reader = SnapshotSuffixReaderV1 { incoming: receive, piece: io::Cursor::new(Vec::new()) };
        let consumed = consume(&mut reader, suffix_len);
        drop(reader); // release a blocked sender even if consume refused at its first byte
        let serialized = writer.join().map_err(|_| io::Error::other("row suffix serialization worker stopped"))?;
        match consumed {
            Err(why) => Err(why),
            Ok(value) => serialized.map(|_| value),
        }
    })
}

pub(crate) struct SnapshotSender {
    spill: SnapshotSpill,
    pub point: Hash64,
    pub root: Hash64,
    pub total: u64,
    pub count: u32,
    pub profile: Option<SnapshotCompressionV1>,
    next: u32,
}

impl SnapshotSender {
    /// Called on the blocking pool AFTER acquiring the global session lease and before
    /// materializing the native snapshot. Writes Borsh directly; no whole encoded Vec.
    pub(crate) fn encode<T: BorshSerialize>(spill: SnapshotSpill, point: Hash64, value: &T) -> io::Result<Self> {
        Self::encode_profile(spill, point, value, None)
    }
    fn encode_profile<T: BorshSerialize>(
        mut spill: SnapshotSpill,
        point: Hash64,
        value: &T,
        profile: Option<SnapshotCompressionV1>,
    ) -> io::Result<Self> {
        let mut buffered = BufWriter::with_capacity(CHUNK_BYTES, &mut spill.file);
        let mut writer = BoundedHashWriter { file: &mut buffered, hash: stream_hasher_profile(point, profile), total: 0 };
        if profile.is_some_and(|p| p.codec == SNAPSHOT_CODEC_ZLIB_V1) {
            let mut encoder = flate2::write::ZlibEncoder::new(&mut writer, flate2::Compression::fast());
            // Borsh emits millions of small fields for a native 100k snapshot.
            // Aggregate them before deflate, using fixed bounded scratch. Do
            // not call ZlibEncoder::flush: it inserts a compressed sync marker.
            let mut input = BufWriter::with_capacity(COMPRESSION_INPUT_BYTES, &mut encoder);
            borsh::to_writer(&mut LeaseDeadlineWriter { inner: &mut input, created: spill.created, checked_bytes: 0 }, value)?;
            input.into_inner().map_err(|e| e.into_error())?;
            encoder.finish()?;
        } else {
            borsh::to_writer(&mut writer, value)?;
        }
        writer.flush()?;
        let total = writer.total;
        let root = writer.hash.finalize();
        drop(buffered);
        if spill.expired() {
            return Err(invalid("snapshot serialization lease expired"));
        }
        if total == 0 {
            return Err(invalid("empty snapshot stream"));
        }
        Ok(Self { spill, point, root, total, count: chunk_count(total), next: 0, profile })
    }
    pub(crate) fn encode_snapshot(
        spill: SnapshotSpill,
        point: Hash64,
        carriage: &PalwStateCarriageV2,
        declarations: &[(Hash64, Vec<u8>)],
    ) -> io::Result<Self> {
        Self::encode(spill, point, &SnapshotEnvelope { carriage, declarations })
    }
    pub(crate) fn encode_snapshot_compressed(
        spill: SnapshotSpill,
        point: Hash64,
        carriage: &PalwStateCarriageV2,
        declarations: &[(Hash64, Vec<u8>)],
        nonce: [u8; 32],
    ) -> io::Result<Self> {
        let mut size = SizeWriter(0, DECODED_SNAPSHOT_MAX_BYTES);
        carriage.serialize(&mut LeaseDeadlineWriter { inner: &mut size, created: spill.created, checked_bytes: 0 })?;
        let decoded = snapshot_payload_size_with_cap(
            size.0,
            declarations.iter().map(|(_, bytes)| bytes.len() as u64),
            DECODED_SNAPSHOT_MAX_BYTES,
        )?;
        let profile = SnapshotCompressionV1::from_wire(SNAPSHOT_CODEC_ZLIB_V1, &nonce, decoded)?.unwrap();
        Self::encode_profile(
            spill,
            point,
            &CompressedSnapshotEnvelope {
                snapshot: SnapshotEnvelope { carriage, declarations },
                state_bytes: size.0,
                decoded_bytes: decoded,
            },
            Some(profile),
        )
    }
    pub(crate) fn encode_row_snapshot(
        mut spill: SnapshotSpill,
        point: Hash64,
        carriage: &PalwStateCarriageV2,
        declarations: &[(Hash64, Vec<u8>)],
        nonce: [u8; 32],
        api: &dyn kaspa_consensus_core::api::ConsensusApi,
    ) -> io::Result<Self> {
        let total = with_snapshot_suffix_reader_v1(carriage, declarations, |suffix, suffix_len| {
            let mut buffer = BufWriter::with_capacity(COMPRESSION_INPUT_BYTES, &mut spill.file);
            let mut bounded = BoundedHashWriter { file: &mut buffer, hash: stream_hasher(point), total: 0 };
            let written = api
                .write_pruning_point_palw_row_generation_v1(point, suffix, suffix_len, &mut bounded)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "pruning snapshot changed before row export"))?;
            bounded.flush()?;
            if written != bounded.total {
                return Err(invalid("row export byte count differs"));
            }
            Ok(written)
        })?;
        let profile = SnapshotCompressionV1::from_wire(SNAPSHOT_CODEC_ROWS_V1, &nonce, total)?.unwrap();
        spill.file.seek(SeekFrom::Start(0))?;
        let _lease = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(COMPRESSION_INPUT_BYTES as u64)
            .map_err(|e| io::Error::new(io::ErrorKind::OutOfMemory, e.to_string()))?;
        let mut hash = stream_hasher_profile(point, Some(profile));
        let mut bytes = [0u8; COMPRESSION_INPUT_BYTES];
        loop {
            let n = spill.file.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            hash.write(&bytes[..n]);
        }
        if spill.expired() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "row export lease expired"));
        }
        Ok(Self { spill, point, root: hash.finalize(), total, count: chunk_count(total), next: 0, profile: Some(profile) })
    }
    /// Shared production chunk encoder, also exercised over the real P2P router.
    pub(crate) fn reply_message(
        &mut self,
        point: Hash64,
        root: Option<Hash64>,
        index: u32,
    ) -> Result<
        (kaspa_p2p_lib::pb::PruningPointPalwStateMessage, Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>),
        NativeSnapshotDecodeErrorV1,
    > {
        self.check_chunk_request_v1(point, root, index)?;
        let bytes_len = (self.total - index as u64 * CHUNK_BYTES as u64).min(CHUNK_BYTES as u64);
        let lease = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(
            bytes_len + std::mem::size_of::<Vec<u8>>() as u64,
        )
        .map_err(NativeSnapshotDecodeErrorV1::Capacity)?;
        let bytes = self.chunk(point, root, index)?;
        Ok((
            kaspa_p2p_lib::pb::PruningPointPalwStateMessage {
                found: true,
                chunked: true,
                pruning_point_hash: Some(point.into()),
                snapshot_root: self.root.as_byte_slice().to_vec(),
                chunk_index: index,
                chunk_count: self.count,
                total_bytes: self.total,
                chunk_hash: chunk_hash_profile(point, self.root, self.total, self.count, index, &bytes, self.profile)
                    .as_byte_slice()
                    .to_vec(),
                palw_state: bytes,
                snapshot_codec: self.profile.map_or(0, |p| p.codec),
                decoded_bytes: self.profile.map_or(0, |p| p.decoded),
                snapshot_nonce: self.profile.map_or_else(Vec::new, |p| p.nonce.to_vec()),
                ..Default::default()
            },
            lease,
        ))
    }
    pub(crate) fn expired(&self) -> bool {
        self.spill.expired()
    }
    pub(crate) fn next_index(&self) -> u32 {
        self.next
    }
    /// A new peer may resume only an independently reconstructed identical stream.
    /// The server has already prepaid its full reconstruction/traversal; the root
    /// pins every byte and no resumed session may rewind after this initialization.
    pub(crate) fn resume(&mut self, point: Hash64, root: Hash64, index: u32) -> io::Result<()> {
        if self.expired() || self.next != 0 || point != self.point || root != self.root || index == 0 || index >= self.count {
            return Err(invalid("snapshot resume point/root/index differs"));
        }
        self.next = index;
        Ok(())
    }
    fn check_chunk_request_v1(&self, point: Hash64, root: Option<Hash64>, index: u32) -> Result<(), NativeSnapshotDecodeErrorV1> {
        if self.expired() {
            return Err(NativeSnapshotDecodeErrorV1::LocalStorage(io::Error::new(
                io::ErrorKind::TimedOut,
                "snapshot sender lease expired",
            )));
        }
        if point != self.point
            || index != self.next
            || index >= self.count
            || (index == 0 && root.is_some())
            || (index > 0 && root != Some(self.root))
        {
            return Err(invalid("snapshot point/root/order/session changed").into());
        }
        Ok(())
    }
    pub(crate) fn chunk(&mut self, point: Hash64, root: Option<Hash64>, index: u32) -> Result<Vec<u8>, NativeSnapshotDecodeErrorV1> {
        self.check_chunk_request_v1(point, root, index)?;
        let offset = index as u64 * CHUNK_BYTES as u64;
        let mut bytes = vec![0; (self.total - offset).min(CHUNK_BYTES as u64) as usize];
        self.spill.file.seek(SeekFrom::Start(offset)).map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
        self.spill.file.read_exact(&mut bytes).map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
        self.next += 1;
        Ok(bytes)
    }
}

pub(crate) struct SnapshotReceiver {
    row_progress: Option<kaspa_consensus_core::palw_row_stream_v1::PalwRowGenerationProgressV1>,
    spill: SnapshotSpill,
    point: Hash64,
    root: Hash64,
    total: u64,
    count: u32,
    next: u32,
    received: u64,
    hash: MerkleBranchHash64,
    profile: Option<SnapshotCompressionV1>,
}
impl SnapshotReceiver {
    pub(crate) fn new(spill: SnapshotSpill, point: Hash64, root: Hash64, total: u64, count: u32) -> io::Result<Self> {
        if total == 0 || total > crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64 || count != chunk_count(total) {
            return Err(invalid("snapshot total/count exceeds policy"));
        }
        Ok(Self {
            spill,
            point,
            root,
            total,
            count,
            next: 0,
            received: 0,
            hash: stream_hasher(point),
            profile: None,
            row_progress: None,
        })
    }
    pub(crate) fn new_compressed(
        spill: SnapshotSpill,
        point: Hash64,
        root: Hash64,
        total: u64,
        count: u32,
        profile: SnapshotCompressionV1,
    ) -> io::Result<Self> {
        SnapshotCompressionV1::from_wire(profile.codec, &profile.nonce, profile.decoded)?;
        if profile.codec == SNAPSHOT_CODEC_ROWS_V1 && profile.decoded != total {
            return Err(invalid("row generation total/profile length differs"));
        }
        let mut state = Self::new(spill, point, root, total, count)?;
        state.profile = Some(profile);
        state.hash = stream_hasher_profile(point, Some(profile));
        Ok(state)
    }
    pub(crate) fn row_expected_root(&self) -> Option<Hash64> {
        self.row_progress.as_ref().map(|p| p.expected_root())
    }
    pub(crate) fn enable_row_verification(&mut self, expected: Hash64) -> Result<(), NativeSnapshotDecodeErrorV1> {
        if self.profile.is_none_or(|p| p.codec != SNAPSHOT_CODEC_ROWS_V1) || self.received != 0 {
            return Err(invalid("row verification initialized outside a new row stream").into());
        }
        self.row_progress = Some(
            kaspa_consensus_core::palw_row_stream_v1::PalwRowGenerationProgressV1::new(expected, self.total)
                .map_err(row_failure_v1)?,
        );
        Ok(())
    }
    pub(crate) fn profile(&self) -> Option<SnapshotCompressionV1> {
        self.profile
    }
    pub(crate) fn accept(
        &mut self,
        point: Hash64,
        root: Hash64,
        total: u64,
        count: u32,
        index: u32,
        hash: Hash64,
        bytes: &[u8],
    ) -> Result<(), NativeSnapshotDecodeErrorV1> {
        if self.spill.expired() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "snapshot receive lease expired").into());
        }
        if point != self.point
            || root != self.root
            || total != self.total
            || count != self.count
            || index != self.next
            || index >= count
            || bytes.is_empty()
            || bytes.len() > CHUNK_BYTES
            || bytes.len() as u64 != (self.total - self.received).min(CHUNK_BYTES as u64)
            || chunk_hash_profile(point, root, total, count, index, bytes, self.profile) != hash
        {
            return Err(invalid("invalid snapshot chunk identity/order/size/hash").into());
        }
        self.spill.file.seek(SeekFrom::Start(self.received)).map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
        self.spill.file.write_all(bytes).map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
        self.hash.write(bytes);
        self.received += bytes.len() as u64;
        self.next += 1;
        if let Some(progress) = self.row_progress.as_mut() {
            let _scratch = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(COMPRESSION_INPUT_BYTES as u64)
                .map_err(NativeSnapshotDecodeErrorV1::Capacity)?;
            let result = {
                let mut reader = BufReader::with_capacity(COMPRESSION_INPUT_BYTES, &mut self.spill.file);
                progress.advance(&mut reader, self.received)
            };
            if let Err(error) = result {
                if !matches!(
                    &error,
                    kaspa_consensus_core::palw_row_stream_v1::PalwRowStreamErrorV1::NativeResource(_)
                        | kaspa_consensus_core::palw_row_stream_v1::PalwRowStreamErrorV1::Sink(_)
                ) {
                    // An early semantic error is attributable to the peer only
                    // after the spill matches the authenticated received prefix.
                    self.verify_local_prefix_v1()?;
                }
                return Err(row_failure_v1(error));
            }
            if self.received == self.total && progress.suffix().is_none() {
                self.verify_local_prefix_v1()?;
                return Err(invalid("row generation ended before its collection inventory").into());
            }
        }
        Ok(())
    }
    pub(crate) fn point(&self) -> Hash64 {
        self.point
    }
    pub(crate) fn root(&self) -> Hash64 {
        self.root
    }
    pub(crate) fn next_index(&self) -> u32 {
        self.next
    }
    fn remaining_ttl(&self) -> Duration {
        SESSION_TTL.saturating_sub(self.spill.created.elapsed())
    }
    pub(crate) fn complete(&self) -> bool {
        self.next == self.count && self.received == self.total
    }
    #[allow(dead_code)] // the whole-buffer form of decode_with, kept beside it
    pub(crate) fn decode<T: BorshDeserialize>(self) -> io::Result<T> {
        if self.profile.is_some() {
            return Err(invalid("compressed snapshots require the framed native decoder"));
        }
        self.decode_with(|reader, _| T::deserialize_reader(reader))
    }
    /// Verify the actual owned file against the already checked network stream.
    /// A disk write/read fault must not turn valid peer bytes into a bad proof.
    fn verify_local_spill_v1(&mut self) -> Result<(), NativeSnapshotDecodeErrorV1> {
        if !self.complete() || self.hash.clone().finalize() != self.root {
            return Err(invalid("snapshot EOF/stream hash mismatch").into());
        }
        self.verify_local_prefix_v1()
    }
    fn verify_local_prefix_v1(&mut self) -> Result<(), NativeSnapshotDecodeErrorV1> {
        let _buffer_lease = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(
            (COMPRESSION_INPUT_BYTES + std::mem::size_of::<MerkleBranchHash64>()) as u64,
        )
        .map_err(NativeSnapshotDecodeErrorV1::Capacity)?;
        self.spill.file.flush().map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
        self.spill.file.seek(SeekFrom::Start(0)).map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
        let mut hash = stream_hasher_profile(self.point, self.profile);
        let mut bytes = [0u8; COMPRESSION_INPUT_BYTES];
        let mut total = 0u64;
        loop {
            let count = self.spill.file.read(&mut bytes).map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .ok_or_else(|| NativeSnapshotDecodeErrorV1::LocalStorage(invalid("local spill size overflow")))?;
            if total > self.received {
                return Err(NativeSnapshotDecodeErrorV1::LocalStorage(invalid("local spill grew after authenticated transfer")));
            }
            hash.write(&bytes[..count]);
        }
        if total != self.received || hash.finalize() != self.hash.clone().finalize() {
            return Err(NativeSnapshotDecodeErrorV1::LocalStorage(invalid("local spill differs from authenticated transfer bytes")));
        }
        Ok(())
    }
    pub(crate) fn decode_row_generation_with_native_lease(
        self,
        expected_root: Hash64,
    ) -> Result<
        (
            Snapshot,
            Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
            kaspa_consensus_core::palw_row_stream_v1::PalwRowInventoryV1,
        ),
        NativeSnapshotDecodeErrorV1,
    > {
        use kaspa_consensus_core::palw_native_resource_v1::*;
        self.decode_row_generation_with_admission_v1(
            expected_root,
            native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1(),
            reserve_native_wire_v1,
        )
    }
    fn decode_row_generation_with_admission_v1(
        mut self,
        expected_root: Hash64,
        enabled: bool,
        reserve: impl FnOnce(
            Hash64,
            &kaspa_consensus_core::palw_native_resource_v1::NativeWirePreflightV1,
        ) -> Result<
            Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
            kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1,
        >,
    ) -> Result<
        (
            Snapshot,
            Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
            kaspa_consensus_core::palw_row_stream_v1::PalwRowInventoryV1,
        ),
        NativeSnapshotDecodeErrorV1,
    > {
        use kaspa_consensus_core::palw_native_resource_v1::*;
        if !self.profile.is_some_and(|p| p.codec == SNAPSHOT_CODEC_ROWS_V1 && p.decoded == self.total) {
            return Err(invalid("row generation profile absent").into());
        }
        self.verify_local_spill_v1()?;
        self.spill.file.seek(SeekFrom::Start(0)).map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
        let suffix = if let Some(progress) = self.row_progress.take() {
            if progress.expected_root() != expected_root {
                return Err(invalid("row witness changed during transfer").into());
            }
            progress.suffix().ok_or_else(|| invalid("row generation did not finish its verified collection inventory"))?
        } else {
            kaspa_consensus_core::palw_row_stream_v1::palw_verify_row_generation_reader_v1(
                expected_root,
                &mut self.spill.file,
                self.total,
            )
            .map_err(row_failure_v1)?
        };
        self.spill.file.seek(SeekFrom::Start(suffix.offset)).map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
        let _scratch = reserve_native_requests_v1(COMPRESSION_INPUT_BYTES as u64).map_err(NativeSnapshotDecodeErrorV1::Capacity)?;
        let shape = preflight_native_snapshot_v1(
            &mut BufReader::with_capacity(COMPRESSION_INPUT_BYTES, &mut self.spill.file).take(suffix.length),
            suffix.length,
        )?;
        let lease = if enabled { reserve(self.point, &shape).map_err(NativeSnapshotDecodeErrorV1::Capacity)? } else { None };
        self.spill.file.seek(SeekFrom::Start(suffix.offset)).map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
        let snapshot = decode_snapshot(
            &mut BufReader::with_capacity(COMPRESSION_INPUT_BYTES, &mut self.spill.file).take(suffix.length),
            suffix.length,
            true,
        )?;
        if self.spill.expired() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "row import staging lease expired").into());
        }
        Ok((snapshot, lease, suffix.inventory))
    }
    pub(crate) fn decode_strict_raw_snapshot_v1(
        mut self,
    ) -> Result<(Snapshot, Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>), NativeSnapshotDecodeErrorV1>
    {
        use kaspa_consensus_core::palw_native_resource_v1::*;
        self.verify_local_spill_v1()?;
        self.spill.file.seek(SeekFrom::Start(0)).map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
        let preflight = preflight_native_snapshot_v1(BufReader::new(&mut self.spill.file), self.total)?;
        let lease = if native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1() {
            reserve_native_wire_v1(self.point, &preflight).map_err(NativeSnapshotDecodeErrorV1::Capacity)?
        } else {
            None
        };
        let snapshot = self.decode_with(|reader, total| decode_snapshot(reader, total, true))?;
        Ok((snapshot, lease))
    }

    pub(crate) fn decode_snapshot_with_native_lease(
        self,
    ) -> Result<(Snapshot, Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>), NativeSnapshotDecodeErrorV1>
    {
        use kaspa_consensus_core::palw_native_resource_v1::*;
        self.decode_snapshot_with_admission_v1(
            native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1(),
            reserve_native_wire_v1,
        )
    }
    fn decode_snapshot_with_admission_v1(
        mut self,
        enabled: bool,
        reserve: impl FnOnce(
            Hash64,
            &kaspa_consensus_core::palw_native_resource_v1::NativeWirePreflightV1,
        ) -> Result<
            Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
            kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1,
        >,
    ) -> Result<(Snapshot, Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>), NativeSnapshotDecodeErrorV1>
    {
        use kaspa_consensus_core::palw_native_resource_v1::*;
        self.verify_local_spill_v1()?;
        let lease = if enabled {
            if !self.complete() || self.hash.clone().finalize() != self.root {
                return Err(invalid("native preflight stream hash/EOF mismatch").into());
            }
            let profile = self
                .profile
                .ok_or_else(|| invalid("explicit native receive admission requires negotiated protocol 107 compressed profile"))?;
            self.spill.file.flush().map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
            self.spill.file.seek(SeekFrom::Start(0)).map_err(NativeSnapshotDecodeErrorV1::LocalStorage)?;
            let decoder = flate2::bufread::ZlibDecoder::new(BufReader::new(&mut self.spill.file));
            let mut reader =
                DecodedLimitReader { inner: decoder, expected: profile.decoded, read: 0, created: self.spill.created, checked_at: 0 };
            let preflight = preflight_native_snapshot_v1(&mut reader, profile.decoded)?;
            let mut extra = [0];
            if reader.read(&mut extra)? != 0 || reader.read != profile.decoded || reader.inner.total_in() != self.total {
                return Err(invalid("native preflight CRC/single-stream/exact decoded length mismatch").into());
            }
            reserve(self.point, &preflight).map_err(NativeSnapshotDecodeErrorV1::Capacity)?
        } else {
            None
        };
        let snapshot = self.decode_snapshot()?;
        Ok((snapshot, lease))
    }
    pub(crate) fn decode_snapshot(self) -> io::Result<Snapshot> {
        if self.profile.is_some_and(|p| p.codec == SNAPSHOT_CODEC_ROWS_V1) {
            return Err(invalid("row generation requires the selected-child root verifier"));
        }
        if self.profile.is_some() {
            return self.decode_compressed_snapshot();
        }
        self.decode_with(|reader, total| decode_snapshot(reader, total, false))
    }
    fn decode_compressed_snapshot(mut self) -> io::Result<Snapshot> {
        if !self.complete() || self.hash.clone().finalize() != self.root {
            return Err(invalid("snapshot EOF/stream hash mismatch"));
        }
        let profile = self.profile.unwrap();
        self.spill.file.flush()?;
        self.spill.file.seek(SeekFrom::Start(0))?;
        let decoder = flate2::bufread::ZlibDecoder::new(BufReader::new(&mut self.spill.file));
        let mut reader =
            DecodedLimitReader { inner: decoder, expected: profile.decoded, read: 0, created: self.spill.created, checked_at: 0 };
        let value = decode_snapshot(&mut reader, profile.decoded, true)?;
        let mut extra = [0];
        if reader.read(&mut extra)? != 0 || reader.read != profile.decoded || reader.inner.total_in() != self.total {
            return Err(invalid("compressed snapshot canonical length/EOF/single stream differs"));
        }
        Ok(value)
    }
    fn decode_with<T>(mut self, decode: impl FnOnce(&mut BufReader<&mut File>, u64) -> io::Result<T>) -> io::Result<T> {
        if !self.complete() || self.hash.clone().finalize() != self.root {
            return Err(invalid("snapshot EOF/stream hash mismatch"));
        }
        self.spill.file.flush()?;
        self.spill.file.seek(SeekFrom::Start(0))?;
        let mut reader = BufReader::new(&mut self.spill.file);
        let value = decode(&mut reader, self.total)?;
        let mut extra = [0];
        if reader.read(&mut extra)? != 0 {
            return Err(invalid("snapshot has trailing bytes"));
        }
        Ok(value)
    }
}

struct DecodedLimitReader<R> {
    inner: R,
    expected: u64,
    read: u64,
    created: Instant,
    checked_at: u64,
}
impl<R: Read> Read for DecodedLimitReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.checked_at == 0 || self.read.saturating_sub(self.checked_at) >= COMPRESSION_INPUT_BYTES as u64 {
            if self.created.elapsed() >= SESSION_TTL {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "snapshot decode lease expired"));
            }
            self.checked_at = self.read.max(1);
        }
        // At most one byte beyond the declared cap is inspected to distinguish
        // exact EOF from a compression bomb. Never emit that byte to Borsh.
        let allowed = self.expected.checked_sub(self.read).ok_or_else(|| invalid("decoded snapshot count overflow"))?;
        let wanted = bytes.len().min(usize::try_from(allowed.saturating_add(1)).unwrap_or(usize::MAX));
        let n = self.inner.read(&mut bytes[..wanted])?;
        if n as u64 > allowed {
            return Err(invalid("compressed snapshot exceeds declared canonical bytes"));
        }
        self.read += n as u64;
        Ok(n)
    }
}

/// Only one interrupted receiver is retained across peer changes, inside the same
/// two-session node budget. No bytes become native state here. Explicit cancellation,
/// invalid chunks, changed points and expired generations drop their spill immediately.
struct ResumeCache {
    slot: Mutex<Option<(u64, SnapshotReceiver)>>,
    generation: std::sync::atomic::AtomicU64,
}
impl ResumeCache {
    fn new() -> Self {
        Self { slot: Mutex::new(None), generation: std::sync::atomic::AtomicU64::new(0) }
    }
    fn take(&self, point: Hash64) -> Option<SnapshotReceiver> {
        self.slot.lock().unwrap().take().and_then(|(_, receiver)| {
            (receiver.point() == point && !receiver.spill.expired() && !receiver.complete()).then_some(receiver)
        })
    }
    fn put(&self, receiver: SnapshotReceiver) -> Option<(u64, Duration)> {
        if receiver.spill.expired() || receiver.complete() || receiver.next == 0 {
            return None;
        }
        let ttl = receiver.remaining_ttl();
        let generation = self.generation.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *self.slot.lock().unwrap() = Some((generation, receiver));
        Some((generation, ttl))
    }
    fn expire(&self, generation: u64) {
        let mut slot = self.slot.lock().unwrap();
        if slot.as_ref().is_some_and(|(id, _)| *id == generation) {
            *slot = None;
        }
    }
}
fn resume_cache() -> &'static ResumeCache {
    static CACHE: OnceLock<ResumeCache> = OnceLock::new();
    CACHE.get_or_init(ResumeCache::new)
}
pub(crate) fn take_interrupted_snapshot(point: Hash64) -> Option<SnapshotReceiver> {
    resume_cache().take(point)
}
pub(crate) fn park_interrupted_snapshot(receiver: SnapshotReceiver) {
    if let Some((generation, ttl)) = resume_cache().put(receiver) {
        // Timer owns no spill and never extends its original lifetime. A replaced or
        // already-resumed generation cannot accidentally be deleted by an older timer.
        tokio::spawn(async move {
            tokio::time::sleep(ttl).await;
            resume_cache().expire(generation);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn lease(budget: &Arc<Semaphore>) -> SnapshotSpill {
        SnapshotSpill::with_budget(Arc::clone(budget)).unwrap()
    }
    fn zlib(raw: &[u8]) -> Vec<u8> {
        let mut writer = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        writer.write_all(raw).unwrap();
        writer.finish().unwrap()
    }
    fn receive_compressed_raw(budget: &Arc<Semaphore>, raw: &[u8], profile: SnapshotCompressionV1) -> SnapshotReceiver {
        let mut hash = stream_hasher_profile(h(1), Some(profile));
        hash.write(raw);
        let root = hash.finalize();
        let total = raw.len() as u64;
        let count = chunk_count(total);
        let mut receiver = SnapshotReceiver::new_compressed(lease(budget), h(1), root, total, count, profile).unwrap();
        for (index, bytes) in raw.chunks(CHUNK_BYTES).enumerate() {
            receiver
                .accept(
                    h(1),
                    root,
                    total,
                    count,
                    index as u32,
                    chunk_hash_profile(h(1), root, total, count, index as u32, bytes, Some(profile)),
                    bytes,
                )
                .unwrap();
        }
        receiver
    }
    #[test]
    fn rfc37_suffix_reader_matches_canonical_bytes_and_joins_after_early_refusal() {
        let carriage = native_carriage();
        let declarations = vec![(h(7), vec![9u8; 3 * COMPRESSION_INPUT_BYTES])];
        let expected = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &declarations }).unwrap();
        with_snapshot_suffix_reader_v1(&carriage, &declarations, |reader, length| {
            let mut actual = Vec::new();
            reader.read_to_end(&mut actual)?;
            assert_eq!(length, expected.len() as u64);
            assert_eq!(actual, expected);
            Ok(())
        })
        .unwrap();
        for read_header in [false, true] {
            let result = with_snapshot_suffix_reader_v1(&carriage, &declarations, |reader, _| {
                if read_header {
                    let mut head = [0; 10];
                    reader.read_exact(&mut head)?;
                }
                Err::<(), _>(invalid("source changed before export"))
            });
            assert_eq!(
                result.unwrap_err().to_string(),
                "source changed before export",
                "early refusal closes the bounded channel and joins the producer"
            );
        }
    }

    #[test]
    fn row_snapshot_reserves_typed_shape_before_decode_and_preserves_capacity_failure() {
        use kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1;
        let carriage = native_carriage();
        let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &[] }).unwrap();
        let refused = decode_row_snapshot_with_admission_v1(h(1), &raw, true, |point, shape| {
            assert_eq!(point, h(1));
            assert!(shape.memory_requests_v1().unwrap() > 0);
            Err(NativeCoverageV1::unsupported::<PalwStateCarriageV2>("row test", "local capacity"))
        });
        assert!(matches!(refused, Err(NativeSnapshotDecodeErrorV1::Capacity(_))));
        let ((restored, declarations), _) = decode_row_snapshot_with_admission_v1(h(1), &raw, true, |_, _| Ok(None)).unwrap();
        assert_eq!(borsh::to_vec(&restored).unwrap(), borsh::to_vec(&carriage).unwrap());
        assert!(declarations.is_empty());
        let mut malformed = raw;
        malformed[2..10].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            decode_row_snapshot_with_admission_v1(h(1), &malformed, true, |_, _| panic!(
                "malformed framing must fail before admission"
            )),
            Err(NativeSnapshotDecodeErrorV1::Encoding(_))
        ));
    }

    #[test]
    fn rfc37_authenticated_generation_classifies_suffix_capacity_disk_and_encoding_failures() {
        use kaspa_consensus_core::{palw_native_resource_v1::NativeCoverageV1, palw_state_v2::PalwChainStateV2};
        let state = PalwChainStateV2::genesis();
        let root = state.state_root();
        let carriage = PalwStateCarriageV2::from_state(&state);
        let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &[] }).unwrap();
        let mut wire = Vec::new();
        kaspa_consensus_core::palw_direct_row_export_v1::write_row_generation_stream_v1(
            &state,
            &mut io::Cursor::new(&raw),
            raw.len() as u64,
            &mut wire,
        )
        .unwrap();
        let profile = SnapshotCompressionV1 { codec: SNAPSHOT_CODEC_ROWS_V1, nonce: [53; 32], decoded: wire.len() as u64 };
        let budget = Arc::new(Semaphore::new(1));
        let receiver = receive_compressed_raw(&budget, &wire, profile);
        let admitted = std::cell::Cell::new(false);
        let result = receiver.decode_row_generation_with_admission_v1(root, true, |point, shape| {
            assert_eq!(point, h(1));
            assert!(shape.memory_requests_v1().unwrap() > 0);
            admitted.set(true);
            Err(NativeCoverageV1::unsupported::<PalwStateCarriageV2>("row generation test", "local capacity"))
        });
        assert!(admitted.get());
        assert!(matches!(result, Err(NativeSnapshotDecodeErrorV1::Capacity(_))));
        assert_eq!(budget.available_permits(), 1);

        let receiver = receive_compressed_raw(&budget, &wire, profile);
        receiver.spill.file.set_len(0).unwrap();
        let result = receiver.decode_row_generation_with_admission_v1(root, true, |_, _| panic!("corrupt disk precedes admission"));
        assert!(matches!(result, Err(NativeSnapshotDecodeErrorV1::LocalStorage(_))));
        assert_eq!(budget.available_permits(), 1);

        let parsed = kaspa_consensus_core::palw_row_stream_v1::palw_verify_row_generation_reader_v1(
            root,
            &mut io::Cursor::new(&wire),
            wire.len() as u64,
        )
        .unwrap();
        let at = parsed.offset as usize;
        wire[at + 2..at + 10].copy_from_slice(&u64::MAX.to_le_bytes());
        let receiver = receive_compressed_raw(&budget, &wire, profile);
        let result = receiver.decode_row_generation_with_admission_v1(root, true, |_, _| panic!("bad suffix precedes admission"));
        assert!(matches!(result, Err(NativeSnapshotDecodeErrorV1::Encoding(_))));
        assert_eq!(budget.available_permits(), 1);
    }

    #[test]
    fn rfc37_early_row_failure_checks_the_owned_disk_prefix_before_blaming_peer() {
        use kaspa_consensus_core::{
            palw_row_stream_v1::*,
            palw_state_v2::{palw_collection_root_of_entries_v1, palw_state_root_of_preimage_v1},
        };
        let rows = vec![(vec![1], vec![7u8; 5 << 20])];
        let label = b"disk-prefix-control";
        let root = palw_collection_root_of_entries_v1(label, 1, rows.clone().into_iter());
        let wire = encode_row_stream_generation_v1(
            root.as_byte_slice(),
            &[PalwRowStreamCollectionV1 { label: label.to_vec(), offset: 0, rows }],
            b"unused-suffix",
        )
        .unwrap();
        let profile = SnapshotCompressionV1 { codec: SNAPSHOT_CODEC_ROWS_V1, nonce: [57; 32], decoded: wire.len() as u64 };
        let mut hasher = stream_hasher_profile(h(1), Some(profile));
        hasher.write(&wire);
        let stream = hasher.finalize();
        let budget = Arc::new(Semaphore::new(2));
        let mut receiver =
            SnapshotReceiver::new_compressed(lease(&budget), h(1), stream, wire.len() as u64, chunk_count(wire.len() as u64), profile)
                .unwrap();
        receiver.enable_row_verification(palw_state_root_of_preimage_v1(root.as_byte_slice())).unwrap();
        for (index, bytes) in wire.chunks(CHUNK_BYTES).enumerate() {
            let result = receiver.accept(
                h(1),
                stream,
                wire.len() as u64,
                chunk_count(wire.len() as u64),
                index as u32,
                chunk_hash_profile(
                    h(1),
                    stream,
                    wire.len() as u64,
                    chunk_count(wire.len() as u64),
                    index as u32,
                    bytes,
                    Some(profile),
                ),
                bytes,
            );
            if index == 0 {
                result.unwrap();
                let at = wire.windows(1024).position(|v| v == [7u8; 1024]).unwrap();
                receiver.spill.file.seek(SeekFrom::Start(at as u64)).unwrap();
                receiver.spill.file.write_all(&[8]).unwrap();
            } else {
                assert!(
                    matches!(result, Err(NativeSnapshotDecodeErrorV1::LocalStorage(_))),
                    "disk changed a correct peer's retained row bytes"
                );
            }
        }
    }
    #[test]
    fn row_snapshot_never_filters_unknown_or_duplicate_declarations() {
        let carriage = native_carriage();
        for declarations in [vec![(h(8), vec![1])], vec![(h(7), vec![1]), (h(7), vec![2])]] {
            let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &declarations }).unwrap();
            assert!(decode_row_snapshot_with_admission_v1(h(1), &raw, false, |_, _| panic!("admission disabled")).is_err());
        }
    }

    #[test]
    fn strict_raw_snapshot_preflights_and_rejects_unknown_declarations() {
        let budget = Arc::new(Semaphore::new(1));
        let carriage = native_carriage();
        let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &[] }).unwrap();
        let (snapshot, _) = receive_raw(&budget, h(1), &raw).decode_strict_raw_snapshot_v1().unwrap();
        assert!(snapshot.1.is_empty());
        let declaration = include_bytes!("../../../consensus/core/tests/corpus/t12-daa508-class-admission.borsh").to_vec();
        for declarations in [vec![(h(8), declaration.clone())], vec![(h(7), declaration.clone()), (h(7), declaration)]] {
            let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &declarations }).unwrap();
            assert!(matches!(
                receive_raw(&budget, h(1), &raw).decode_strict_raw_snapshot_v1(),
                Err(NativeSnapshotDecodeErrorV1::Encoding(error)) if error.to_string().contains("unknown or duplicate declaration")
            ));
        }
    }

    #[test]
    fn compressed_native_snapshot_keeps_canonical_state_and_complete_declarations() {
        let budget = Arc::new(Semaphore::new(2));
        let carriage = native_carriage();
        let declarations = vec![(h(7), vec![0x73; 4096])];
        let canonical = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &declarations }).unwrap();
        let mut sender =
            SnapshotSender::encode_snapshot_compressed(lease(&budget), h(1), &carriage, &declarations, [0x31; 32]).unwrap();
        let profile = sender.profile.unwrap();
        assert_eq!(profile.decoded, canonical.len() as u64);
        let mut receiver =
            SnapshotReceiver::new_compressed(lease(&budget), h(1), sender.root, sender.total, sender.count, profile).unwrap();
        for index in 0..sender.count {
            let bytes = sender.chunk(h(1), (index > 0).then_some(sender.root), index).unwrap();
            receiver
                .accept(
                    h(1),
                    sender.root,
                    sender.total,
                    sender.count,
                    index,
                    chunk_hash_profile(h(1), sender.root, sender.total, sender.count, index, &bytes, Some(profile)),
                    &bytes,
                )
                .unwrap();
        }
        let (decoded, filtered) = receiver.decode_snapshot().unwrap();
        assert_eq!(borsh::to_vec(&decoded).unwrap(), borsh::to_vec(&carriage).unwrap());
        assert_eq!(filtered, vec![declarations[0].clone()]);
        drop(sender);
        assert_eq!(budget.available_permits(), 2);
    }
    #[test]
    fn native_capacity_refusal_preserves_cause_and_never_reconstructs_or_installs_snapshot() {
        use kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1;
        let budget = Arc::new(Semaphore::new(1));
        let carriage = native_carriage();
        // No class declaration: the native preflight parses each one as its real admission carriage (16 junk bytes are an
        // encoding error, not a capacity refusal), and this test is about the capacity cause.
        let declarations = Vec::new();
        let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &declarations }).unwrap();
        let profile = SnapshotCompressionV1 { codec: SNAPSHOT_CODEC_ZLIB_V1, nonce: [7; 32], decoded: raw.len() as u64 };
        let failure = NativeCoverageV1::unsupported::<SnapshotReceiver>("actual local request", "quota exhausted");
        let result = receive_compressed_raw(&budget, &zlib(&raw), profile).decode_snapshot_with_admission_v1(true, |point, _| {
            assert_eq!(point, h(1));
            Err(failure.clone())
        });
        assert!(
            matches!(result, Err(NativeSnapshotDecodeErrorV1::Capacity(ref actual)) if actual == &failure),
            "{:?}",
            result.as_ref().err()
        );
        assert_eq!(budget.available_permits(), 1, "refusal returns this receiver's global spill permit");

        // Authenticated frames can still encode invalid Borsh. It fails before
        // quota admission, keeping a malformed stream distinct from local load.
        let malformed = [0xff; 3];
        let profile = SnapshotCompressionV1 { decoded: malformed.len() as u64, ..profile };
        let admitted = std::cell::Cell::new(false);
        let result = receive_compressed_raw(&budget, &zlib(&malformed), profile).decode_snapshot_with_admission_v1(true, |_, _| {
            admitted.set(true);
            Ok(None)
        });
        assert!(matches!(result, Err(NativeSnapshotDecodeErrorV1::Encoding(_))));
        assert!(!admitted.get(), "malformed wire has no native reconstruction permission");
        assert_eq!(budget.available_permits(), 1);
    }

    #[test]
    fn actual_spill_corruption_is_local_and_precedes_native_reconstruction() {
        let budget = Arc::new(Semaphore::new(1));
        let carriage = native_carriage();
        let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &[] }).unwrap();
        let stream = zlib(&raw);
        let profile = SnapshotCompressionV1 { codec: SNAPSHOT_CODEC_ZLIB_V1, nonce: [9; 32], decoded: raw.len() as u64 };
        for truncate in [true, false] {
            let mut receiver = receive_compressed_raw(&budget, &stream, profile);
            if truncate {
                receiver.spill.file.set_len(0).unwrap();
            } else {
                receiver.spill.file.seek(SeekFrom::Start(0)).unwrap();
                receiver.spill.file.write_all(&[stream[0] ^ 1]).unwrap();
            }
            let admitted = std::cell::Cell::new(false);
            let result = receiver.decode_snapshot_with_admission_v1(true, |_, _| {
                admitted.set(true);
                Ok(None)
            });
            assert!(
                matches!(result, Err(NativeSnapshotDecodeErrorV1::LocalStorage(_))),
                "local file failure cannot condemn valid network bytes"
            );
            assert!(!admitted.get());
            assert_eq!(budget.available_permits(), 1);
        }
    }

    #[test]
    fn compressed_complete_material_refuses_duplicate_and_unknown_before_allocation() {
        let budget = Arc::new(Semaphore::new(1));
        let carriage = native_carriage();
        for declarations in [vec![(h(7), vec![1]), (h(7), vec![2])], vec![(h(9), vec![3])]] {
            let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &declarations }).unwrap();
            let profile = SnapshotCompressionV1 { codec: SNAPSHOT_CODEC_ZLIB_V1, nonce: [1; 32], decoded: raw.len() as u64 };
            assert!(receive_compressed_raw(&budget, &zlib(&raw), profile).decode_snapshot().is_err());
            assert_eq!(budget.available_permits(), 1);
        }
    }

    #[test]
    fn expired_authenticated_decode_is_a_local_resource_refusal_and_releases_spill() {
        let budget = Arc::new(Semaphore::new(1));
        let carriage = native_carriage();
        let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &[] }).unwrap();
        let profile = SnapshotCompressionV1 { codec: SNAPSHOT_CODEC_ZLIB_V1, nonce: [9; 32], decoded: raw.len() as u64 };
        let mut receiver = receive_compressed_raw(&budget, &zlib(&raw), profile);
        receiver.spill.created = Instant::now() - SESSION_TTL - Duration::from_secs(1);
        let result = receiver.decode_snapshot_with_admission_v1(true, |_, _| {
            panic!("expired local decoder must not reach native reconstruction admission")
        });
        assert!(matches!(result, Err(NativeSnapshotDecodeErrorV1::Capacity(_))));
        assert_eq!(budget.available_permits(), 1);
        for kind in [io::ErrorKind::Unsupported, io::ErrorKind::OutOfMemory] {
            assert!(matches!(
                NativeSnapshotDecodeErrorV1::from(io::Error::new(kind, "local scanner resource")),
                NativeSnapshotDecodeErrorV1::Capacity(_)
            ));
        }
    }

    #[test]
    fn serialization_deadline_refuses_before_emitting_and_preserves_sink() {
        let mut output = Vec::new();
        let created = Instant::now() - SESSION_TTL - Duration::from_secs(1);
        let mut writer = LeaseDeadlineWriter { inner: &mut output, created, checked_bytes: 0 };
        assert!(writer.write_all(&[1]).is_err());
        assert!(writer.flush().is_err());
        assert!(output.is_empty());
    }

    #[test]
    fn exact_canonical_writer_refuses_overrun_and_forwards_write_failures() {
        let mut bytes = Vec::new();
        let mut exact = ExactCanonicalWriter { writer: &mut bytes, remaining: 3 };
        exact.write_all(&[1, 2, 3]).unwrap();
        assert_eq!(exact.remaining, 0);
        assert!(exact.write_all(&[4]).is_err());
        struct Refuse;
        impl Write for Refuse {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("disk refused"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut refused = Refuse;
        let mut exact = ExactCanonicalWriter { writer: &mut refused, remaining: 3 };
        assert!(exact.write_all(&[1]).is_err());
        assert_eq!(exact.remaining, 3);
    }

    #[test]
    fn buffered_deflate_is_byte_identical_to_field_writes_and_decodes_old_streams() {
        let carriage = native_carriage();
        let declarations = vec![(h(7), vec![0x81; 131073])];
        let envelope = SnapshotEnvelope { carriage: &carriage, declarations: &declarations };
        let canonical = borsh::to_vec(&envelope).unwrap();
        let profile = SnapshotCompressionV1 { codec: SNAPSHOT_CODEC_ZLIB_V1, nonce: [1; 32], decoded: canonical.len() as u64 };
        let mut old = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        envelope.serialize(&mut old).unwrap();
        let old = old.finish().unwrap();
        let mut new = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut input = BufWriter::with_capacity(COMPRESSION_INPUT_BYTES, &mut new);
        envelope.serialize(&mut input).unwrap();
        input.into_inner().map_err(|e| e.into_error()).unwrap();
        let new = new.finish().unwrap();
        assert_eq!(old, new, "resume roots require identical compressed bytes");
        let budget = Arc::new(Semaphore::new(1));
        for stream in [&old, &new] {
            let (decoded, entries) = receive_compressed_raw(&budget, stream, profile).decode_snapshot().unwrap();
            assert_eq!(decoded, carriage);
            assert_eq!(entries, declarations);
        }
    }

    #[test]
    fn compression_negotiation_nonce_codec_and_separate_caps_refuse_downgrades() {
        assert_eq!(compression_request_nonce(106, 0, &[]).unwrap(), None);
        assert_eq!(compression_request_nonce(107, 0, &[]).unwrap(), None);
        assert_eq!(compression_request_nonce(107, 1, &[1; 32]).unwrap(), Some([1; 32]));
        for version in [0, 105, 106] {
            assert!(compression_request_nonce(version, 1, &[1; 32]).is_err());
        }
        for codec in [0, 2, u32::MAX] {
            assert!(compression_request_nonce(107, codec, &[1; 32]).is_err());
        }
        for nonce in [&[][..], &[1; 31][..], &[0; 32][..], &[1; 33][..]] {
            assert!(compression_request_nonce(107, 1, nonce).is_err());
        }
        assert!(SnapshotCompressionV1::from_wire(3, &[1; 32], 128).is_err());
        assert_eq!(compression_request_nonce(109, 2, &[1; 32]).unwrap(), Some([1; 32]));
        assert!(compression_request_nonce(108, 2, &[1; 32]).is_err());
        assert!(SnapshotCompressionV1::from_wire(2, &[1; 32], (crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64) + 1).is_err());
        assert!(SnapshotCompressionV1::from_wire(1, &[1; 32], 0).is_err());
        assert!(SnapshotCompressionV1::from_wire(1, &[1; 32], DECODED_SNAPSHOT_MAX_BYTES + 1).is_err());
        assert!(SnapshotCompressionV1::from_wire(0, &[], 1).is_err());
        assert!(snapshot_payload_size(424_900_000, []).is_err(), "106 cap must not be raised");
        assert!(snapshot_payload_size_with_cap(424_900_000, [], DECODED_SNAPSHOT_MAX_BYTES).is_ok());
        let profile = SnapshotCompressionV1 { codec: SNAPSHOT_CODEC_ZLIB_V1, nonce: [1; 32], decoded: 128 };
        let budget = Arc::new(Semaphore::new(1));
        let total = crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64 + 1;
        assert!(SnapshotReceiver::new_compressed(lease(&budget), h(1), h(2), total, chunk_count(total), profile).is_err());
        assert_eq!(budget.available_permits(), 1);
    }
    #[test]
    fn compressed_truncation_crc_bombs_declared_length_and_trailing_streams_drop_every_spill() {
        let budget = Arc::new(Semaphore::new(1));
        let carriage = native_carriage();
        let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &[] }).unwrap();
        let good = zlib(&raw);
        let profile = SnapshotCompressionV1 { codec: SNAPSHOT_CODEC_ZLIB_V1, nonce: [1; 32], decoded: raw.len() as u64 };
        let mut corrupt_crc = good.clone();
        *corrupt_crc.last_mut().unwrap() ^= 1;
        let mut trailing = good.clone();
        trailing.extend_from_slice(b"trailing");
        let mut doubled = good.clone();
        doubled.extend_from_slice(&good);
        let mut bomb = raw.clone();
        bomb.resize(bomb.len() + (1 << 20), 0);
        for bytes in [&good[..good.len() - 1], &corrupt_crc, &trailing, &doubled, &zlib(&bomb)] {
            let receiver = receive_compressed_raw(&budget, bytes, profile);
            let path = receiver.spill.path.clone();
            assert!(receiver.decode_snapshot().is_err());
            assert!(!path.exists());
            assert_eq!(budget.available_permits(), 1);
        }
        for decoded in [profile.decoded - 1, profile.decoded + 1] {
            assert!(receive_compressed_raw(&budget, &good, SnapshotCompressionV1 { decoded, ..profile }).decode_snapshot().is_err());
            assert_eq!(budget.available_permits(), 1);
        }
        let mut reader = DecodedLimitReader {
            inner: std::io::Cursor::new(vec![3; 4096]),
            expected: 17,
            read: 0,
            created: Instant::now(),
            checked_at: 0,
        };
        assert!(io::copy(&mut reader, &mut io::sink()).is_err());
        assert!(reader.read <= 17, "bomb bytes may never be emitted beyond the declared count");
    }
    #[test]
    fn compressed_hashes_bind_point_nonce_codec_length_and_resume_identity() {
        let profile = SnapshotCompressionV1 { codec: SNAPSHOT_CODEC_ZLIB_V1, nonce: [1; 32], decoded: 128 };
        let altered_nonce = SnapshotCompressionV1 { codec: SNAPSHOT_CODEC_ZLIB_V1, nonce: [2; 32], ..profile };
        let altered_size = SnapshotCompressionV1 { decoded: 129, ..profile };
        let raw = vec![0x38; CHUNK_BYTES + 1];
        let count = chunk_count(raw.len() as u64);
        let expected = chunk_hash_profile(h(1), h(2), raw.len() as u64, count, 0, &raw[..CHUNK_BYTES], Some(profile));
        for other in [None, Some(altered_nonce), Some(altered_size)] {
            assert_ne!(expected, chunk_hash_profile(h(1), h(2), raw.len() as u64, count, 0, &raw[..CHUNK_BYTES], other));
        }
        assert_ne!(expected, chunk_hash_profile(h(9), h(2), raw.len() as u64, count, 0, &raw[..CHUNK_BYTES], Some(profile)));
        let budget = Arc::new(Semaphore::new(2));
        let mut hash = stream_hasher_profile(h(1), Some(profile));
        hash.write(&raw);
        let root = hash.finalize();
        let mut receiver = SnapshotReceiver::new_compressed(lease(&budget), h(1), root, raw.len() as u64, count, profile).unwrap();
        assert!(
            receiver
                .accept(
                    h(1),
                    root,
                    raw.len() as u64,
                    count,
                    0,
                    chunk_hash_profile(h(1), root, raw.len() as u64, count, 0, &raw[..CHUNK_BYTES], Some(altered_nonce)),
                    &raw[..CHUNK_BYTES]
                )
                .is_err()
        );
        receiver
            .accept(
                h(1),
                root,
                raw.len() as u64,
                count,
                0,
                chunk_hash_profile(h(1), root, raw.len() as u64, count, 0, &raw[..CHUNK_BYTES], Some(profile)),
                &raw[..CHUNK_BYTES],
            )
            .unwrap();
        receiver.spill.created = Instant::now() - SESSION_TTL;
        assert!(
            receiver
                .accept(
                    h(1),
                    root,
                    raw.len() as u64,
                    count,
                    1,
                    chunk_hash_profile(h(1), root, raw.len() as u64, count, 1, &raw[CHUNK_BYTES..], Some(profile)),
                    &raw[CHUNK_BYTES..]
                )
                .is_err()
        );
        drop(receiver);
        assert_eq!(budget.available_permits(), 2);
        let mut left = stream_hasher_profile(h(1), Some(profile));
        left.write(&raw);
        let mut right = stream_hasher_profile(h(1), Some(altered_nonce));
        right.write(&raw);
        assert_ne!(left.finalize(), right.finalize(), "an identical stream cannot resume under a different nonce");
    }
    #[test]
    fn compressed_snapshot_resumes_on_identical_peer_and_refuses_new_nonce() {
        let budget = Arc::new(Semaphore::new(3));
        let carriage = native_carriage();
        let mut entropy = Vec::with_capacity(CHUNK_BYTES + 4096);
        for n in 0..(CHUNK_BYTES / 64 + 64) {
            let mut hash = MerkleBranchHash64::new();
            hash.write((n as u64).to_le_bytes());
            entropy.extend_from_slice(hash.finalize().as_byte_slice());
        }
        let declarations = vec![(h(7), entropy)];
        let mut old = SnapshotSender::encode_snapshot_compressed(lease(&budget), h(1), &carriage, &declarations, [3; 32]).unwrap();
        assert!(old.count > 1, "resume test must actually cross a wire chunk boundary");
        let profile = old.profile.unwrap();
        let root = old.root;
        let mut receiver = SnapshotReceiver::new_compressed(lease(&budget), h(1), root, old.total, old.count, profile).unwrap();
        let first = old.chunk(h(1), None, 0).unwrap();
        receiver
            .accept(
                h(1),
                root,
                old.total,
                old.count,
                0,
                chunk_hash_profile(h(1), root, old.total, old.count, 0, &first, Some(profile)),
                &first,
            )
            .unwrap();
        drop(old);
        let mut wrong = SnapshotSender::encode_snapshot_compressed(lease(&budget), h(1), &carriage, &declarations, [4; 32]).unwrap();
        assert!(wrong.resume(h(1), root, 1).is_err());
        drop(wrong);
        let mut peer = SnapshotSender::encode_snapshot_compressed(lease(&budget), h(1), &carriage, &declarations, [3; 32]).unwrap();
        peer.resume(h(1), root, receiver.next_index()).unwrap();
        for index in receiver.next_index()..peer.count {
            let bytes = peer.chunk(h(1), Some(root), index).unwrap();
            receiver
                .accept(
                    h(1),
                    root,
                    peer.total,
                    peer.count,
                    index,
                    chunk_hash_profile(h(1), root, peer.total, peer.count, index, &bytes, Some(profile)),
                    &bytes,
                )
                .unwrap();
        }
        let (decoded, rows) = receiver.decode_snapshot().unwrap();
        assert_eq!(decoded, carriage);
        assert_eq!(rows, declarations);
        drop(peer);
        assert_eq!(budget.available_permits(), 3);
    }
    #[test]
    fn legacy_and_chunked_payload_caps_include_all_declaration_framing() {
        let cap = crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64;
        assert_eq!(snapshot_payload_size(cap - 14, []).unwrap(), cap);
        assert!(snapshot_payload_size(cap - 13, []).is_err());
        // Legacy previously admitted this near-cap payload; the same state's
        // canonical chunk envelope cannot fit. Both consumers now refuse it.
        assert!(snapshot_payload_size(cap - 4, []).is_err());
        let state = cap - 14 - 2 * 68 - 17;
        assert_eq!(snapshot_payload_size(state, [8, 9]).unwrap(), cap);
        assert!(snapshot_payload_size(state, [8, 10]).is_err());
        assert!(snapshot_payload_size(state, [8, 9, 0]).is_err(), "empty/unknown declarations still cost their header");
        assert_eq!(snapshot_payload_size(cap - 14 - 1024 * 68, std::iter::repeat_n(0, 1024)).unwrap(), cap);
        assert!(snapshot_payload_size(0, []).is_err());
        assert!(snapshot_payload_size(u64::MAX, []).is_err());
        assert!(snapshot_payload_size(1, [u32::MAX as u64 + 1]).is_err());
        assert!(snapshot_payload_size(1, [u64::MAX]).is_err());
    }

    #[test]
    fn canonical_snapshot_measurement_and_legacy_encoding_match_actual_bytes() {
        let carriage = native_carriage();
        let declarations = vec![(h(7), vec![1, 2]), (h(9), vec![3; 101]), (h(7), vec![])];
        let envelope = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &declarations }).unwrap();
        assert_eq!(snapshot_encoded_size(&carriage, &declarations).unwrap(), envelope.len() as u64);
        let legacy = encode_legacy_snapshot(&carriage, &declarations).unwrap();
        assert_eq!(legacy.capacity(), legacy.len(), "precounted legacy serialization has no unreserved growth tail");
        assert_eq!(legacy, borsh::to_vec(&carriage).unwrap(), "legacy state bytes and optional tails are unchanged");
        let old_field_sum = legacy.len() + declarations.iter().map(|(_, bytes)| bytes.len()).sum::<usize>();
        assert_eq!(envelope.len() - old_field_sum, 14 + 68 * declarations.len());
        eprintln!(
            "[rfc06-snapshot-size] old_field_sum={old_field_sum} canonical_bytes={} omitted_bytes={}",
            envelope.len(),
            envelope.len() - old_field_sum
        );
        assert_eq!(
            snapshot_payload_size(legacy.len() as u64, declarations.iter().map(|(_, b)| b.len() as u64)).unwrap(),
            envelope.len() as u64
        );
    }
    #[test]
    fn chunked_snapshot_preserves_bytes_and_authenticates_complete_stream() {
        let budget = Arc::new(Semaphore::new(2));
        let value = (vec![7u8; CHUNK_BYTES + 137], vec![(h(7), vec![3u8; 11])]);
        let mut sender = SnapshotSender::encode(lease(&budget), h(1), &value).unwrap();
        let mut receiver = SnapshotReceiver::new(lease(&budget), h(1), sender.root, sender.total, sender.count).unwrap();
        let root = sender.root;
        let total = sender.total;
        let count = sender.count;
        for i in 0..count {
            let bytes = sender.chunk(h(1), (i > 0).then_some(root), i).unwrap();
            receiver.accept(h(1), root, total, count, i, chunk_hash(h(1), root, total, count, i, &bytes), &bytes).unwrap();
        }
        let got: (Vec<u8>, Vec<(Hash64, Vec<u8>)>) = receiver.decode().unwrap();
        assert_eq!(got, value);
    }

    #[test]
    fn chunk_spill_failure_and_expiry_do_not_implicate_the_peer() {
        let budget = Arc::new(Semaphore::new(1));
        let spill = SnapshotSpill::with_budget(budget.clone()).unwrap();
        let mut sender = SnapshotSender::encode(spill, h(1), &vec![1u8; 10]).unwrap();
        assert!(matches!(sender.chunk(h(2), None, 0), Err(NativeSnapshotDecodeErrorV1::Encoding(_))));
        // This is a local storage failure after an otherwise valid request.
        sender.spill.file.set_len(0).unwrap();
        assert!(matches!(sender.chunk(h(1), None, 0), Err(NativeSnapshotDecodeErrorV1::LocalStorage(ref error))
            if error.kind() == io::ErrorKind::UnexpectedEof));
        assert_eq!(sender.next_index(), 0);
        sender.spill.created = Instant::now() - SESSION_TTL - Duration::from_secs(1);
        assert!(
            matches!(sender.chunk(h(1), None, 0), Err(NativeSnapshotDecodeErrorV1::LocalStorage(ref error)) if error.kind() == io::ErrorKind::TimedOut)
        );
        drop(sender);
        assert_eq!(budget.available_permits(), 1);
    }
    #[test]
    fn hostile_chunk_order_replay_point_root_length_eof_and_cancel_are_refused() {
        let budget = Arc::new(Semaphore::new(2));
        let mut sender = SnapshotSender::encode(lease(&budget), h(1), &vec![4u8; CHUNK_BYTES + 1]).unwrap();
        let root = sender.root;
        let total = sender.total;
        let count = sender.count;
        assert!(sender.chunk(h(2), None, 0).is_err());
        assert!(sender.chunk(h(1), Some(root), 0).is_err());
        let first = sender.chunk(h(1), None, 0).unwrap();
        assert!(sender.chunk(h(1), None, 0).is_err());
        let mut receiver = SnapshotReceiver::new(lease(&budget), h(1), root, total, count).unwrap();
        let path = receiver.spill.path.clone();
        let hash = chunk_hash(h(1), root, total, count, 0, &first);
        assert!(receiver.accept(h(2), root, total, count, 0, hash, &first).is_err());
        assert!(receiver.accept(h(1), h(9), total, count, 0, hash, &first).is_err());
        assert!(receiver.accept(h(1), root, total + 1, count, 0, hash, &first).is_err());
        assert!(receiver.accept(h(1), root, total, count, 1, hash, &first).is_err());
        assert!(receiver.accept(h(1), root, total, count, 0, h(9), &first).is_err());
        assert!(receiver.accept(h(1), root, total, count, 0, hash, &first[..first.len() - 1]).is_err());
        receiver.accept(h(1), root, total, count, 0, hash, &first).unwrap();
        assert!(receiver.accept(h(1), root, total, count, 0, hash, &first).is_err());
        assert!(receiver.decode::<Vec<u8>>().is_err(), "early EOF cannot decode");
        assert!(!path.exists());
        assert!(
            SnapshotReceiver::new(lease(&budget), h(1), root, crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64 + 1, 1).is_err()
        );
        assert!(SnapshotReceiver::new(lease(&budget), h(1), root, total, count + 1).is_err());
    }
    #[test]
    fn immutable_stream_survives_a_new_snapshot_and_refuses_forged_stream_and_trailing_data() {
        let budget = Arc::new(Semaphore::new(3));
        let value = vec![5u8; CHUNK_BYTES + 2];
        let mut sender = SnapshotSender::encode(lease(&budget), h(1), &value).unwrap();
        let root = sender.root;
        let total = sender.total;
        let count = sender.count;
        let first = sender.chunk(h(1), None, 0).unwrap();
        let mut receiver = SnapshotReceiver::new(lease(&budget), h(1), root, total, count).unwrap();
        receiver.accept(h(1), root, total, count, 0, chunk_hash(h(1), root, total, count, 0, &first), &first).unwrap();
        // Another generation can be encoded while a transfer is paused; the original file and
        // commitment remain immutable and a resumed continuation names only that original root.
        let newer = SnapshotSender::encode(lease(&budget), h(1), &vec![8u8; 32]).unwrap();
        assert_ne!(root, newer.root);
        assert!(sender.chunk(h(1), Some(newer.root), 1).is_err());
        let second = sender.chunk(h(1), Some(root), 1).unwrap();
        receiver.accept(h(1), root, total, count, 1, chunk_hash(h(1), root, total, count, 1, &second), &second).unwrap();
        assert_eq!(receiver.decode::<Vec<u8>>().unwrap(), value);
        drop(sender);
        drop(newer);
        let raw = [7u8];
        let forged = h(99);
        let mut receiver = SnapshotReceiver::new(lease(&budget), h(1), forged, 1, 1).unwrap();
        receiver.accept(h(1), forged, 1, 1, 0, chunk_hash(h(1), forged, 1, 1, 0, &raw), &raw).unwrap();
        assert!(receiver.decode::<u8>().is_err(), "per-chunk hashes alone cannot authenticate a forged stream root");
        let mut sender = SnapshotSender::encode(lease(&budget), h(1), &(7u8, 8u8)).unwrap();
        let mut receiver = SnapshotReceiver::new(lease(&budget), h(1), sender.root, sender.total, sender.count).unwrap();
        let root = sender.root;
        let bytes = sender.chunk(h(1), None, 0).unwrap();
        receiver
            .accept(h(1), root, sender.total, sender.count, 0, chunk_hash(h(1), root, sender.total, sender.count, 0, &bytes), &bytes)
            .unwrap();
        assert!(receiver.decode::<u8>().is_err(), "a valid stream still cannot add trailing unparsed bytes");
    }

    #[test]
    fn an_interrupted_stream_resumes_on_an_identical_new_peer_without_rewind_or_new_state() {
        let budget = Arc::new(Semaphore::new(2));
        let cache = ResumeCache::new();
        let value = vec![6u8; CHUNK_BYTES + 63];
        let mut sender = SnapshotSender::encode(lease(&budget), h(1), &value).unwrap();
        let mut receiver = SnapshotReceiver::new(lease(&budget), h(1), sender.root, sender.total, sender.count).unwrap();
        let root = sender.root;
        let total = sender.total;
        let count = sender.count;
        let first = sender.chunk(h(1), None, 0).unwrap();
        receiver.accept(h(1), root, total, count, 0, chunk_hash(h(1), root, total, count, 0, &first), &first).unwrap();
        drop(sender); // The original connection and server spill are gone.
        let (generation, _) = cache.put(receiver).unwrap();
        assert_eq!(budget.available_permits(), 1, "parked receiver spends the original global lease");
        let mut rebuilt = SnapshotSender::encode(lease(&budget), h(1), &value).unwrap();
        let mut receiver = cache.take(h(1)).unwrap();
        assert_eq!(receiver.point(), h(1));
        assert_eq!(receiver.root(), root);
        assert_eq!(receiver.next_index(), 1);
        cache.expire(generation); // Old timer cannot delete a taken/resuming generation.
        assert!(rebuilt.resume(h(2), root, 1).is_err());
        assert!(rebuilt.resume(h(1), h(99), 1).is_err());
        rebuilt.resume(h(1), root, receiver.next_index()).unwrap();
        assert!(rebuilt.resume(h(1), root, 1).is_err(), "initial resume cannot be repeated");
        assert!(rebuilt.chunk(h(1), None, 0).is_err(), "resume may never rewind a prepaid traversal");
        let second = rebuilt.chunk(h(1), Some(root), 1).unwrap();
        receiver.accept(h(1), root, total, count, 1, chunk_hash(h(1), root, total, count, 1, &second), &second).unwrap();
        assert_eq!(receiver.decode::<Vec<u8>>().unwrap(), value);
        drop(rebuilt);
        assert_eq!(budget.available_permits(), 2);
    }
    #[test]
    fn parked_receiver_point_change_replacement_and_expiry_release_only_owned_generations() {
        let budget = Arc::new(Semaphore::new(3));
        let cache = ResumeCache::new();
        let partial = || {
            let mut sender = SnapshotSender::encode(lease(&budget), h(1), &vec![8u8; CHUNK_BYTES + 1]).unwrap();
            let mut receiver = SnapshotReceiver::new(lease(&budget), h(1), sender.root, sender.total, sender.count).unwrap();
            let bytes = sender.chunk(h(1), None, 0).unwrap();
            receiver
                .accept(
                    h(1),
                    sender.root,
                    sender.total,
                    sender.count,
                    0,
                    chunk_hash(h(1), sender.root, sender.total, sender.count, 0, &bytes),
                    &bytes,
                )
                .unwrap();
            receiver
        };
        let (older, _) = cache.put(partial()).unwrap();
        let (newer, _) = cache.put(partial()).unwrap();
        assert_ne!(older, newer);
        assert_eq!(budget.available_permits(), 2, "only one receiver is retained after replacement");
        cache.expire(older);
        assert_eq!(budget.available_permits(), 2, "older expiry must not delete a replacement");
        assert!(cache.take(h(2)).is_none(), "changing pruning point drops the incomplete old stream");
        assert_eq!(budget.available_permits(), 3);
        let (expired, _) = cache.put(partial()).unwrap();
        cache.expire(expired);
        assert_eq!(budget.available_permits(), 3);
        let mut receiver = partial();
        receiver.spill.created = Instant::now() - SESSION_TTL;
        assert!(cache.put(receiver).is_none(), "an expired stream cannot extend its lifetime by parking");
        assert_eq!(budget.available_permits(), 3);
        let receiver = partial();
        drop(receiver); // Explicit cancellation does not park or retain a permit.
        assert_eq!(budget.available_permits(), 3);
    }

    fn receive_raw(budget: &Arc<Semaphore>, point: Hash64, raw: &[u8]) -> SnapshotReceiver {
        let mut hasher = stream_hasher(point);
        hasher.write(raw);
        let root = hasher.finalize();
        let total = raw.len() as u64;
        let count = chunk_count(total);
        let mut receiver = SnapshotReceiver::new(lease(budget), point, root, total, count).unwrap();
        for (i, bytes) in raw.chunks(CHUNK_BYTES).enumerate() {
            receiver
                .accept(point, root, total, count, i as u32, chunk_hash(point, root, total, count, i as u32, bytes), bytes)
                .unwrap();
        }
        receiver
    }
    fn native_carriage() -> PalwStateCarriageV2 {
        use kaspa_consensus_core::palw_state_v2::{PalwChainStateV2, PalwClassStateV2, PalwClassStatusV2, PalwPwuRuleV2};
        let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
        carriage.classes.insert(
            h(7),
            PalwClassStateV2 {
                artifact_root: h(8),
                slash_value_per_pwu: 1,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1),
                status: PalwClassStatusV2::Active,
                registered_daa: 0,
                registrant_bond: None,
                fused_attention: false,
            },
        );
        // Nonempty EOF tails exercise the native custom decoder rather than a generic tuple.
        carriage.class_court_windows.insert(h(7), 30);
        carriage
    }
    #[test]
    fn native_optional_tails_are_framed_and_declarations_keep_legacy_filtering_semantics() {
        let budget = Arc::new(Semaphore::new(2));
        let carriage = native_carriage();
        let offered = vec![(h(7), vec![1, 2]), (h(9), vec![3; 4096]), (h(7), vec![4; 4096])];
        let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &offered }).unwrap();
        let (decoded, declarations) = receive_raw(&budget, h(1), &raw).decode_snapshot().unwrap();
        assert_eq!(decoded, carriage);
        assert_eq!(declarations, vec![(h(7), vec![1, 2])]);
        let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &carriage, declarations: &[] }).unwrap();
        let (decoded, declarations) = receive_raw(&budget, h(1), &raw).decode_snapshot().unwrap();
        assert_eq!(decoded, carriage, "missing graph carriage does not gate consensus-state acquisition");
        assert!(declarations.is_empty(), "native adoption leaves this class unservable until its local declaration is supplied");
        // The unframed tuple really is incompatible with native EOF-tail decoding.
        assert!(borsh::from_slice::<Snapshot>(&borsh::to_vec(&(carriage, offered)).unwrap()).is_err());
    }
    #[test]
    fn malicious_borsh_lengths_truncated_vectors_and_declaration_amplification_are_refused() {
        let budget = Arc::new(Semaphore::new(1));
        let empty = PalwStateCarriageV2::from_state(&kaspa_consensus_core::palw_state_v2::PalwChainStateV2::genesis());
        let raw = borsh::to_vec(&SnapshotEnvelope { carriage: &empty, declarations: &[] }).unwrap();
        let mut wrong_version = raw.clone();
        wrong_version[..2].copy_from_slice(&2u16.to_le_bytes());
        assert!(receive_raw(&budget, h(1), &wrong_version).decode_snapshot().is_err());
        let mut over = raw.clone();
        over[2..10].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(receive_raw(&budget, h(1), &over).decode_snapshot().is_err());
        let mut huge_count = raw.clone();
        let end = huge_count.len();
        huge_count[end - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(
            receive_raw(&budget, h(1), &huge_count).decode_snapshot().is_err(),
            "declarations must not preallocate from an attacker count"
        );
        // Native bonds map's first count becomes 2^32-1 with no elements. Borsh's
        // cautious allocation must refuse EOF; no complete/native state may escape.
        let mut truncated_map = raw.clone();
        truncated_map[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(receive_raw(&budget, h(1), &truncated_map).decode_snapshot().is_err());
        let mut declaration = raw.clone();
        declaration.truncate(declaration.len() - 4);
        declaration.extend_from_slice(&1u32.to_le_bytes());
        declaration.extend_from_slice(h(7).as_byte_slice());
        declaration.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(receive_raw(&budget, h(1), &declaration).decode_snapshot().is_err());
        assert!(receive_raw(&budget, h(1), &u32::MAX.to_le_bytes()).decode::<Vec<u8>>().is_err());
        assert!(receive_raw(&budget, h(1), &u32::MAX.to_le_bytes()).decode::<Vec<Vec<u8>>>().is_err());
        // A valid Borsh stream can amplify allocations: 4 wire bytes per empty
        // Vec vs 24 resident bytes on 64-bit. This records why 384 MiB encoded
        // and two session permits do NOT establish a decoded-native RAM bound.
        let nested = vec![Vec::<u8>::new(); 4096];
        let bytes = borsh::to_vec(&nested).unwrap();
        let decoded = receive_raw(&budget, h(1), &bytes).decode::<Vec<Vec<u8>>>().unwrap();
        let resident_lower_bound = decoded.capacity() * std::mem::size_of::<Vec<u8>>();
        assert!(resident_lower_bound > bytes.len(), "encoded cap alone is not a decoded-memory cap");
        assert_eq!(budget.available_permits(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn spill_is_unlinked_while_its_handle_is_live_so_crash_cannot_orphan_it() {
        let budget = Arc::new(Semaphore::new(1));
        let mut spill = lease(&budget);
        assert!(!spill.path.exists(), "cleanup must precede process death, not depend only on Drop");
        spill.file.write_all(b"still usable through its private FD").unwrap();
        spill.file.seek(SeekFrom::Start(0)).unwrap();
        let mut data = String::new();
        spill.file.read_to_string(&mut data).unwrap();
        assert_eq!(data, "still usable through its private FD");
        drop(spill);
        assert_eq!(budget.available_permits(), 1);
    }

    #[test]
    fn global_session_lease_is_released_on_cancel_and_io_failure() {
        let budget = Arc::new(Semaphore::new(1));
        let spill = lease(&budget);
        let path = spill.path.clone();
        assert!(SnapshotSpill::with_budget(Arc::clone(&budget)).is_err());
        drop(spill);
        assert!(!path.exists());
        let mut spill = lease(&budget);
        let path = spill.path.clone();
        let mut writer = BoundedHashWriter {
            file: &mut spill.file,
            hash: stream_hasher(h(1)),
            total: crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64,
        };
        assert!(writer.write_all(&[1]).is_err());
        drop(spill);
        assert!(!path.exists());
        assert_eq!(budget.available_permits(), 1);
    }
}

#[cfg(test)]
#[path = "palw_row_stream_peer_tests.rs"]
mod row_stream_peer_tests;
