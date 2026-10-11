//! Bounded tokenizer/table derivation envelope. The caller must store this row under the
//! literal claim/reservation identity; a supplied serialized row is never a court credential.
//! Neither JSON output nor a table rendering is exposed before both original flat hashes finish.
use crate::Hash64;
use crate::palw_fp_constraint_job_v1::{
    PALW_TOKEN_TABLE_DOMAIN_LEAF_V1, PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1, PALW_TOKEN_TABLE_MAX_LEAVES_V1,
};
use crate::palw_json_key_set_v1::PalwJsonKeyCompactProofV1;
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_step_leg::{PalwStepOpeningV1, step_opening_root_capped_v1};
use crate::palw_tokenizer_json_stream_v1::{
    PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1, PalwTokenizerJsonStreamV1, PalwTokenizerJsonVerdictV1, PalwTokenizerRenderingDigestV1,
};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAuthenticatedRenderingDigestStreamV1 {
    context: Hash64,
    present: bool,
    length: u32,
    offset: u32,
    expected_leaf: Hash64,
    leaf: LeafHashContinuationV1,
    rendering: LeafHashContinuationV1,
}
impl PalwAuthenticatedRenderingDigestStreamV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        context: Hash64,
        root: Hash64,
        vocab: u32,
        id: u32,
        present: bool,
        length: u32,
        opening: &PalwStepOpeningV1,
    ) -> Result<Self, String> {
        if id >= vocab
            || opening.leaf_index != u64::from(id)
            || (!present && length != 0)
            || u64::from(length) > PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1 as u64
            || step_opening_root_capped_v1(u64::from(vocab) + 1, opening, PALW_TOKEN_TABLE_MAX_LEAVES_V1).map_err(|e| e.to_string())?
                != root
        {
            return Err("rendering digest seed does not open the original table envelope".into());
        }
        let mut leaf = LeafHashContinuationV1::new(PALW_TOKEN_TABLE_DOMAIN_LEAF_V1);
        leaf.update(&id.to_le_bytes());
        leaf.update(&[u8::from(present)]);
        if present {
            leaf.update(&length.to_le_bytes());
        }
        Ok(Self {
            context,
            present,
            length,
            offset: 0,
            expected_leaf: opening.leaf_hash,
            leaf,
            rendering: LeafHashContinuationV1::new(PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1),
        })
    }
    pub fn offset(&self) -> u32 {
        self.offset
    }
    pub fn check(&self) -> Result<(), String> {
        let header = if self.present { 9 } else { 5 };
        if self.offset > self.length
            || (!self.present && self.length != 0)
            || u64::from(self.length) > PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1 as u64
            || self.leaf.byte_count() != Some(128 + header + u128::from(self.offset))
            || self.rendering.byte_count() != Some(128 + u128::from(self.offset))
        {
            return Err("rendering digest continuation has inconsistent bounded counters".into());
        }
        Ok(())
    }
    pub fn push(&mut self, context: Hash64, offset: u32, bytes: &[u8]) -> Result<(), String> {
        self.check()?;
        if context != self.context
            || offset != self.offset
            || bytes.is_empty()
            || bytes.len() > 65536
            || bytes.len() as u64 > u64::from(self.length - self.offset)
        {
            return Err("rendering digest move changes identity/order or exceeds bounded work".into());
        }
        self.leaf.update(bytes);
        self.rendering.update(bytes);
        self.offset += bytes.len() as u32;
        self.check()
    }
    pub fn finish(&self) -> Result<Option<PalwTokenizerRenderingDigestV1>, String> {
        self.check()?;
        if self.offset != self.length || self.leaf.finish() != self.expected_leaf {
            return Err("rendering digest is incomplete or does not open the legacy leaf".into());
        }
        Ok(self.present.then(|| PalwTokenizerRenderingDigestV1 { length: u64::from(self.length), digest: self.rendering.finish() }))
    }
}

/// One carrier can cross many parser pauses. All proofs must be consumed in the exact parser
/// order; unused proof bytes and oversize packets refuse atomically.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTokenizerStreamPacketV1 {
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub proofs: Vec<PalwJsonKeyCompactProofV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTokenizerTableStreamV1 {
    context: Hash64,
    signed_key_bound: u64,
    parser: PalwTokenizerJsonStreamV1,
    table: PalwAuthenticatedRenderingDigestStreamV1,
}
impl PalwTokenizerTableStreamV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        context: Hash64,
        target: &crate::palw_offence_attribution_v1::PalwOffenceTargetV1,
        binding: &crate::palw_step_leg::PalwStepBindingV2,
        job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        id: u32,
        present: bool,
        length: u32,
        opening: &PalwStepOpeningV1,
    ) -> Result<Self, String> {
        crate::palw_offence_attribution_v1::palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
        let registered = target
            .registered_semantics
            .as_ref()
            .filter(|r| r.matches_context(&binding.job_context))
            .ok_or("tokenizer stream has no matching immutable claim semantics")?;
        let (Some(root), Some(rule), Some(raw_length), Some(bound)) =
            (registered.token_table_root, registered.rendering_rule, registered.raw_tokenizer_bytes, registered.tokenizer_key_events)
        else {
            return Err("historical unqualified tokenizer cannot enter prospective stream court".into());
        };
        match rule {
            crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1 => (),
        }
        if crate::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job) != Some(root) {
            return Err("tokenizer stream table is not the immutable registered root".into());
        }
        Ok(Self {
            context,
            signed_key_bound: bound,
            parser: PalwTokenizerJsonStreamV1::new(context, registered.tokenizer_id, raw_length, id),
            table: PalwAuthenticatedRenderingDigestStreamV1::new(
                context,
                root,
                binding.shape_profile.vocab_size,
                id,
                present,
                length,
                opening,
            )?,
        })
    }
    pub fn tokenizer_offset(&self) -> u64 {
        self.parser.offset()
    }
    pub fn pending_key(&self) -> Option<(u64, Hash64)> {
        self.parser.pending_key()
    }
    pub fn key_set_root(&self) -> Hash64 {
        self.parser.key_set_root()
    }
    pub fn key_events(&self) -> u64 {
        self.parser.key_events()
    }

    pub fn rendering_offset(&self) -> u32 {
        self.table.offset()
    }
    fn pending_overrun(&mut self) -> Result<(), String> {
        if self.parser.pending_key().is_some() && self.parser.key_events() == self.signed_key_bound {
            self.parser.mark_work_overrun(self.context, self.signed_key_bound)?;
        }
        Ok(())
    }
    pub fn packet(&mut self, context: Hash64, packet: &PalwTokenizerStreamPacketV1) -> Result<(), String> {
        if context != self.context
            || packet.offset != self.parser.offset()
            || packet.bytes.len() > 65536
            || packet.proofs.len() > 64
            || (packet.bytes.is_empty() && packet.proofs.is_empty())
            || borsh::to_vec(packet).map_err(|e| e.to_string())?.len() > 90000
        {
            return Err("tokenizer packet exceeds bounded carrier or changes rooted order/context".into());
        }
        // Fixed-size parser only; no public file, token strings or proof history copied here.
        let previous = self.parser.clone();
        let result = (|| {
            let mut bytes = 0usize;
            let mut proofs = 0usize;
            loop {
                self.pending_overrun()?;
                if self.parser.pending_key().is_some() {
                    if proofs == packet.proofs.len() {
                        break;
                    }
                    self.parser.prove_key(context, &packet.proofs[proofs].expand()?)?;
                    proofs += 1;
                } else {
                    if bytes == packet.bytes.len() {
                        break;
                    }
                    let consumed = self.parser.push(context, self.parser.offset(), &packet.bytes[bytes..])?;
                    if consumed == 0 {
                        return Err("tokenizer stream made no bounded progress".into());
                    }
                    bytes += consumed;
                }
            }
            if bytes != packet.bytes.len() || proofs != packet.proofs.len() {
                return Err("tokenizer packet skips a proof or carries unconsumed bytes/proofs".into());
            }
            Ok(())
        })();
        if result.is_err() {
            self.parser = previous;
        }
        result
    }
    pub fn seal_tokenizer(&mut self, context: Hash64) -> Result<(), String> {
        if context != self.context {
            return Err("tokenizer seal changes context".into());
        }
        self.pending_overrun()?;
        self.parser.seal()
    }
    pub fn rendering_part(&mut self, context: Hash64, offset: u32, bytes: &[u8]) -> Result<(), String> {
        self.table.push(context, offset, bytes)
    }
    /// True is an authenticated rule/table/declared-work violation, not incomplete material.
    pub fn finish(&self) -> Result<bool, String> {
        let actual = self.table.finish()?;
        Ok(match self.parser.finish()? {
            PalwTokenizerJsonVerdictV1::Invalid | PalwTokenizerJsonVerdictV1::WorkBoundExceeded => true,
            PalwTokenizerJsonVerdictV1::Rendering(expected) => expected != actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_fp_constraint_job_v1::PalwTokenTableV1;
    fn digest_stream(bytes: Option<&[u8]>) -> PalwAuthenticatedRenderingDigestStreamV1 {
        let table = PalwTokenTableV1 {
            entries: vec![bytes.map(<[u8]>::to_vec), Some(vec![b'x'])],
            eog_token_ids: vec![1],
            tokenizer_id: Hash64::default(),
        };
        let opening = table.opening_of(0).unwrap();
        PalwAuthenticatedRenderingDigestStreamV1::new(
            Hash64::from_u64_word(18),
            table.root(),
            table.vocab(),
            0,
            bytes.is_some(),
            bytes.map_or(0, |b| b.len()) as u32,
            &opening.opening,
        )
        .unwrap()
    }
    fn derive(raw: &[u8], table_bytes: Option<&[u8]>, bound: u64) -> bool {
        use crate::palw_tokenizer_json_stream_v1::tests::Book;
        let context = Hash64::from_u64_word(18);
        let mut court = PalwTokenizerTableStreamV1 {
            context,
            signed_key_bound: bound,
            parser: PalwTokenizerJsonStreamV1::new(
                context,
                crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(raw),
                raw.len() as u64,
                0,
            ),
            table: digest_stream(table_bytes),
        };
        let mut book = Book::new();
        while court.tokenizer_offset() < raw.len() as u64 || court.pending_key().is_some() {
            if let Some((namespace, key)) = court.pending_key() {
                let proof = book.proof(namespace, key);
                let compact = PalwJsonKeyCompactProofV1::compress(&proof).unwrap();
                let mut corrupt = compact.clone();
                corrupt.siblings.push(Hash64::default());
                let before = court.clone();
                assert!(
                    court
                        .packet(
                            context,
                            &PalwTokenizerStreamPacketV1 { offset: court.tokenizer_offset(), bytes: vec![], proofs: vec![corrupt] }
                        )
                        .is_err()
                );
                assert_eq!(court, before);
                court
                    .packet(
                        context,
                        &PalwTokenizerStreamPacketV1 { offset: court.tokenizer_offset(), bytes: vec![], proofs: vec![compact] },
                    )
                    .unwrap();
                book.insert(namespace, key);
            } else {
                let offset = court.tokenizer_offset();
                court
                    .packet(
                        context,
                        &PalwTokenizerStreamPacketV1 {
                            offset,
                            bytes: raw[offset as usize..(offset as usize + 1).min(raw.len())].to_vec(),
                            proofs: vec![],
                        },
                    )
                    .unwrap();
            }
            let wire = borsh::to_vec(&court).unwrap();
            assert!(wire.len() < 70_000);
            court = borsh::from_slice(&wire).unwrap();
            assert!(court.finish().is_err());
        }
        court.seal_tokenizer(context).unwrap();
        if let Some(bytes) = table_bytes {
            for part in bytes.chunks(65536) {
                court.rendering_part(context, court.rendering_offset(), part).unwrap();
            }
        }
        court.finish().unwrap()
    }
    #[test]
    fn raw_json_and_original_table_authenticate_before_honest_mismatch_duplicate_or_overrun() {
        let raw = br#"{"model":{"vocab":{"A":0,"B":1}},"added_tokens":[]}"#;
        assert!(!derive(raw, Some(b"A"), 512));
        assert!(derive(raw, Some(b"X"), 512));
        assert!(derive(br#"{"model":{"vocab":{"A":0,"A":1}}}"#, Some(b"A"), 512));
        // B=0 cannot skip the file hash: this actual first key event violates the signed contract,
        // then the same full original raw hash must still complete before objective overrun.
        assert!(derive(raw, Some(b"A"), 0));
        assert!(!derive(b"{}", Some(b""), 0));
    }
    #[test]
    fn original_table_leaf_and_rendering_digest_match_at_restart_boundaries() {
        let context = Hash64::from_u64_word(18);
        for n in [0, 1, 118, 119, 127, 128, 129, 65536, 65537, 300000] {
            let bytes = vec![b'a'; n];
            let mut stream = digest_stream(Some(&bytes));
            for chunk in bytes.chunks(65536) {
                stream.push(context, stream.offset(), chunk).unwrap();
                let wire = borsh::to_vec(&stream).unwrap();
                assert!(wire.len() < 1024);
                stream = borsh::from_slice(&wire).unwrap();
            }
            let digest = stream.finish().unwrap().unwrap();
            let reference = blake2b_simd::Params::new().hash_length(64).key(PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1).hash(&bytes);
            assert_eq!(digest.length, n as u64);
            assert_eq!(digest.digest.as_byte_slice(), reference.as_bytes());
        }
        assert_eq!(digest_stream(None).finish().unwrap(), None);
        assert!(digest_stream(Some(&[])).finish().unwrap().is_some());
    }
    #[test]
    fn incomplete_corrupt_or_foreign_table_leaf_never_exposes_digest() {
        let context = Hash64::from_u64_word(18);
        let mut stream = digest_stream(Some(b"abc"));
        assert!(stream.finish().is_err());
        let before = stream.clone();
        assert!(stream.push(Hash64::default(), 0, b"abc").is_err());
        assert!(stream.push(context, 1, b"abc").is_err());
        assert_eq!(stream, before);
        stream.push(context, 0, b"abx").unwrap();
        assert!(stream.finish().is_err());
        let mut changed = before;
        changed.length = 2;
        assert!(changed.finish().is_err());
    }
}
