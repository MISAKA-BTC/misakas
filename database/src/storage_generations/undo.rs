//! **Per-generation undo log** — what lets a generation reorg the blocks applied on top of its verified base.
//!
//! A generation is a verified base (e.g. the pruning-point state) plus the blocks applied since. Each applied block
//! appends one record carrying whatever the caller needs to reverse it (old values of the rows it changed); a reorg pops
//! records newest-first. Records are framed `len ‖ body ‖ checksum` and fsynced on append, so a crash mid-append leaves
//! a torn tail that [`UndoLog::open`] detects and truncates — the log never yields a half record.
//!
//! The checksum is torn-write detection on the node's own disk, not authentication: nothing read from a peer is ever
//! stored here unverified.

use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const MAGIC: u32 = 0x554e_444f; // "UNDO"

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoRecord {
    /// Monotone within one log.
    pub seq: u64,
    /// The block (or other unit) this record reverses.
    pub id: Vec<u8>,
    pub payload: Vec<u8>,
}

pub struct UndoLog {
    file: File,
    path: PathBuf,
    /// Offsets of every complete record, oldest first, and the end offset.
    offsets: Vec<u64>,
    end: u64,
    next_seq: u64,
    torn_bytes_dropped: u64,
}

pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn encode_body(seq: u64, id: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + 8 + 4 + id.len() + 4 + payload.len());
    body.extend_from_slice(&MAGIC.to_le_bytes());
    body.extend_from_slice(&seq.to_le_bytes());
    body.extend_from_slice(&(id.len() as u32).to_le_bytes());
    body.extend_from_slice(id);
    body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    body.extend_from_slice(payload);
    body
}

fn take<'a>(body: &'a [u8], at: &mut usize, n: usize) -> Option<&'a [u8]> {
    let s = body.get(*at..at.checked_add(n)?)?;
    *at += n;
    Some(s)
}

fn decode_body(body: &[u8]) -> Option<UndoRecord> {
    let mut at = 0;
    if u32::from_le_bytes(take(body, &mut at, 4)?.try_into().ok()?) != MAGIC {
        return None;
    }
    let seq = u64::from_le_bytes(take(body, &mut at, 8)?.try_into().ok()?);
    let id_len = u32::from_le_bytes(take(body, &mut at, 4)?.try_into().ok()?) as usize;
    let id = take(body, &mut at, id_len)?.to_vec();
    let p_len = u32::from_le_bytes(take(body, &mut at, 4)?.try_into().ok()?) as usize;
    let payload = take(body, &mut at, p_len)?.to_vec();
    (at == body.len()).then_some(UndoRecord { seq, id, payload })
}

impl UndoLog {
    /// Open (creating if absent) and scan. A torn or corrupt tail is truncated away and reported by
    /// [`Self::torn_bytes_dropped`].
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path)?;
        let file_len = file.metadata()?.len();
        // Streamed scan: one record in memory at a time, whatever the log's size.
        let mut reader = io::BufReader::new(&mut file);
        let mut offsets = Vec::new();
        let mut at = 0u64;
        let mut next_seq = 0;
        loop {
            let mut len_bytes = [0u8; 4];
            if reader.read_exact(&mut len_bytes).is_err() {
                break;
            }
            let len = u32::from_le_bytes(len_bytes) as u64;
            if at + 4 + len + 8 > file_len {
                break;
            }
            let mut body = vec![0u8; len as usize];
            let mut sum = [0u8; 8];
            if reader.read_exact(&mut body).is_err() || reader.read_exact(&mut sum).is_err() {
                break;
            }
            if u64::from_le_bytes(sum) != fnv1a64(&body) {
                break;
            }
            let Some(rec) = decode_body(&body) else { break };
            if rec.seq != next_seq {
                break; // a gap is corruption, never a record to trust
            }
            next_seq += 1;
            offsets.push(at);
            at += 4 + len + 8;
        }
        drop(reader);
        let end = at;
        let torn = file_len - end;
        if torn > 0 {
            file.set_len(end)?;
            file.sync_all()?;
        }
        Ok(Self { file, path, offsets, end, next_seq, torn_bytes_dropped: torn })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn len(&self) -> usize {
        self.offsets.len()
    }
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    pub fn bytes(&self) -> u64 {
        self.end
    }
    pub fn torn_bytes_dropped(&self) -> u64 {
        self.torn_bytes_dropped
    }

    /// Append and fsync. Returns the record's sequence number.
    pub fn append(&mut self, id: &[u8], payload: &[u8]) -> io::Result<u64> {
        let seq = self.next_seq;
        let body = encode_body(seq, id, payload);
        let mut frame = Vec::with_capacity(body.len() + 12);
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(&body);
        frame.extend_from_slice(&fnv1a64(&body).to_le_bytes());
        self.file.seek(SeekFrom::Start(self.end))?;
        self.file.write_all(&frame)?;
        self.file.sync_data()?;
        self.offsets.push(self.end);
        self.end += frame.len() as u64;
        self.next_seq += 1;
        Ok(seq)
    }

    /// Append only the first `keep` bytes of a frame — a simulated crash mid-append, for recovery tests.
    #[doc(hidden)]
    pub fn append_torn_for_test(&mut self, id: &[u8], payload: &[u8], keep: usize) -> io::Result<()> {
        let body = encode_body(self.next_seq, id, payload);
        let mut frame = Vec::new();
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(&body);
        frame.extend_from_slice(&fnv1a64(&body).to_le_bytes());
        self.file.seek(SeekFrom::Start(self.end))?;
        self.file.write_all(&frame[..keep.min(frame.len())])?;
        self.file.sync_data()
    }

    fn read_at(&mut self, index: usize) -> io::Result<UndoRecord> {
        let start = self.offsets[index];
        let stop = self.offsets.get(index + 1).copied().unwrap_or(self.end);
        let mut frame = vec![0u8; (stop - start) as usize];
        self.file.seek(SeekFrom::Start(start))?;
        self.file.read_exact(&mut frame)?;
        let len = u32::from_le_bytes(frame[..4].try_into().unwrap()) as usize;
        decode_body(&frame[4..4 + len]).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "undo record changed on disk"))
    }

    /// The newest record, without removing it.
    pub fn last(&mut self) -> io::Result<Option<UndoRecord>> {
        if self.offsets.is_empty() { Ok(None) } else { self.read_at(self.offsets.len() - 1).map(Some) }
    }

    /// Remove and return the newest record (truncate + fsync). The caller applies the returned undo BEFORE popping the
    /// next one; a crash between the caller's apply and this truncate replays the same undo, so undo payloads must be
    /// idempotent "restore old value" records — which old-row images are.
    pub fn pop(&mut self) -> io::Result<Option<UndoRecord>> {
        let Some(rec) = self.last()? else { return Ok(None) };
        let start = self.offsets.pop().unwrap();
        self.file.set_len(start)?;
        self.file.sync_all()?;
        self.end = start;
        self.next_seq -= 1;
        Ok(Some(rec))
    }

    /// Every record, oldest first.
    pub fn records(&mut self) -> io::Result<Vec<UndoRecord>> {
        (0..self.offsets.len()).map(|i| self.read_at(i)).collect()
    }
}
