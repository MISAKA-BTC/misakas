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

/// Serializes the actual compression continuation, never prior rendering bytes. The locked
/// library's public portable compression primitive supplies all BLAKE2 rounds and flags.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct LeafHashContinuationV1 {
    words: [u64; 8],
    compressed: u128,
    buffer: Vec<u8>,
}
impl LeafHashContinuationV1 {
    fn new(key: &[u8]) -> Self {
        assert!(key.len() <= 64);
        let mut words = IV;
        words[0] ^= 0x01010000 ^ ((key.len() as u64) << 8) ^ 64;
        let mut buffer = vec![0; 128];
        buffer[..key.len()].copy_from_slice(key);
        Self { words, compressed: 0, buffer }
    }
    fn update(&mut self, mut bytes: &[u8]) {
        use blake2b_simd::guts::{Finalize, Implementation, LastNode, Stride};
        while !bytes.is_empty() {
            if self.buffer.len() == 128 {
                Implementation::portable().compress1_loop(
                    &self.buffer,
                    &mut self.words,
                    self.compressed,
                    LastNode::No,
                    Finalize::No,
                    Stride::Serial,
                );
                self.compressed += 128;
                self.buffer.clear();
            }
            let n = (128 - self.buffer.len()).min(bytes.len());
            self.buffer.extend_from_slice(&bytes[..n]);
            bytes = &bytes[n..];
        }
    }
    fn finish(&self) -> Hash64 {
        use blake2b_simd::guts::{Finalize, Implementation, LastNode, Stride};
        let mut words = self.words;
        Implementation::portable().compress1_loop(
            &self.buffer,
            &mut words,
            self.compressed,
            LastNode::No,
            Finalize::Yes,
            Stride::Serial,
        );
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
            || self.hash.compressed % 128 != 0
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
        self.check()?;
        if context != self.context
            || constraint_id_v1(&constraint.to_bytes()) != self.constraint_id
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
