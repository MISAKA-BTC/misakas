//! Protocol-106 PALW pruning snapshot spill/reassembly. Existing total byte policy is
//! preserved; chunk hashes authenticate a fixed point/stream and native import still
//! verifies the consensus state root. No decoded snapshot is installed by this module.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash64, hashers::MerkleBranchHash64};
use std::{
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
        let file = options.open(&path)?;
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
    pub(crate) fn decode<T: BorshDeserialize>(mut self) -> io::Result<T> {
        if !self.complete() || self.hash.clone().finalize() != self.root {
            return Err(invalid("snapshot EOF/stream hash mismatch"));
        }
        self.spill.file.flush()?;
        self.spill.file.seek(SeekFrom::Start(0))?;
        let mut reader = BufReader::new(&mut self.spill.file);
        let value = T::deserialize_reader(&mut reader)?;
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
