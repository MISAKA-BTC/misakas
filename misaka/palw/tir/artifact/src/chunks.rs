//! # Content-addressed chunks of a PALW-TIR artifact (RFC-0002 Part II: the streaming loader)
//!
//! A conversion produces the artifact's tensors in the order the LOWERING reaches them (one block
//! occurrence at a time: `pre`, layer 0, layer 1, …, `post`), while a `PALWTIR1` container holds
//! them in INVENTORY order (param-major). A converter that must hold every tensor until the last
//! one is made holds the whole artifact. This module is the way out: each tensor instance is cut
//! into **canonical chunks** as it is produced, every chunk is stored under the hash of its bytes
//! ([`ChunkStore`]), and the container is assembled from the chunks afterwards, in inventory order,
//! by streaming ([`write_container_v1_chunked`]). Resident memory is a chunk, never an artifact.
//!
//! * **Canonical.** A tensor instance of `n` bytes is cut at multiples of [`PALW_TIR_CHUNK_BYTES_V1`]
//!   from its first byte, the last chunk shorter — a function of the bytes and nothing else (not of
//!   the block sizes the converter produced them in, not of its memory budget, not of its thread
//!   count). The same tensor is therefore the same chunks in every conversion: **a composite class
//!   (a parent and its adapter) shares the parent's chunks**, the parent's weight codes being the
//!   same bytes whichever artifact they sit in, and a converter that is interrupted resumes by the
//!   chunks it already has.
//! * **Content-addressed.** A chunk's id is `BLAKE2b-256(key "misaka-palw/tir/chunk/v1", bytes)`.
//!   [`ChunkStore::put`] of bytes the store already holds writes nothing. Every read the container
//!   assembly makes re-hashes the chunk, so a corrupted store is refused, not copied.
//! * **Not identity.** A chunk id is a storage address. What the chain commits to is the inventory
//!   root over the tensors' leaves (`kaspa_consensus_core::palw_tir_artifact_v1`), which does not
//!   depend on any of this; the chunked artifact and the container written from it carry the same
//!   tensors and so the same root.

use crate::{PalwTirContainerError, write_container_v1_streamed};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::TirProgramV1;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The canonical chunk size: a tensor instance is cut at multiples of this (a multiple of the
/// inventory's 32 KiB leaf piece, so a chunk holds whole leaves of a row-aligned tensor).
pub const PALW_TIR_CHUNK_BYTES_V1: usize = 4 << 20;

/// Key of [`chunk_id_v1`].
pub const PALW_TIR_CHUNK_DOMAIN_V1: &[u8] = b"misaka-palw/tir/chunk/v1";

/// A chunk's content address: `BLAKE2b-256` keyed with [`PALW_TIR_CHUNK_DOMAIN_V1`] over its bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct ChunkId(pub [u8; 32]);

impl ChunkId {
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 64 || !s.is_ascii() {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, o) in out.iter_mut().enumerate() {
            *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
        }
        Some(ChunkId(out))
    }
}

impl std::fmt::Display for ChunkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl std::fmt::Debug for ChunkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ChunkId({})", &self.to_hex()[..16])
    }
}

/// The address of `bytes`.
pub fn chunk_id_v1(bytes: &[u8]) -> ChunkId {
    let h = blake2b_simd::Params::new().hash_length(32).key(PALW_TIR_CHUNK_DOMAIN_V1).hash(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(h.as_bytes());
    ChunkId(out)
}

/// What a store has been asked to do since it was opened (instrumentation: a conversion reports it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChunkStoreStats {
    /// `put` calls.
    pub puts: u64,
    /// Of them, chunks the store already held (nothing written).
    pub deduplicated: u64,
    /// Bytes handed to `put`.
    pub bytes_put: u64,
    /// Bytes actually written (new chunks).
    pub bytes_written: u64,
}

/// **A directory of chunks**, `<dir>/<first two hex digits>/<id>`. Writes are atomic (a temporary
/// file renamed into place) and idempotent, so concurrent writers of the same chunk are safe.
#[derive(Debug)]
pub struct ChunkStore {
    dir: PathBuf,
    puts: AtomicU64,
    dedup: AtomicU64,
    bytes_put: AtomicU64,
    bytes_written: AtomicU64,
    tmp_seq: AtomicU64,
}

impl ChunkStore {
    /// Open (creating it if need be) the store at `dir`.
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir.join("tmp"))?;
        Ok(Self {
            dir: dir.to_path_buf(),
            puts: AtomicU64::new(0),
            dedup: AtomicU64::new(0),
            bytes_put: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            tmp_seq: AtomicU64::new(0),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path_of(&self, id: &ChunkId) -> PathBuf {
        let h = id.to_hex();
        self.dir.join(&h[..2]).join(&h)
    }

    pub fn contains(&self, id: &ChunkId) -> bool {
        self.path_of(id).is_file()
    }

    /// Store `bytes`; returns their id. Bytes the store already holds (same id, same length) are
    /// not written again.
    pub fn put(&self, bytes: &[u8]) -> std::io::Result<ChunkId> {
        let id = chunk_id_v1(bytes);
        self.puts.fetch_add(1, Ordering::Relaxed);
        self.bytes_put.fetch_add(bytes.len() as u64, Ordering::Relaxed);
        let path = self.path_of(&id);
        if std::fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64) {
            self.dedup.fetch_add(1, Ordering::Relaxed);
            return Ok(id);
        }
        std::fs::create_dir_all(path.parent().expect("a chunk path has a parent"))?;
        let tmp = self.dir.join("tmp").join(format!("{}.{}", std::process::id(), self.tmp_seq.fetch_add(1, Ordering::Relaxed)));
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(bytes)?;
            f.flush()?;
        }
        std::fs::rename(&tmp, &path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })?;
        self.bytes_written.fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(id)
    }

    /// The chunk's bytes, checked against its id.
    pub fn get(&self, id: &ChunkId) -> std::io::Result<Vec<u8>> {
        let bytes = std::fs::read(self.path_of(id))?;
        if chunk_id_v1(&bytes) != *id {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("chunk {id} does not hash to its address")));
        }
        Ok(bytes)
    }

    pub fn stats(&self) -> ChunkStoreStats {
        ChunkStoreStats {
            puts: self.puts.load(Ordering::Relaxed),
            deduplicated: self.dedup.load(Ordering::Relaxed),
            bytes_put: self.bytes_put.load(Ordering::Relaxed),
            bytes_written: self.bytes_written.load(Ordering::Relaxed),
        }
    }
}

/// One chunk of a tensor instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ChunkRef {
    pub id: ChunkId,
    pub len: u32,
}

/// The chunks of one tensor instance, in order.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TensorChunks {
    pub param: u16,
    pub layer: Option<u16>,
    pub bytes: u64,
    pub chunks: Vec<ChunkRef>,
}

/// **The chunked artifact**: for every tensor instance made, its chunks. The recipe a container is
/// assembled from ([`write_container_v1_chunked`]); with the store it is the artifact's tensors.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ChunkedArtifactV1 {
    pub tensors: BTreeMap<(u16, Option<u16>), TensorChunks>,
}

impl ChunkedArtifactV1 {
    pub fn insert(&mut self, t: TensorChunks) {
        self.tensors.insert((t.param, t.layer), t);
    }

    /// Bytes of every instance.
    pub fn bytes(&self) -> u64 {
        self.tensors.values().map(|t| t.bytes).sum()
    }

    pub fn chunk_count(&self) -> usize {
        self.tensors.values().map(|t| t.chunks.len()).sum()
    }

    /// The distinct chunk ids this artifact names.
    pub fn distinct_chunks(&self) -> std::collections::BTreeSet<ChunkId> {
        self.tensors.values().flat_map(|t| t.chunks.iter().map(|c| c.id)).collect()
    }
}

/// **Cuts a tensor instance into canonical chunks as its bytes arrive**, in pieces of any size:
/// [`push`](Self::push) holds at most one chunk (plus the piece being pushed), and
/// [`finish`](Self::finish) stores the last, shorter one. The same bytes are the same chunks
/// however they are pushed.
pub struct InstanceWriter<'s> {
    store: &'s ChunkStore,
    param: u16,
    layer: Option<u16>,
    buf: Vec<u8>,
    chunks: Vec<ChunkRef>,
    bytes: u64,
    /// The largest buffer this writer ever held (instrumentation).
    pub peak_buffer: usize,
}

impl<'s> InstanceWriter<'s> {
    pub fn new(store: &'s ChunkStore, param: u16, layer: Option<u16>) -> Self {
        Self { store, param, layer, buf: Vec::new(), chunks: Vec::new(), bytes: 0, peak_buffer: 0 }
    }

    fn emit(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let id = self.store.put(bytes)?;
        self.chunks.push(ChunkRef { id, len: bytes.len() as u32 });
        Ok(())
    }

    pub fn push(&mut self, mut piece: &[u8]) -> std::io::Result<()> {
        self.bytes += piece.len() as u64;
        // Complete the chunk in progress first.
        if !self.buf.is_empty() {
            let take = (PALW_TIR_CHUNK_BYTES_V1 - self.buf.len()).min(piece.len());
            self.buf.extend_from_slice(&piece[..take]);
            piece = &piece[take..];
            self.peak_buffer = self.peak_buffer.max(self.buf.len());
            if self.buf.len() == PALW_TIR_CHUNK_BYTES_V1 {
                let full = std::mem::take(&mut self.buf);
                self.emit(&full)?;
                self.buf = full;
                self.buf.clear();
            }
        }
        // Whole chunks straight from the piece, no copy.
        while piece.len() >= PALW_TIR_CHUNK_BYTES_V1 {
            let (head, rest) = piece.split_at(PALW_TIR_CHUNK_BYTES_V1);
            self.emit(head)?;
            piece = rest;
        }
        if !piece.is_empty() {
            self.buf.extend_from_slice(piece);
            self.peak_buffer = self.peak_buffer.max(self.buf.len());
        }
        Ok(())
    }

    pub fn finish(mut self) -> std::io::Result<TensorChunks> {
        if !self.buf.is_empty() {
            let last = std::mem::take(&mut self.buf);
            self.emit(&last)?;
        }
        Ok(TensorChunks { param: self.param, layer: self.layer, bytes: self.bytes, chunks: self.chunks })
    }
}

/// Store one whole tensor instance (a caller that holds it anyway).
pub fn put_instance(store: &ChunkStore, param: u16, layer: Option<u16>, bytes: &[u8]) -> std::io::Result<TensorChunks> {
    let mut w = InstanceWriter::new(store, param, layer);
    w.push(bytes)?;
    w.finish()
}

/// Stream the chunks of `t` to `out`, each re-hashed against its address.
pub fn stream_tensor_chunks(store: &ChunkStore, t: &TensorChunks, out: &mut dyn Write) -> Result<(), String> {
    let mut total = 0u64;
    for c in &t.chunks {
        let bytes = store.get(&c.id).map_err(|e| format!("chunk {}: {e}", c.id))?;
        if bytes.len() != c.len as usize {
            return Err(format!("chunk {} is {} bytes, the index says {}", c.id, bytes.len(), c.len));
        }
        out.write_all(&bytes).map_err(|e| e.to_string())?;
        total += bytes.len() as u64;
    }
    if total != t.bytes {
        return Err(format!("the chunks of param {} (layer {:?}) hold {total} bytes, the index says {}", t.param, t.layer, t.bytes));
    }
    Ok(())
}

/// **Assemble a `PALWTIR1` container from chunks**, streaming: each instance, in inventory order,
/// is read from the store a chunk at a time. The file is byte-identical to the one
/// [`write_container_v1`](crate::write_container_v1) writes for the same tensors. Returns the file
/// digest.
pub fn write_container_v1_chunked(
    path: &Path,
    program: &TirProgramV1,
    layout: Vec<u8>,
    tokenizer_id: [u8; 64],
    meta: String,
    store: &ChunkStore,
    artifact: &ChunkedArtifactV1,
) -> Result<[u8; 64], PalwTirContainerError> {
    write_container_v1_streamed(path, program, layout, tokenizer_id, meta, &mut |j, l, out| {
        let t = artifact.tensors.get(&(j, l)).ok_or_else(|| format!("param {j} (layer {l:?}) was never produced"))?;
        stream_tensor_chunks(store, t, out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PalwTirContainerV1, file_digest_v1, param_instances_v1, tensor_bytes_v1, write_container_v1};
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{DType, Ref, TensorType};

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tir-chunks-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("tmp dir");
        d
    }

    fn program() -> TirProgramV1 {
        let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
        let table = pb.param("embed.table", DType::I8, &[16, 4], false);
        let w = pb.param("blk.w", DType::I8, &[4, 4], true);
        let head = pb.param("head.w", DType::I16, &[16, 4], false);
        let carry = vec![TensorType::fixed(DType::I32, &[4])];
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let row = b.gather(table, Ref::Input(0), 0, 0);
            let row = b.cast(row, DType::I32);
            b.finish(&[row])
        };
        let layer = {
            let mut b = pb.block("layer", carry.clone());
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let acc = b.matmul(w, x, DType::I64);
            let acc = b.reshape_fixed(acc, &[4]);
            let y = b.clamp(acc, -30_000, 30_000, DType::I32);
            b.finish(&[y])
        };
        let post = {
            let mut b = pb.block("post", carry.clone());
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let l = b.matmul(head, x, DType::I64);
            let l = b.reshape_fixed(l, &[16]);
            let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
            b.commit(l);
            b.finish(&[])
        };
        let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
        pb.finish(pre, vec![layer, layer, layer], post, logits)
    }

    fn bytes_of(p: &TirProgramV1, j: u16, l: Option<u16>) -> Vec<u8> {
        let n = tensor_bytes_v1(p, j) as usize;
        (0..n).map(|i| ((i * 31 + j as usize * 7 + l.map_or(0, |l| l as usize) * 13) % 251) as u8).collect()
    }

    #[test]
    fn a_chunk_is_its_hash_and_a_repeated_put_writes_nothing() {
        let d = tmpdir("put");
        let s = ChunkStore::open(&d).expect("store");
        let a = s.put(b"hello chunk").expect("put");
        assert_eq!(a, chunk_id_v1(b"hello chunk"));
        assert_ne!(a, chunk_id_v1(b"hello chunk "));
        let b = s.put(b"hello chunk").expect("put again");
        assert_eq!(a, b);
        let st = s.stats();
        assert_eq!((st.puts, st.deduplicated, st.bytes_written), (2, 1, 11));
        assert_eq!(s.get(&a).expect("get"), b"hello chunk");
        assert_eq!(ChunkId::from_hex(&a.to_hex()), Some(a));
        // A corrupted chunk is refused, not served.
        std::fs::write(s.path_of(&a), b"hello chunk!").expect("corrupt");
        assert!(s.get(&a).is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_same_bytes_are_the_same_chunks_however_they_arrive() {
        let d = tmpdir("canon");
        let s = ChunkStore::open(&d).expect("store");
        let n = 2 * PALW_TIR_CHUNK_BYTES_V1 + 12_345;
        let data: Vec<u8> = (0..n).map(|i| (i % 253) as u8).collect();
        let whole = put_instance(&s, 3, Some(1), &data).expect("whole");
        assert_eq!(whole.chunks.len(), 3);
        assert_eq!(whole.chunks.iter().map(|c| c.len as usize).collect::<Vec<_>>(), vec![PALW_TIR_CHUNK_BYTES_V1, PALW_TIR_CHUNK_BYTES_V1, 12_345]);
        for piece in [1usize, 7, 4096, PALW_TIR_CHUNK_BYTES_V1 - 1, PALW_TIR_CHUNK_BYTES_V1 + 1, 3 * PALW_TIR_CHUNK_BYTES_V1] {
            let mut w = InstanceWriter::new(&s, 3, Some(1));
            for c in data.chunks(piece) {
                w.push(c).expect("push");
            }
            assert!(w.peak_buffer <= PALW_TIR_CHUNK_BYTES_V1, "the writer holds at most a chunk");
            assert_eq!(w.finish().expect("finish"), whole, "pieces of {piece}");
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_container_from_chunks_is_the_container_from_tensors() {
        let d = tmpdir("container");
        let p = program();
        let direct = d.join("direct.palwtir");
        let digest = write_container_v1(&direct, &p, vec![1, 2], [9u8; 64], "{\"a\":1}".into(), &mut |j, l| Ok(bytes_of(&p, j, l)))
            .expect("direct");
        // Produce the instances in the order a lowering would (occurrence-major), not inventory order.
        let store = ChunkStore::open(&d.join("chunks")).expect("store");
        let mut art = ChunkedArtifactV1::default();
        let inst = param_instances_v1(&p);
        let mut order: Vec<(u16, Option<u16>)> = Vec::new();
        for (j, v) in inst.iter().enumerate() {
            if v == &vec![None] {
                order.push((j as u16, None));
            }
        }
        for l in 0..3u16 {
            for (j, v) in inst.iter().enumerate() {
                if v.contains(&Some(l)) {
                    order.push((j as u16, Some(l)));
                }
            }
        }
        for (j, l) in order {
            art.insert(put_instance(&store, j, l, &bytes_of(&p, j, l)).expect("put"));
        }
        let chunked = d.join("chunked.palwtir");
        let digest2 = write_container_v1_chunked(&chunked, &p, vec![1, 2], [9u8; 64], "{\"a\":1}".into(), &store, &art).expect("chunked");
        assert_eq!(digest, digest2);
        assert_eq!(std::fs::read(&direct).expect("read"), std::fs::read(&chunked).expect("read"));
        assert_eq!(file_digest_v1(&chunked).expect("digest"), digest);
        PalwTirContainerV1::open(&chunked).expect("a valid container");
        // A tensor that was never produced, and a chunk that went missing, are refusals.
        let mut missing = art.clone();
        missing.tensors.remove(&(1, Some(2)));
        assert!(write_container_v1_chunked(&d.join("m.palwtir"), &p, vec![], [0u8; 64], String::new(), &store, &missing).is_err());
        let victim = art.tensors[&(1, Some(0))].chunks[0].id;
        std::fs::remove_file(store.path_of(&victim)).expect("remove");
        assert!(write_container_v1_chunked(&d.join("n.palwtir"), &p, vec![], [0u8; 64], String::new(), &store, &art).is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn two_artifacts_sharing_tensors_share_chunks() {
        // A parent and a child that adds one tensor: the child's store writes only the new bytes.
        let d = tmpdir("share");
        let store = ChunkStore::open(&d).expect("store");
        let big: Vec<u8> = (0..3 * PALW_TIR_CHUNK_BYTES_V1).map(|i| (i % 249) as u8).collect();
        let parent = put_instance(&store, 0, None, &big).expect("parent");
        let written = store.stats().bytes_written;
        let child_same = put_instance(&store, 0, None, &big).expect("child");
        assert_eq!(parent, child_same);
        assert_eq!(store.stats().bytes_written, written, "the parent's chunks are not written again");
        let adapter: Vec<u8> = vec![5u8; 1000];
        put_instance(&store, 1, None, &adapter).expect("adapter");
        assert_eq!(store.stats().bytes_written, written + 1000);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_index_round_trips_in_borsh() {
        let d = tmpdir("borsh");
        let store = ChunkStore::open(&d).expect("store");
        let mut art = ChunkedArtifactV1::default();
        art.insert(put_instance(&store, 0, None, b"abc").expect("put"));
        art.insert(put_instance(&store, 2, Some(7), &vec![1u8; PALW_TIR_CHUNK_BYTES_V1 + 1]).expect("put"));
        let b = borsh::to_vec(&art).expect("borsh");
        assert_eq!(borsh::from_slice::<ChunkedArtifactV1>(&b).expect("back"), art);
        assert_eq!(art.chunk_count(), 3);
        assert_eq!(art.distinct_chunks().len(), 3);
        let _ = std::fs::remove_dir_all(d);
    }
}
