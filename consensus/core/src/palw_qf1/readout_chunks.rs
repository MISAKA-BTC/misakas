//! QF1_READOUT_CHUNK_V1 compression relation. The caller authenticates the
//! preceding record, expected index and source bytes against the original trace.
use super::{MAX_NEW, ROLES, WIDTH};
use crate::palw_rendering_stream_v1::{IV, compress};

const CHUNK: usize = 4096;
pub(super) const DOMAIN: &[u8] = b"misaka/palw/qf1/readout/v1\0";

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct HashChunk {
    pub index: u32,
    pub consumed: u64,
    pub length: u16,
    pub final_flag: bool,
    pub chain_after: [u8; 64],
}

/// Number of authenticatable compression records at a successful QF1 stop.
pub fn record_count(steps: u32) -> Result<u32, &'static str> {
    if !(1..=MAX_NEW).contains(&steps) {
        return Err("QF1 readout step count");
    }
    Ok((DOMAIN.len() as u32 + steps * (5 + 2 * (ROLES * WIDTH) as u32) + 5).div_ceil(CHUNK as u32))
}

fn initial() -> [u64; 8] {
    let mut state = IV;
    state[0] ^= 0x01010040;
    state
}

fn advance(mut state: [u64; 8], offset: u64, data: &[u8], last: bool) -> [u64; 8] {
    let mut count = offset;
    for part in data.chunks(128) {
        let mut block = [0; 128];
        block[..part.len()].copy_from_slice(part);
        count += part.len() as u64;
        compress(&block, &mut state, u128::from(count), last && count == offset + data.len() as u64);
    }
    state
}
fn encode(state: [u64; 8]) -> [u8; 64] {
    let mut out = [0; 64];
    for (dst, word) in out.chunks_exact_mut(8).zip(state) {
        dst.copy_from_slice(&word.to_le_bytes());
    }
    out
}

impl HashChunk {
    pub fn verify(&self, steps: u32, expected_index: u32, previous: Option<&HashChunk>, source: &[u8]) -> bool {
        if !(1..=MAX_NEW).contains(&steps) || self.index != expected_index {
            return false;
        }
        let total = DOMAIN.len() as u64 + u64::from(steps) * (5 + 2 * (ROLES * WIDTH) as u64) + 5;
        let offset = u64::from(self.index) * CHUNK as u64;
        if offset >= total {
            return false;
        }
        let length = (total - offset).min(CHUNK as u64);
        if self.consumed != offset + length
            || u64::from(self.length) != length
            || source.len() as u64 != length
            || self.final_flag != (self.consumed == total)
        {
            return false;
        }
        let state = match (previous, self.index) {
            (None, 0) => initial(),
            (Some(p), i) if i > 0 && p.index == i - 1 && p.consumed == offset && p.length == CHUNK as u16 && !p.final_flag => {
                std::array::from_fn(|n| u64::from_le_bytes(p.chain_after[n * 8..n * 8 + 8].try_into().unwrap()))
            }
            _ => return false,
        };
        encode(advance(state, offset, source, self.final_flag)) == self.chain_after
    }
}

pub(super) struct ChunkStream {
    state: [u64; 8],
    buffer: Vec<u8>,
    records: Vec<HashChunk>,
}
impl ChunkStream {
    pub(super) fn new() -> Self {
        Self { state: initial(), buffer: DOMAIN.to_vec(), records: Vec::new() }
    }
    pub(super) fn push(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            if self.buffer.len() == CHUNK {
                self.emit(false);
            }
            let n = bytes.len().min(CHUNK - self.buffer.len());
            self.buffer.extend_from_slice(&bytes[..n]);
            bytes = &bytes[n..];
        }
    }
    fn emit(&mut self, last: bool) {
        let index = self.records.len() as u32;
        let offset = u64::from(index) * CHUNK as u64;
        self.state = advance(self.state, offset, &self.buffer, last);
        self.records.push(HashChunk {
            index,
            consumed: offset + self.buffer.len() as u64,
            length: self.buffer.len() as u16,
            final_flag: last,
            chain_after: encode(self.state),
        });
        self.buffer.clear();
    }
    pub(super) fn finish(mut self) -> ([u8; 64], Vec<HashChunk>) {
        self.emit(true);
        (encode(self.state), self.records)
    }
}
