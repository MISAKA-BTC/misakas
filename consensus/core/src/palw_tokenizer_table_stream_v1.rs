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

/// Every prospective FP job answers its immutable registered tokenizer/table
/// statement. Only constrained jobs additionally name that table in their job tail.
pub fn palw_tokenizer_statement_v1<'a>(
    target: &'a crate::palw_offence_attribution_v1::PalwOffenceTargetV1,
    binding: &crate::palw_step_leg::PalwStepBindingV2,
    job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
) -> Result<&'a crate::palw_class_semantics_v1::PalwClassSemanticsV1, String> {
    crate::palw_offence_attribution_v1::palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
    let registered = target
        .registered_semantics
        .as_ref()
        .filter(|r| r.matches_context(&binding.job_context))
        .ok_or("tokenizer stream has no matching immutable claim semantics")?;
    let (Some(root), Some(_), Some(_), Some(_)) =
        (registered.token_table_root, registered.rendering_rule, registered.raw_tokenizer_bytes, registered.tokenizer_key_events)
    else {
        return Err("historical unqualified tokenizer cannot enter prospective stream court".into());
    };
    if job.is_constraint() && crate::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job) != Some(root) {
        return Err("tokenizer stream table is not the immutable registered root".into());
    }
    Ok(registered)
}

/// [`palw_tokenizer_statement_v1`] over a decode view (a TIR claim's, derived natively).
pub fn palw_tokenizer_statement_view_v1<'a>(
    target: &'a crate::palw_offence_attribution_v1::PalwOffenceTargetV1,
    view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
    job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
) -> Result<&'a crate::palw_class_semantics_v1::PalwClassSemanticsV1, String> {
    crate::palw_decode_view_v1::palw_constrained_job_target_view_v1(target, view, job)?;
    let registered = target
        .registered_semantics
        .as_ref()
        .filter(|r| r.matches_context(&view.job_context))
        .ok_or("tokenizer stream has no matching immutable claim semantics")?;
    let (Some(root), Some(_), Some(_), Some(_)) =
        (registered.token_table_root, registered.rendering_rule, registered.raw_tokenizer_bytes, registered.tokenizer_key_events)
    else {
        return Err("historical unqualified tokenizer cannot enter prospective stream court".into());
    };
    if job.is_constraint() && crate::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job) != Some(root) {
        return Err("tokenizer stream table is not the immutable registered root".into());
    }
    Ok(registered)
}

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
    signed_raw_length: u64,
    operand_raw_length: u64,
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
        let signed = target
            .registered_semantics
            .as_ref()
            .and_then(|r| r.raw_tokenizer_bytes)
            .ok_or("tokenizer has no prospective exact raw length")?;
        Self::new_with_operand_length(context, target, binding, job, id, present, length, opening, signed)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_operand_length(
        context: Hash64,
        target: &crate::palw_offence_attribution_v1::PalwOffenceTargetV1,
        binding: &crate::palw_step_leg::PalwStepBindingV2,
        job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        id: u32,
        present: bool,
        length: u32,
        opening: &PalwStepOpeningV1,
        operand_raw_length: u64,
    ) -> Result<Self, String> {
        let registered = palw_tokenizer_statement_v1(target, binding, job)?;
        Self::with_statement_v1(
            context,
            registered,
            binding.shape_profile.vocab_size,
            id,
            present,
            length,
            opening,
            operand_raw_length,
        )
    }
    /// The same court over a decode view (a TIR claim's, behind `palw_staged_tir_route_v1`).
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_operand_length_view_v1(
        context: Hash64,
        target: &crate::palw_offence_attribution_v1::PalwOffenceTargetV1,
        view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
        job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        id: u32,
        present: bool,
        length: u32,
        opening: &PalwStepOpeningV1,
        operand_raw_length: u64,
    ) -> Result<Self, String> {
        let registered = palw_tokenizer_statement_view_v1(target, view, job)?;
        Self::with_statement_v1(context, registered, view.vocab_size, id, present, length, opening, operand_raw_length)
    }
    #[allow(clippy::too_many_arguments)]
    fn with_statement_v1(
        context: Hash64,
        registered: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
        vocab: u32,
        id: u32,
        present: bool,
        length: u32,
        opening: &PalwStepOpeningV1,
        operand_raw_length: u64,
    ) -> Result<Self, String> {
        let (Some(root), Some(rule), Some(raw_length), Some(bound)) =
            (registered.token_table_root, registered.rendering_rule, registered.raw_tokenizer_bytes, registered.tokenizer_key_events)
        else {
            return Err("historical unqualified tokenizer cannot enter prospective stream court".into());
        };
        match rule {
            crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1 => (),
        }
        Ok(Self {
            context,
            signed_key_bound: bound,
            signed_raw_length: raw_length,
            operand_raw_length,
            parser: PalwTokenizerJsonStreamV1::new(context, registered.tokenizer_id, operand_raw_length, id),
            table: PalwAuthenticatedRenderingDigestStreamV1::new(context, root, vocab, id, present, length, opening)?,
        })
    }
    pub(crate) fn same_contract_v1(&self, other: &Self) -> bool {
        (
            self.context,
            self.signed_key_bound,
            self.signed_raw_length,
            self.operand_raw_length,
            self.table.context,
            self.table.present,
            self.table.length,
            self.table.expected_leaf,
        ) == (
            other.context,
            other.signed_key_bound,
            other.signed_raw_length,
            other.operand_raw_length,
            other.table.context,
            other.table.present,
            other.table.length,
            other.table.expected_leaf,
        )
    }
    pub fn check(&self) -> Result<(), String> {
        self.parser.check()?;
        self.table.check()
    }
    pub fn tokenizer_sealed_v1(&self) -> bool {
        self.parser.is_sealed_v1()
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
        let verdict = self.parser.finish()?;
        if self.operand_raw_length != self.signed_raw_length {
            return Ok(true);
        }
        Ok(match verdict {
            PalwTokenizerJsonVerdictV1::Invalid | PalwTokenizerJsonVerdictV1::WorkBoundExceeded => true,
            PalwTokenizerJsonVerdictV1::Rendering(expected) => expected != actual,
        })
    }
}

#[path = "palw_tokenizer_canonical_replay_v1.rs"]
pub(crate) mod canonical_replay;

#[path = "palw_tokenizer_key_producer_v1.rs"]
pub(crate) mod producer;

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
            signed_raw_length: raw.len() as u64,
            operand_raw_length: raw.len() as u64,
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
        let raw = br#"{"model":{"vocab":{"A":0,"B":1},"merges":[]},"added_tokens":[]}"#;
        assert!(!derive(raw, Some(b"A"), 512));
        assert!(derive(raw, Some(b"X"), 512));
        assert!(derive(br#"{"model":{"vocab":{"A":0,"A":1},"merges":[]}}"#, Some(b"A"), 512));
        // B=0 cannot skip the file hash: this actual first key event violates the signed contract,
        // then the same full original raw hash must still complete before objective overrun.
        assert!(derive(raw, Some(b"A"), 0));
        assert!(!derive(br#"{"model":{"vocab":{},"merges":[]}}"#, Some(b""), 512));
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
    // Primitive-only replay invariance, NOT equality of old signed moves,
    // prices, retention, clocks, custody duties or complete-original admission.
    fn replay_fixture(raw: &[u8], table_bytes: &[u8]) -> PalwTokenizerTableStreamV1 {
        let context = Hash64::from_u64_word(18);
        PalwTokenizerTableStreamV1 {
            context,
            signed_key_bound: 512,
            signed_raw_length: raw.len() as u64,
            operand_raw_length: raw.len() as u64,
            parser: PalwTokenizerJsonStreamV1::new(
                context,
                crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(raw),
                raw.len() as u64,
                0,
            ),
            table: digest_stream(Some(table_bytes)),
        }
    }
    fn replay_atoms(
        base: &PalwTokenizerTableStreamV1,
        raw: &[u8],
        bytewise: bool,
    ) -> Result<Vec<PalwTokenizerStreamPacketV1>, String> {
        use crate::palw_tokenizer_json_stream_v1::tests::Book;
        let mut book = Book::new();
        let mut probe = base.clone();
        let mut out = Vec::new();
        while probe.tokenizer_offset() < raw.len() as u64 || probe.pending_key().is_some() {
            let offset = probe.tokenizer_offset();
            let packet = if let Some((namespace, key)) = probe.pending_key() {
                let compact = PalwJsonKeyCompactProofV1::compress(&book.proof(namespace, key))
                    .map_err(|e| format!("plan compact proof: {e}"))?;
                let packet = PalwTokenizerStreamPacketV1 { offset, bytes: vec![], proofs: vec![compact] };
                probe.packet(base.context, &packet).map_err(|e| format!("plan native packet: {e}"))?;
                book.insert(namespace, key);
                out.push(packet);
                continue;
            } else {
                let mut parser = probe.parser.clone();
                let maximum = if bytewise { 1 } else { 65536 };
                let end = (offset as usize + maximum).min(raw.len());
                let consumed =
                    parser.push(base.context, offset, &raw[offset as usize..end]).map_err(|e| format!("plan native parser: {e}"))?;
                if consumed == 0 {
                    return Err("plan native parser made no progress".into());
                }
                PalwTokenizerStreamPacketV1 {
                    offset,
                    bytes: raw[offset as usize..offset as usize + consumed].to_vec(),
                    proofs: vec![],
                }
            };
            probe.packet(base.context, &packet).map_err(|e| format!("plan native packet: {e}"))?;
            out.push(packet);
        }
        Ok(out)
    }
    fn fixture_maximal_packet(atoms: &[PalwTokenizerStreamPacketV1]) -> Vec<PalwTokenizerStreamPacketV1> {
        // This nonempty fixture's ENTIRE authenticated file and every necessary
        // proof fit one legal carrier. Thus this is maximal for these materials,
        // not an asserted general-purpose packet planner or upper-bound closure.
        let mut packet = PalwTokenizerStreamPacketV1 { offset: 0, bytes: vec![], proofs: vec![] };
        for p in atoms {
            packet.bytes.extend_from_slice(&p.bytes);
            packet.proofs.extend(p.proofs.clone());
        }
        assert!(!packet.bytes.is_empty() && !packet.proofs.is_empty());
        assert!(packet.bytes.len() <= 65536 && packet.proofs.len() <= 64);
        assert!(borsh::to_vec(&packet).unwrap().len() <= 90000);
        vec![packet]
    }
    fn run_replay(
        base: &PalwTokenizerTableStreamV1,
        packets: &[PalwTokenizerStreamPacketV1],
        rendering: &[u8],
        small: bool,
    ) -> Result<(bool, Option<PalwTokenizerRenderingDigestV1>, u64, PalwTokenizerTableStreamV1), String> {
        let mut c = base.clone();
        for p in packets {
            c.packet(base.context, p)?;
        }
        c.seal_tokenizer(base.context)?;
        let chunk = if small { 1 } else { 65536 };
        for part in rendering.chunks(chunk) {
            c.rendering_part(base.context, c.rendering_offset(), part)?;
        }
        Ok((c.finish()?, c.table.finish()?, c.key_events(), c))
    }
    #[test]
    fn canonical_material_replay_matches_distinct_legal_packetizations() {
        let raw = br#"{"model":{"vocab":{"A":0,"B":1},"merges":[]},"added_tokens":[]}"#;
        for table_bytes in [b"A".as_slice(), b"X".as_slice()] {
            let base = replay_fixture(raw, table_bytes);
            let atoms = replay_atoms(&base, raw, false).unwrap();
            let canonical = fixture_maximal_packet(&atoms);
            let alternative = replay_atoms(&base, raw, true).unwrap();
            assert!(alternative.iter().any(|p| p.bytes.is_empty() && !p.proofs.is_empty()));
            assert!(alternative.len() > canonical.len());
            let a = run_replay(&base, &canonical, table_bytes, false).unwrap();
            let b = run_replay(&base, &alternative, table_bytes, true).unwrap();
            assert_eq!(a.0, b.0);
            assert_eq!(a.1, b.1);
            assert_eq!(a.2, b.2);
            assert!(a.2 > 0);
            assert_eq!(a.0, table_bytes != b"A");
            assert!(a.3.same_contract_v1(&b.3) && a.3.same_contract_v1(&base));
            for mut packets in [canonical.clone(), alternative.clone()] {
                let p = packets.iter_mut().find(|p| !p.proofs.is_empty()).unwrap();
                p.proofs[0].siblings.push(Hash64::default());
                assert!(run_replay(&base, &packets, table_bytes, false).is_err());
            }
            assert!(run_replay(&base, &canonical, b"Q", false).is_err());
            assert!(run_replay(&base, &alternative, b"Q", true).is_err());
        }
        let base = replay_fixture(raw, b"A");
        let mut unauthenticated = raw.to_vec();
        let i = unauthenticated.iter().position(|b| *b == b'B').unwrap();
        unauthenticated[i] = b'C';
        for small in [false, true] {
            match replay_atoms(&base, &unauthenticated, small) {
                Err(stage) => assert!(stage.starts_with("plan "), "native planning refusal must be explicit: {stage}"),
                Ok(atoms) => {
                    let packets = if small { atoms } else { fixture_maximal_packet(&atoms) };
                    assert!(
                        run_replay(&base, &packets, b"A", small).is_err(),
                        "unauthenticated raw must never expose a semantic result"
                    );
                }
            }
        }
    }

    #[test]
    fn private_tokenizer_canonical_builder_consumes_real_ordered_proofs_and_authenticates_operands() {
        use super::canonical_replay::{self as replay, Budget, Material, Refusal};
        let budget = Budget { max_material_bytes: 1 << 25, max_proofs: 1000, max_native_calls: 100_000, max_packets: 10_000 };
        let mut vocab = serde_json::Map::new();
        vocab.insert("A".into(), serde_json::json!(0));
        for id in 1..100u32 {
            vocab.insert(format!("B{}{:04}", "a".repeat(1024), id), serde_json::json!(id));
        }
        let raw = serde_json::to_vec(&serde_json::json!({"model":{"vocab":vocab,"merges":[]},"added_tokens":[]})).unwrap();
        let base = replay_fixture(&raw, b"A");
        let saved = borsh::to_vec(&base).unwrap();
        let atoms = replay_atoms(&base, &raw, false).unwrap();
        let proofs: Vec<_> = atoms.iter().flat_map(|p| p.proofs.clone()).collect();
        assert!(proofs.len() > 64 && raw.len() > 65536);
        let input = Material { raw: &raw, proofs: &proofs, rendering: Some(b"A") };
        let got = replay::build(&base, input, budget).unwrap();
        let direct = run_replay(&base, &atoms, b"A", false).unwrap();
        assert_eq!(got.violation(), direct.0);
        assert!(!got.violation());
        assert_eq!(got.key_events(), direct.2);
        assert_eq!(got.key_root(), direct.3.key_set_root());
        assert!(got.work().packets >= 2);
        assert_eq!(got.work().proofs, proofs.len() as u64);
        assert_eq!(borsh::to_vec(&base).unwrap(), saved);
        let missing = Material { proofs: &proofs[..proofs.len() - 1], ..input };
        assert!(matches!(replay::build(&base, missing, budget), Err(Refusal::NeedProof { .. })));
        let mut extra = proofs.clone();
        extra.push(proofs[0].clone());
        assert!(matches!(replay::build(&base, Material { proofs: &extra, ..input }, budget), Err(Refusal::ExtraProof { .. })));
        let mut foreign = proofs.clone();
        foreign[0].namespace ^= 1;
        assert!(matches!(replay::build(&base, Material { proofs: &foreign, ..input }, budget), Err(Refusal::Native(_))));
        let mut badraw = raw.clone();
        let at = badraw.iter().position(|b| *b == b'A').unwrap();
        badraw[at] = b'X';
        assert!(replay::build(&base, Material { raw: &badraw, ..input }, budget).is_err());
        assert!(matches!(replay::build(&base, Material { raw: &raw[..raw.len() - 1], ..input }, budget), Err(Refusal::OperandLength)));
        assert!(matches!(replay::build(&base, Material { rendering: None, ..input }, budget), Err(Refusal::RenderingOperand)));
        assert!(matches!(replay::build(&base, input, Budget { max_packets: got.work().packets - 1, ..budget }), Err(Refusal::Budget)));
        assert_eq!(borsh::to_vec(&base).unwrap(), saved);
    }

    fn data_producer_budget() -> producer::Budget {
        producer::Budget {
            max_proofs: 1000,
            max_nodes: 513000,
            max_requests: 1 << 30,
            max_hash_calls: 1000000,
            max_hash_bytes: 1 << 30,
        }
    }
    #[test]
    fn production_key_material_is_data_identical_across_native_contexts_and_rechecks() {
        let raw = br#"{"model":{"vocab":{"A":0,"B":1},"merges":[]},"added_tokens":[]}"#;
        let a = replay_fixture(raw, b"A");
        let mut b = replay_fixture(raw, b"A");
        b.context = Hash64::from_u64_word(109);
        b.parser = PalwTokenizerJsonStreamV1::new(
            b.context,
            crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(raw),
            raw.len() as u64,
            0,
        );
        b.table.context = b.context;
        let first = producer::generate(&a, raw, data_producer_budget()).unwrap();
        let second = producer::generate(&b, raw, data_producer_budget()).unwrap();
        assert!(!first.proofs().is_empty());
        assert_eq!(first.proofs(), second.proofs());
        assert_eq!(first.root(), second.root());
        // Independent preexisting test Book and actual parser grammar, not generator twice only.
        let atoms = replay_atoms(&a, raw, true).unwrap();
        let expected: Vec<_> = atoms.iter().flat_map(|p| p.proofs.clone()).collect();
        assert_eq!(first.proofs(), expected);
        let budget =
            canonical_replay::Budget { max_material_bytes: 1 << 20, max_proofs: 1000, max_native_calls: 10000, max_packets: 10000 };
        for base in [&a, &b] {
            let material = canonical_replay::Material { raw, proofs: first.proofs(), rendering: Some(b"A") };
            let finding = canonical_replay::build(base, material, budget).unwrap();
            assert!(!finding.violation());
            assert_eq!(finding.key_root(), first.root());
        }
        // Counts only wrapped producer hash invocations, not verifier CPU/hash totals.
        assert_eq!(first.hash_calls, first.native_proof_calls * 514);
        assert_eq!(first.hash_bytes, first.native_proof_calls * (72 + 64 + 512 * 130));
    }
    #[test]
    fn production_key_material_obeys_actual_overrun_duplicate_and_eof_authentication() {
        let raw = br#"{"model":{"vocab":{"A":0,"A":1},"merges":[]}}"#;
        for bound in [0, 1, 512] {
            let mut base = replay_fixture(raw, b"A");
            base.signed_key_bound = bound;
            let result = producer::generate(&base, raw, data_producer_budget()).unwrap();
            assert!(result.proofs().len() as u64 <= bound);
            if bound == 0 {
                assert!(result.proofs().is_empty());
            }
            let budget = canonical_replay::Budget {
                max_material_bytes: 1 << 20,
                max_proofs: 1000,
                max_native_calls: 10000,
                max_packets: 10000,
            };
            let finding = canonical_replay::build(
                &base,
                canonical_replay::Material { raw, proofs: result.proofs(), rendering: Some(b"A") },
                budget,
            )
            .unwrap();
            assert!(finding.violation());
            if bound == 512 {
                // Duplicate membership causes no new leaf/ancestor hashes.
                assert!(result.hash_calls < result.native_proof_calls * 514);
            }
        }
        let base = replay_fixture(raw, b"A");
        let mut bad = raw.to_vec();
        *bad.last_mut().unwrap() = b' ';
        assert!(matches!(
            producer::generate(&base, &bad, data_producer_budget()),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_))
        ));
        let mut budget = data_producer_budget();
        budget.max_requests = 0;
        assert!(matches!(
            producer::generate(&base, raw, budget),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))
        ));
        let good = producer::generate(&base, raw, data_producer_budget()).unwrap();
        let mut budget = data_producer_budget();
        budget.max_hash_calls = good.hash_calls - 1;
        assert!(matches!(
            producer::generate(&base, raw, budget),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))
        ));
    }
    #[test]
    fn production_key_material_owner_refuses_before_construction_holds_quota_and_releases() {
        use crate::palw_native_resource_v1::{NativeDecodeErrorV1, NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1};
        let raw = br#"{"model":{"vocab":{"A":0,"B":1},"merges":[]}}"#;
        let base = replay_fixture(raw, b"A");
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: 0 });
        let first =
            producer::generate_reserved(&base, raw, data_producer_budget(), |q| checker.reserve_requests_v1(q, 0).map(Some)).unwrap();
        let q = checker.outstanding().0;
        assert!(q > 0);
        drop(first);
        assert_eq!(checker.outstanding(), (0, 0));
        let mut wrong = raw.to_vec();
        *wrong.last_mut().unwrap() = b' ';
        for bytes in [raw.as_slice(), wrong.as_slice()] {
            let short = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: q - 1, database_bytes: 0 });
            assert!(matches!(
                producer::generate_reserved(&base, bytes, data_producer_budget(), |n| short.reserve_requests_v1(n, 0).map(Some)),
                Err(NativeDecodeErrorV1::Resource(_))
            ));
            assert_eq!(short.outstanding(), (0, 0));
        }
        let exact = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: q, database_bytes: 0 });
        let owner =
            producer::generate_reserved(&base, raw, data_producer_budget(), |n| exact.reserve_requests_v1(n, 0).map(Some)).unwrap();
        assert_eq!(exact.outstanding(), (q, 0));
        assert!(exact.reserve_requests_v1(1, 0).is_err());
        drop(owner);
        assert_eq!(exact.outstanding(), (0, 0));
        let ticket = exact.reserve_requests_v1(q, 0).unwrap();
        drop(ticket);
        assert!(matches!(
            producer::generate_reserved(&base, &wrong, data_producer_budget(), |n| exact.reserve_requests_v1(n, 0).map(Some)),
            Err(NativeDecodeErrorV1::Decode(_))
        ));
        assert_eq!(exact.outstanding(), (0, 0));
        let ticket = exact.reserve_requests_v1(q, 0).unwrap();
        drop(ticket);
        assert_eq!(exact.outstanding(), (0, 0));
    }
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_tokenizer_table_stream_v1.rs"]
mod native_resource_inventory_v1;
