//! Protocol-106 PALW pruning snapshot spill/reassembly. Existing total byte policy is
//! preserved; chunk hashes authenticate a fixed point/stream and native import still
//! verifies the consensus state root. No decoded snapshot is installed by this module.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
use kaspa_hashes::{Hash64, hashers::MerkleBranchHash64};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub(crate) const CHUNK_BYTES: usize = 4 << 20;
pub(crate) const SESSION_TTL: Duration = Duration::from_secs(600);
/// Two sessions across all peers and receive/serve flows: at most 768 MiB spill files
/// and two 4 MiB chunk buffers. Decoded/native state memory is additional, not a RAM proof.
const GLOBAL_SESSIONS: usize = 2;

fn session_budget() -> Arc<Semaphore> {
    static BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();
    Arc::clone(BUDGET.get_or_init(|| Arc::new(Semaphore::new(GLOBAL_SESSIONS))))
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn stream_hasher(point: Hash64) -> MerkleBranchHash64 {
    let mut h = MerkleBranchHash64::new();
    h.write(b"misaka-palw/pruning-spill/v1");
    h.write(point.as_byte_slice());
    h
}
pub(crate) fn chunk_hash(point: Hash64, root: Hash64, total: u64, count: u32, index: u32, bytes: &[u8]) -> Hash64 {
    let mut h = MerkleBranchHash64::new();
    h.write(b"misaka-palw/pruning-chunk/v1");
    h.write(point.as_byte_slice());
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
}
impl SnapshotSpill {
    pub(crate) fn new() -> io::Result<Self> {
        Self::with_budget(session_budget())
    }
    fn with_budget(budget: Arc<Semaphore>) -> io::Result<Self> {
        let permit = budget
            .try_acquire_owned()
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "PALW snapshot session budget is busy"))?;
        let path = std::env::temp_dir().join(format!("misaka-palw-snapshot-{}.spill", uuid::Uuid::new_v4()));
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
        Ok(Self { file, path, _permit: permit, created: Instant::now() })
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

// PalwStateCarriageV2 reads its optional tagged tails to EOF. A direct tuple is
// undecodable: the declaration count is mistaken for another tail. This envelope
// gives that decoder an exact EOF without allocating another whole state Vec.
struct SnapshotEnvelope<'a> {
    carriage: &'a PalwStateCarriageV2,
    declarations: &'a [(Hash64, Vec<u8>)],
}
struct SizeWriter(u64);
impl Write for SizeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self.0.checked_add(bytes.len() as u64).ok_or_else(|| invalid("snapshot length overflow"))?;
        if self.0 > crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64 {
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
        let mut size = SizeWriter(0);
        self.carriage.serialize(&mut size)?;
        SNAPSHOT_FORMAT.serialize(writer)?;
        size.0.serialize(writer)?;
        self.carriage.serialize(writer)?;
        self.declarations.serialize(writer)
    }
}

fn decode_snapshot(reader: &mut impl Read, total: u64) -> io::Result<Snapshot> {
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
        } else if io::copy(&mut reader.take(bytes), &mut io::sink())? != bytes {
            return Err(invalid("snapshot declaration truncated"));
        }
        remaining -= bytes;
    }
    if remaining != 0 {
        return Err(invalid("snapshot has trailing declaration bytes"));
    }
    Ok((carriage, declarations))
}

pub(crate) struct SnapshotSender {
    spill: SnapshotSpill,
    pub point: Hash64,
    pub root: Hash64,
    pub total: u64,
    pub count: u32,
    next: u32,
}
impl SnapshotSender {
    /// Called on the blocking pool AFTER acquiring the global session lease and before
    /// materializing the native snapshot. Writes Borsh directly; no whole encoded Vec.
    pub(crate) fn encode<T: BorshSerialize>(mut spill: SnapshotSpill, point: Hash64, value: &T) -> io::Result<Self> {
        let mut buffered = BufWriter::with_capacity(CHUNK_BYTES, &mut spill.file);
        let mut writer = BoundedHashWriter { file: &mut buffered, hash: stream_hasher(point), total: 0 };
        borsh::to_writer(&mut writer, value)?;
        writer.flush()?;
        let total = writer.total;
        let root = writer.hash.finalize();
        drop(buffered);
        if total == 0 {
            return Err(invalid("empty snapshot stream"));
        }
        Ok(Self { spill, point, root, total, count: chunk_count(total), next: 0 })
    }
    pub(crate) fn encode_snapshot(
        spill: SnapshotSpill,
        point: Hash64,
        carriage: &PalwStateCarriageV2,
        declarations: &[(Hash64, Vec<u8>)],
    ) -> io::Result<Self> {
        Self::encode(spill, point, &SnapshotEnvelope { carriage, declarations })
    }
    pub(crate) fn expired(&self) -> bool {
        self.spill.expired()
    }
    pub(crate) fn next_index(&self) -> u32 {
        self.next
    }
    pub(crate) fn chunk(&mut self, point: Hash64, root: Option<Hash64>, index: u32) -> io::Result<Vec<u8>> {
        if self.expired()
            || point != self.point
            || index != self.next
            || index >= self.count
            || (index == 0 && root.is_some())
            || (index > 0 && root != Some(self.root))
        {
            return Err(invalid("snapshot point/root/order/session changed"));
        }
        let offset = index as u64 * CHUNK_BYTES as u64;
        let mut bytes = vec![0; (self.total - offset).min(CHUNK_BYTES as u64) as usize];
        self.spill.file.seek(SeekFrom::Start(offset))?;
        self.spill.file.read_exact(&mut bytes)?;
        self.next += 1;
        Ok(bytes)
    }
}

pub(crate) struct SnapshotReceiver {
    spill: SnapshotSpill,
    point: Hash64,
    root: Hash64,
    total: u64,
    count: u32,
    next: u32,
    received: u64,
    hash: MerkleBranchHash64,
}
impl SnapshotReceiver {
    pub(crate) fn new(spill: SnapshotSpill, point: Hash64, root: Hash64, total: u64, count: u32) -> io::Result<Self> {
        if total == 0 || total > crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES as u64 || count != chunk_count(total) {
            return Err(invalid("snapshot total/count exceeds policy"));
        }
        Ok(Self { spill, point, root, total, count, next: 0, received: 0, hash: stream_hasher(point) })
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
    ) -> io::Result<()> {
        if self.spill.expired()
            || point != self.point
            || root != self.root
            || total != self.total
            || count != self.count
            || index != self.next
            || index >= count
            || bytes.is_empty()
            || bytes.len() > CHUNK_BYTES
            || bytes.len() as u64 != (self.total - self.received).min(CHUNK_BYTES as u64)
            || chunk_hash(point, root, total, count, index, bytes) != hash
        {
            return Err(invalid("invalid snapshot chunk identity/order/size/hash"));
        }
        self.spill.file.write_all(bytes)?;
        self.hash.write(bytes);
        self.received += bytes.len() as u64;
        self.next += 1;
        Ok(())
    }
    pub(crate) fn complete(&self) -> bool {
        self.next == self.count && self.received == self.total
    }
    pub(crate) fn decode<T: BorshDeserialize>(self) -> io::Result<T> {
        self.decode_with(|reader, _| T::deserialize_reader(reader))
    }
    pub(crate) fn decode_snapshot(self) -> io::Result<Snapshot> {
        self.decode_with(decode_snapshot)
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

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn lease(budget: &Arc<Semaphore>) -> SnapshotSpill {
        SnapshotSpill::with_budget(Arc::clone(budget)).unwrap()
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
        drop(writer);
        drop(spill);
        assert!(!path.exists());
        assert_eq!(budget.available_permits(), 1);
    }
}
