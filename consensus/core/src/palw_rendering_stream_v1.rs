//! Restart-safe bounded streaming of the EXISTING token-table leaf and constraint transition.
//! This primitive is not a conviction door: its caller must authenticate the job/table,
//! bind the initial state to the already authenticated prefix, and own the reservation.
use crate::Hash64;
use crate::palw_decode_constraint_v1::{PalwConstraintStateV1, PalwDecodeConstraintV1, constraint_id_v1, constraint_step_v1};
use crate::palw_fp_constraint_job_v1::{
    PALW_TOKEN_TABLE_DOMAIN_LEAF_V1, PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1, PALW_TOKEN_TABLE_MAX_LEAVES_V1,
};
use crate::palw_step_leg::{PalwStepOpeningV1, step_opening_root_capped_v1};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_RENDERING_STREAM_PART_BYTES_V1: usize = 65_536;
const IV: [u64; 8] = [
    0x6A09E667F3BCC908,
    0xBB67AE8584CAA73B,
    0x3C6EF372FE94F82B,
    0xA54FF53A5F1D36F1,
    0x510E527FADE682D1,
    0x9B05688C2B3E6C1F,
    0x1F83D9ABFB41BD6B,
    0x5BE0CD19137E2179,
];

// Portable BLAKE2b compression ported from blake2b_simd1.0.2 src/portable.rs,
// copyright Jack O'Connor, licensed MIT (notice in rendering-stream-upstream/LICENSE-MIT).
// Parameter initialization, counters and last-block handling are covered by the keyed V1-leaf
// differential tests below against that same locked library's public API.
const SIGMA: [[u8; 16]; 12] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
];
fn mix(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}
fn compress(block: &[u8; 128], words: &mut [u64; 8], count: u128, last: bool) {
    let mut v = [0u64; 16];
    v[..8].copy_from_slice(words);
    v[8..].copy_from_slice(&IV);
    v[12] ^= count as u64;
    v[13] ^= (count >> 64) as u64;
    if last {
        v[14] ^= u64::MAX;
    }
    let mut m = [0u64; 16];
    for (word, bytes) in m.iter_mut().zip(block.chunks_exact(8)) {
        *word = u64::from_le_bytes(bytes.try_into().unwrap());
    }
    for s in SIGMA {
        mix(&mut v, 0, 4, 8, 12, m[s[0] as usize], m[s[1] as usize]);
        mix(&mut v, 1, 5, 9, 13, m[s[2] as usize], m[s[3] as usize]);
        mix(&mut v, 2, 6, 10, 14, m[s[4] as usize], m[s[5] as usize]);
        mix(&mut v, 3, 7, 11, 15, m[s[6] as usize], m[s[7] as usize]);
        mix(&mut v, 0, 5, 10, 15, m[s[8] as usize], m[s[9] as usize]);
        mix(&mut v, 1, 6, 11, 12, m[s[10] as usize], m[s[11] as usize]);
        mix(&mut v, 2, 7, 8, 13, m[s[12] as usize], m[s[13] as usize]);
        mix(&mut v, 3, 4, 9, 14, m[s[14] as usize], m[s[15] as usize]);
    }
    for i in 0..8 {
        words[i] ^= v[i] ^ v[i + 8];
    }
}

/// Serializes the actual compression continuation, never prior rendering bytes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct LeafHashContinuationV1 {
    words: [u64; 8],
    compressed: u128,
    buffer: Vec<u8>,
}
impl LeafHashContinuationV1 {
    pub(crate) fn new(key: &[u8]) -> Self {
        assert!(key.len() <= 64);
        let mut words = IV;
        words[0] ^= 0x01010000 ^ ((key.len() as u64) << 8) ^ 64;
        let mut buffer = vec![0; 128];
        buffer[..key.len()].copy_from_slice(key);
        Self { words, compressed: 0, buffer }
    }
    pub(crate) fn update(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            if self.buffer.len() == 128 {
                compress(self.buffer.as_slice().try_into().expect("full block"), &mut self.words, self.compressed + 128, false);
                self.compressed += 128;
                self.buffer.clear();
            }
            let n = (128 - self.buffer.len()).min(bytes.len());
            self.buffer.extend_from_slice(&bytes[..n]);
            bytes = &bytes[n..];
        }
    }
    pub(crate) fn byte_count(&self) -> Option<u128> {
        (self.buffer.len() <= 128 && self.compressed.is_multiple_of(128)).then(|| self.compressed + self.buffer.len() as u128)
    }
    pub(crate) fn finish(&self) -> Hash64 {
        let mut words = self.words;
        let mut block = [0u8; 128];
        block[..self.buffer.len()].copy_from_slice(&self.buffer);
        compress(&block, &mut words, self.compressed + self.buffer.len() as u128, true);
        let mut out = [0u8; 64];
        for (part, word) in out.chunks_exact_mut(8).zip(words) {
            part.copy_from_slice(&word.to_le_bytes());
        }
        Hash64::from_bytes(out)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRenderingStreamV1 {
    context: Hash64,
    constraint_id: Hash64,
    id: u32,
    present: bool,
    length: u32,
    offset: u32,
    expected_leaf: Hash64,
    hash: LeafHashContinuationV1,
    /// Tentative state: inaccessible until the legacy leaf's exact bytes authenticate.
    next: Option<PalwConstraintStateV1>,
}
impl PalwRenderingStreamV1 {
    /// `context` is the caller's authenticated claim/job/position/prefix identity. This
    /// initializer does not prove `initial` by itself; only the surrounding prefix court can.
    pub fn new(
        context: Hash64,
        constraint: &PalwDecodeConstraintV1,
        initial: Option<PalwConstraintStateV1>,
        table_root: Hash64,
        vocab: u32,
        id: u32,
        present: bool,
        length: u32,
        opening: &PalwStepOpeningV1,
    ) -> Result<Self, String> {
        if id >= vocab
            || opening.leaf_index != u64::from(id)
            || u64::from(length) > PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1 as u64
            || (!present && length != 0)
        {
            return Err("rendering stream is outside the old table envelope".into());
        }
        let root =
            step_opening_root_capped_v1(u64::from(vocab) + 1, opening, PALW_TOKEN_TABLE_MAX_LEAVES_V1).map_err(|e| e.to_string())?;
        if root != table_root {
            return Err("rendering opening does not authenticate the existing table root".into());
        }
        let mut hash = LeafHashContinuationV1::new(PALW_TOKEN_TABLE_DOMAIN_LEAF_V1);
        hash.update(&id.to_le_bytes());
        hash.update(&[u8::from(present)]);
        if present {
            hash.update(&length.to_le_bytes());
        }
        let next = if present && length != 0 { initial } else { None };
        Ok(Self {
            context,
            constraint_id: constraint_id_v1(&constraint.to_bytes()),
            id,
            present,
            length,
            offset: 0,
            expected_leaf: opening.leaf_hash,
            hash,
            next,
        })
    }
    pub fn offset(&self) -> u32 {
        self.offset
    }
    pub fn check(&self) -> Result<(), String> {
        let header = if self.present { 9u128 } else { 5 };
        if self.hash.buffer.len() > 128
            || !self.hash.compressed.is_multiple_of(128)
            || self.offset > self.length
            || u64::from(self.length) > PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1 as u64
            || (!self.present && self.length != 0)
            || self.hash.compressed + self.hash.buffer.len() as u128 != 128 + header + u128::from(self.offset)
        {
            return Err("rendering continuation has inconsistent bounded counters".into());
        }
        Ok(())
    }
    /// Exact contiguous moves only. Hostile offsets, lengths or constraints fail before mutation.
    pub fn push(&mut self, context: Hash64, constraint: &PalwDecodeConstraintV1, offset: u32, bytes: &[u8]) -> Result<(), String> {
        if constraint_id_v1(&constraint.to_bytes()) != self.constraint_id {
            return Err("rendering part changes the canonical constraint".into());
        }
        self.push_owned(context, constraint, offset, bytes)
    }
    /// Outer rooted court owns the immutable constraint; avoid rehashing its 256KiB bytes
    /// on every move. This private-to-crate path is never exposed to a wire caller.
    pub(crate) fn push_owned(
        &mut self,
        context: Hash64,
        constraint: &PalwDecodeConstraintV1,
        offset: u32,
        bytes: &[u8],
    ) -> Result<(), String> {
        self.check()?;
        if context != self.context
            || offset != self.offset
            || bytes.is_empty()
            || bytes.len() > PALW_RENDERING_STREAM_PART_BYTES_V1
            || bytes.len() as u64 > u64::from(self.length - self.offset)
        {
            return Err("rendering part changes identity, skips/repeats bytes, or exceeds the bounded move".into());
        }
        self.hash.update(bytes);
        if let Some(state) = self.next.as_mut() {
            for byte in bytes {
                if !constraint_step_v1(constraint, state, *byte) {
                    self.next = None;
                    break;
                }
            }
        }
        self.offset += bytes.len() as u32;
        self.check()
    }
    /// None means a fully authenticated non-admitted rendering; incomplete/mismatched bytes
    /// are errors and cannot be confused with a semantic rejection or conviction.
    pub fn finish(&self) -> Result<Option<PalwConstraintStateV1>, String> {
        self.check()?;
        if self.offset != self.length {
            return Err("rendering bytes are not complete".into());
        }
        if self.hash.finish() != self.expected_leaf {
            return Err("rendering bytes do not open the legacy table leaf".into());
        }
        Ok(self.next.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_decode_constraint_v1::*;
    use crate::palw_fp_constraint_job_v1::*;
    fn grammar() -> PalwDecodeConstraintV1 {
        PalwDecodeConstraintV1 {
            version: 2,
            compiler_id: Hash64::default(),
            start_frame: 0,
            frames: vec![PalwConstraintFrameV1 {
                start: 0,
                nodes: vec![PalwConstraintNodeV1 {
                    accepting: true,
                    edges: vec![PalwConstraintEdgeV1 { lo: b'a', hi: b'a', action: PalwConstraintActionV1::Goto(0) }],
                }],
            }],
        }
    }
    fn stream(bytes: Option<&[u8]>) -> PalwRenderingStreamV1 {
        let table = PalwTokenTableV1 {
            entries: vec![bytes.map(<[u8]>::to_vec), Some(vec![b'x'])],
            eog_token_ids: vec![1],
            tokenizer_id: Hash64::default(),
        };
        let opening = table.opening_of(0).unwrap();
        let c = grammar();
        PalwRenderingStreamV1::new(
            Hash64::default(),
            &c,
            Some(constraint_start_state_v1(&c)),
            table.root(),
            table.vocab(),
            0,
            bytes.is_some(),
            bytes.map_or(0, |b| b.len() as u32),
            &opening.opening,
        )
        .unwrap()
    }
    #[test]
    fn legacy_leaf_hash_matches_library_at_all_block_boundaries_and_restart() {
        let c = grammar();
        for n in [0, 1, 118, 119, 120, 127, 128, 129, 65_536, 65_537, 3_000_000] {
            let bytes = vec![b'a'; n];
            let mut s = stream(Some(&bytes));
            assert!(borsh::to_vec(&s).unwrap().len() < 1024);
            for part in bytes.chunks(PALW_RENDERING_STREAM_PART_BYTES_V1) {
                s.push(Hash64::default(), &c, s.offset(), part).unwrap();
                let encoded = borsh::to_vec(&s).unwrap();
                assert!(encoded.len() < 1024);
                s = borsh::from_slice(&encoded).unwrap();
                s.check().unwrap();
            }
            assert_eq!(
                s.finish().unwrap(),
                if n == 0 { None } else { constraint_admits_v1(&c, &constraint_start_state_v1(&c), &bytes) }
            );
        }
        assert_eq!(stream(None).finish().unwrap(), None);
    }
    #[test]
    fn continuation_matches_upstream_keyed_hash_for_every_short_length_and_split() {
        for n in 0..=512 {
            let bytes: Vec<u8> = (0..n).map(|i| ((i * 131 + n * 17) % 256) as u8).collect();
            let expected = table_leaf_hash_v1(17, Some(&bytes));
            for cut in [0, n / 2, n] {
                let mut hash = LeafHashContinuationV1::new(PALW_TOKEN_TABLE_DOMAIN_LEAF_V1);
                hash.update(&17u32.to_le_bytes());
                hash.update(&[1]);
                hash.update(&(n as u32).to_le_bytes());
                hash.update(&bytes[..cut]);
                let checkpoint = borsh::to_vec(&hash).unwrap();
                let mut hash: LeafHashContinuationV1 = borsh::from_slice(&checkpoint).unwrap();
                hash.update(&bytes[cut..]);
                assert_eq!(hash.finish(), expected, "length {n}, cut {cut}");
            }
        }
    }

    #[test]
    fn incomplete_corrupted_and_foreign_chunks_never_expose_a_transition() {
        let c = grammar();
        let mut s = stream(Some(b"aaaa"));
        let before = s.clone();
        assert!(s.finish().is_err());
        assert!(s.push(Hash64::from_u64_word(1), &c, 0, b"a").is_err());
        assert_eq!(s, before);
        assert!(s.push(Hash64::default(), &c, 1, b"a").is_err());
        assert_eq!(s, before);
        s.push(Hash64::default(), &c, 0, b"aaxa").unwrap();
        assert!(s.finish().is_err());
        let mut authentic_dead = stream(Some(b"aaxa"));
        authentic_dead.push(Hash64::default(), &c, 0, b"aaxa").unwrap();
        assert_eq!(authentic_dead.finish().unwrap(), None);
    }
}
