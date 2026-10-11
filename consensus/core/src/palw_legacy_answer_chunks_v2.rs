//! Session-owned carriage of a complete signed LG14-B answer. The certification chunk table
//! is never used: a liable discloser has at most one assembly per demanded claim/unit.
use crate::palw_legacy_held_da_v2::{PalwLegacyHeldAnswerCarriageV2, PalwLegacyHeldUnitV2};
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeMap;

pub const PALW_LEGACY_ANSWER_DECLARED_TAG_V2: u8 = 160;
pub const PALW_LEGACY_ANSWER_CHUNK_TAG_V2: u8 = 161;
pub const PALW_CARRIAGE_LEGACY_ANSWERS_TAIL_V2: u8 = 0xF1;
pub const PALW_LEGACY_ANSWER_CHUNK_BYTES_V2: u32 = 90_000;
pub const PALW_LEGACY_ANSWER_MAX_CHUNKS_V2: u8 = 32;
pub const PALW_LEGACY_ANSWER_DECLARE_CONTEXT_V2: &[u8] = b"misaka-palw/legacy-held-da/v2/declare/mldsa87/v1";
pub const DECLARE_DOMAIN: &[u8] = b"misaka-palw/legacy-held-da/v2/declare-message";
pub const CHUNK_DOMAIN: &[u8] = b"misaka-palw/legacy-held-da/v2/answer-chunk";
pub const ANSWER_DOMAIN: &[u8] = b"misaka-palw/legacy-held-da/v2/assembled-answer";

pub type PalwLegacyAnswerKeyV2 = (Hash64, PalwLegacyHeldUnitV2, PalwBondKeyV2);

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwLegacyAnswerDeclarationV2 {
    pub version: u16,
    pub claim: Hash64,
    pub unit: PalwLegacyHeldUnitV2,
    pub discloser: PalwBondKeyV2,
    pub part_bytes: u32,
    pub total_bytes: u64,
    pub chunk_digests: Vec<Hash64>,
    pub answer_digest: Hash64,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwLegacyAnswerChunkV2 {
    pub claim: Hash64,
    pub unit: PalwLegacyHeldUnitV2,
    pub discloser: PalwBondKeyV2,
    pub answer_digest: Hash64,
    pub index: u8,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPendingLegacyAnswerV2 {
    pub declaration: PalwLegacyAnswerDeclarationV2,
    /// Fixed by the live DA obligation. A declaration never grants more time.
    pub deadline_daa: u64,
    pub parts: BTreeMap<u8, Vec<u8>>,
}

impl PalwLegacyAnswerDeclarationV2 {
    pub fn key(&self) -> PalwLegacyAnswerKeyV2 {
        (self.claim, self.unit, self.discloser)
    }
    pub fn check(&self, maximum: u64) -> Result<(), String> {
        if self.version != 2
            || self.part_bytes == 0
            || self.part_bytes > PALW_LEGACY_ANSWER_CHUNK_BYTES_V2
            || self.total_bytes == 0
            || self.total_bytes > maximum
            || self.chunk_digests.is_empty()
            || self.chunk_digests.len() > PALW_LEGACY_ANSWER_MAX_CHUNKS_V2 as usize
            || self.total_bytes.div_ceil(u64::from(self.part_bytes)) != self.chunk_digests.len() as u64
        {
            return Err("legacy answer declaration exceeds its exact byte/part bounds".into());
        }
        Ok(())
    }
}

impl PalwLegacyAnswerChunkV2 {
    pub fn key(&self) -> PalwLegacyAnswerKeyV2 {
        (self.claim, self.unit, self.discloser)
    }
}

fn digest(domain: &[u8], bytes: &[u8]) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    s.update(&(bytes.len() as u64).to_le_bytes());
    s.update(bytes);
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64-byte hash"))
}

pub fn palw_legacy_answer_chunk_digest_v2(index: u8, bytes: &[u8]) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(CHUNK_DOMAIN).to_state();
    s.update(&[index]);
    s.update(&(bytes.len() as u64).to_le_bytes());
    s.update(bytes);
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64-byte hash"))
}

pub fn palw_legacy_answer_declaration_message_v2(domain: &[u8], declaration: &PalwLegacyAnswerDeclarationV2) -> Hash64 {
    let mut unsigned = declaration.clone();
    unsigned.signature.clear();
    let mut s = blake2b_simd::Params::new().hash_length(64).key(DECLARE_DOMAIN).to_state();
    s.update(&(domain.len() as u32).to_le_bytes());
    s.update(domain);
    s.update(&borsh::to_vec(&unsigned).expect("serializable declaration"));
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64-byte hash"))
}

/// The declared digest covers the original answer's signature as well as its binding and job.
/// The signature on the declaration prevents another carrier from reserving this discloser's row.
pub fn palw_legacy_answer_split_v2(
    answer: &PalwLegacyHeldAnswerCarriageV2,
    part_bytes: u32,
    maximum: u64,
    sign: impl FnOnce(&PalwLegacyAnswerDeclarationV2) -> Result<Vec<u8>, String>,
) -> Result<(PalwLegacyAnswerDeclarationV2, Vec<PalwLegacyAnswerChunkV2>), String> {
    if part_bytes == 0 || part_bytes > PALW_LEGACY_ANSWER_CHUNK_BYTES_V2 {
        return Err("invalid legacy answer chunk size".into());
    }
    // Count the full envelope before allocating its encoding, including the original signature.
    let length = borsh::object_length(answer).map_err(|e| e.to_string())?;
    let count = length.div_ceil(part_bytes as usize);
    if count == 0 || count > PALW_LEGACY_ANSWER_MAX_CHUNKS_V2 as usize || length as u64 > maximum {
        return Err("legacy answer exceeds its bounded multipart carriage".into());
    }
    let bytes = borsh::to_vec(answer).map_err(|e| e.to_string())?;
    let mut declaration = PalwLegacyAnswerDeclarationV2 {
        version: 2,
        claim: answer.claim,
        unit: answer.unit,
        discloser: answer.discloser,
        part_bytes,
        total_bytes: bytes.len() as u64,
        chunk_digests: Vec::new(),
        answer_digest: digest(ANSWER_DOMAIN, &bytes),
        signature: Vec::new(),
    };
    declaration.chunk_digests =
        bytes.chunks(part_bytes as usize).enumerate().map(|(i, bytes)| palw_legacy_answer_chunk_digest_v2(i as u8, bytes)).collect();
    declaration.check(maximum)?;
    declaration.signature = sign(&declaration)?;
    let chunks = bytes
        .chunks(part_bytes as usize)
        .enumerate()
        .map(|(index, bytes)| PalwLegacyAnswerChunkV2 {
            claim: answer.claim,
            unit: answer.unit,
            discloser: answer.discloser,
            answer_digest: declaration.answer_digest,
            index: index as u8,
            bytes: bytes.to_vec(),
        })
        .collect();
    Ok((declaration, chunks))
}

impl PalwPendingLegacyAnswerV2 {
    pub fn check_chunk(&self, chunk: &PalwLegacyAnswerChunkV2) -> Result<(), String> {
        let d = &self.declaration;
        d.check(u64::from(PALW_LEGACY_ANSWER_MAX_CHUNKS_V2) * u64::from(PALW_LEGACY_ANSWER_CHUNK_BYTES_V2))?;
        if chunk.key() != d.key() || chunk.answer_digest != d.answer_digest {
            return Err("legacy answer chunk is another declared claim/unit/discloser/digest".into());
        }
        let Some(expected) = d.chunk_digests.get(chunk.index as usize) else {
            return Err("legacy answer chunk index is outside its declaration".into());
        };
        let first = u64::from(chunk.index) * u64::from(d.part_bytes);
        let len = (d.total_bytes.checked_sub(first).ok_or("chunk starts past answer")?).min(u64::from(d.part_bytes));
        if chunk.bytes.len() as u64 != len || palw_legacy_answer_chunk_digest_v2(chunk.index, &chunk.bytes) != *expected {
            return Err("legacy answer chunk length or digest differs from its signed declaration".into());
        }
        if self.parts.contains_key(&chunk.index) {
            return Err("duplicate legacy answer chunk".into());
        }
        Ok(())
    }

    /// Validate all retained parts, including on carriage load. No unsigned, unchecked part can
    /// survive a restart and become input to the final parser.
    pub fn check(&self, maximum: u64) -> Result<(), String> {
        self.declaration.check(maximum)?;
        if self.parts.len() >= self.declaration.chunk_digests.len() {
            return Err("a complete legacy answer must not remain pending".into());
        }
        let empty = Self { declaration: self.declaration.clone(), deadline_daa: self.deadline_daa, parts: BTreeMap::new() };
        for (&index, bytes) in &self.parts {
            empty.check_chunk(&PalwLegacyAnswerChunkV2 {
                claim: self.declaration.claim,
                unit: self.declaration.unit,
                discloser: self.declaration.discloser,
                answer_digest: self.declaration.answer_digest,
                index,
                bytes: bytes.clone(),
            })?;
        }
        Ok(())
    }

    /// Only the completing part exposes an answer. All indices, lengths and both hash layers
    /// are checked before parsing; the caller still verifies the original signature and court predicate.
    pub fn preview(&self, chunk: &PalwLegacyAnswerChunkV2) -> Result<Option<PalwLegacyHeldAnswerCarriageV2>, String> {
        self.check_chunk(chunk)?;
        if self.parts.len() + 1 != self.declaration.chunk_digests.len() {
            return Ok(None);
        }
        let mut bytes = Vec::with_capacity(usize::try_from(self.declaration.total_bytes).map_err(|_| "answer too large")?);
        for i in 0..self.declaration.chunk_digests.len() {
            let part =
                if i == chunk.index as usize { &chunk.bytes } else { self.parts.get(&(i as u8)).ok_or("missing declared part")? };
            bytes.extend_from_slice(part);
        }
        if bytes.len() as u64 != self.declaration.total_bytes || digest(ANSWER_DOMAIN, &bytes) != self.declaration.answer_digest {
            return Err("assembled legacy answer differs from its signed digest".into());
        }
        let answer: PalwLegacyHeldAnswerCarriageV2 = borsh::from_slice(&bytes).map_err(|e| e.to_string())?;
        if (answer.claim, answer.unit, answer.discloser) != self.declaration.key() {
            return Err("assembled legacy answer belongs to another claim/unit/discloser".into());
        }
        Ok(Some(answer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_legacy_held_da_v2::PalwLegacyHeldAnswerV2;
    use crate::tx::TransactionOutpoint;

    fn answer() -> PalwLegacyHeldAnswerCarriageV2 {
        let binding = crate::palw_checkpoint_court_v1::tests::held_fixture(true, 20, None).binding;
        PalwLegacyHeldAnswerCarriageV2 {
            version: 2,
            claim: Hash64::from_u64_word(100),
            unit: PalwLegacyHeldUnitV2::PromptIds { chunk: 0 },
            binding,
            answer: PalwLegacyHeldAnswerV2::PromptIds { ids: vec![7; 30_000], tree_root: None, siblings: vec![] },
            discloser: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(101), 0)),
            signature: vec![8; 4627],
        }
    }

    #[test]
    fn the_full_32_part_transport_fits_each_carrier_and_refuses_one_byte_less_before_signing() {
        let maximum = u64::from(PALW_LEGACY_ANSWER_CHUNK_BYTES_V2) * u64::from(PALW_LEGACY_ANSWER_MAX_CHUNKS_V2);
        let mut a = answer();
        a.answer = PalwLegacyHeldAnswerV2::PromptIds { ids: Vec::new(), tree_root: None, siblings: Vec::new() };
        let overhead = borsh::object_length(&a).unwrap();
        a.answer = PalwLegacyHeldAnswerV2::PromptIds {
            ids: vec![7; (maximum as usize - overhead) / 4],
            tree_root: None,
            siblings: Vec::new(),
        };
        let length = borsh::object_length(&a).unwrap() as u64;
        assert!(length <= maximum && maximum - length < 4);
        assert!(
            palw_legacy_answer_split_v2(&a, PALW_LEGACY_ANSWER_CHUNK_BYTES_V2, length - 1, |_| {
                panic!("an oversized answer is rejected before signing")
            })
            .is_err()
        );
        let (d, chunks) = palw_legacy_answer_split_v2(&a, PALW_LEGACY_ANSWER_CHUNK_BYTES_V2, maximum, |_| Ok(vec![9; 4627])).unwrap();
        assert_eq!(chunks.len(), PALW_LEGACY_ANSWER_MAX_CHUNKS_V2 as usize);
        assert!(
            borsh::object_length(&crate::palw_state_v2::PalwConsensusObjectV2::LegacyHeldAnswerDeclaredV2 {
                declaration: Box::new(d.clone()),
            })
            .unwrap()
                < crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES
        );
        let mut row = PalwPendingLegacyAnswerV2 { declaration: d, deadline_daa: 1000, parts: BTreeMap::new() };
        for (i, c) in chunks.iter().rev().enumerate() {
            assert!(
                borsh::object_length(&crate::palw_state_v2::PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 {
                    chunk: Box::new(c.clone()),
                })
                .unwrap()
                    < crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES
            );
            let result = row.preview(c).unwrap();
            if i + 1 == chunks.len() {
                assert_eq!(result, Some(a.clone()));
            } else {
                assert!(result.is_none());
                row.parts.insert(c.index, c.bytes.clone());
            }
        }
    }

    #[test]
    fn a_large_answer_is_exposed_only_after_all_authenticated_parts_arrive() {
        let a = answer();
        let (d, chunks) = palw_legacy_answer_split_v2(&a, 90_000, 200_000, |_| Ok(vec![9; 4627])).unwrap();
        assert_eq!(chunks.len(), 2);
        let mut row = PalwPendingLegacyAnswerV2 { declaration: d, deadline_daa: 1_000, parts: BTreeMap::new() };
        // Out-of-order arrival and a persisted partial row are legitimate; partial means no answer.
        assert!(row.preview(&chunks[1]).unwrap().is_none());
        row.parts.insert(1, chunks[1].bytes.clone());
        row.check(200_000).unwrap();
        let row: PalwPendingLegacyAnswerV2 = borsh::from_slice(&borsh::to_vec(&row).unwrap()).unwrap();
        assert_eq!(row.preview(&chunks[0]).unwrap(), Some(a));
        assert!(row.preview(&chunks[1]).is_err(), "duplicates never overwrite a retained part");
    }

    #[test]
    fn foreign_truncated_tampered_and_unbounded_parts_cannot_complete_an_answer() {
        let a = answer();
        let (d, chunks) = palw_legacy_answer_split_v2(&a, 90_000, 200_000, |_| Ok(vec![9; 4627])).unwrap();
        let row = PalwPendingLegacyAnswerV2 { declaration: d.clone(), deadline_daa: 1_000, parts: BTreeMap::new() };
        for mutation in 0..6 {
            let mut c = chunks[0].clone();
            match mutation {
                0 => c.claim = Hash64::from_u64_word(999),
                1 => c.unit = PalwLegacyHeldUnitV2::FpJob,
                2 => c.discloser.0.index += 1,
                3 => c.index = 2,
                4 => {
                    c.bytes.pop();
                }
                _ => c.bytes[0] ^= 1,
            }
            assert!(row.preview(&c).is_err());
        }
        let mut incorrect = row.clone();
        incorrect.declaration.answer_digest = Hash64::from_u64_word(555);
        incorrect.parts.insert(0, chunks[0].bytes.clone());
        let mut last = chunks[1].clone();
        last.answer_digest = incorrect.declaration.answer_digest;
        assert!(incorrect.preview(&last).is_err(), "part hashes alone are not the whole signed answer digest");
        let mut invalid = d.clone();
        invalid.total_bytes = u64::MAX;
        assert!(invalid.check(200_000).is_err());
        invalid = d.clone();
        invalid.chunk_digests.push(Hash64::default());
        assert!(invalid.check(200_000).is_err());
        assert!(palw_legacy_answer_split_v2(&a, 1, 200_000, |_| Ok(vec![])).is_err());
        assert!(palw_legacy_answer_split_v2(&a, 0, 200_000, |_| Ok(vec![])).is_err());
        let domain = Hash64::from_u64_word(50);
        let message = palw_legacy_answer_declaration_message_v2(domain.as_byte_slice(), &d);
        invalid = d;
        invalid.total_bytes += 1;
        assert_ne!(message, palw_legacy_answer_declaration_message_v2(domain.as_byte_slice(), &invalid));
    }
}
