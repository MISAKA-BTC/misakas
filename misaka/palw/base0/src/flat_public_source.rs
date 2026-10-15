//! Read-at projection of an original retained capture. Offsets are disposable process-local
//! acceleration; after restart the bounded index is rebuilt from the original bytes.
use kaspa_consensus_core::{
    palw_backend::{PalwFlatPublicSourceReaderV1, PalwOriginalCaptureReaderV1},
    palw_flat_constraint_stream_v1::{
        PALW_FLAT_CONSTRAINT_PART_WORDS_V1, PalwFlatConstraintActionV1 as Action, PalwFlatConstraintCursorV1 as Cursor,
    },
    palw_step_leg::{PalwStepBindingV2, verify_binding_v1},
    palw_step_refute::flat_logits_scheme_id_v1,
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{Arc, Mutex},
};

pub const FLAT_SOURCE_INDEX_WORK_V1: usize = 64;
pub const FLAT_SOURCE_MAX_READ_V1: u32 = 16_384;
pub const FLAT_PUBLIC_SOURCE_MAGIC_V1: &[u8; 8] = b"MSKFLTV1";
const PUBLIC_HEADER_BYTES_V1: u64 = 212;

/// An FPC1 nested capture range, never a regenerated FPM1/FPA1 answer. Original public job
/// identity is checked before selecting the range; native Flat EOF remains the authority.
pub struct Base0OriginalCaptureRangeV1 {
    source: Arc<dyn PalwOriginalCaptureReaderV1>,
    start: u64,
    len: u64,
}
impl Base0OriginalCaptureRangeV1 {
    pub fn fp_capture_v1(
        source: Arc<dyn PalwOriginalCaptureReaderV1>,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    ) -> Result<Self, String> {
        if Base0FlatPublicSourceV1::read(&*source, 0, 4)?.as_slice() != b"FPC1" {
            return Err("Flat source needs original capture, not job/answer-only envelope".into());
        }
        let expected = borsh::to_vec(job).map_err(|e| e.to_string())?;
        let mut at = 4;
        for part in expected.chunks(FLAT_SOURCE_MAX_READ_V1 as usize) {
            if Base0FlatPublicSourceV1::read(&*source, at, part.len() as u32)? != part {
                return Err("Flat capture envelope changes original public job".into());
            }
            at = Base0FlatPublicSourceV1::advance(&*source, at, part.len() as u64)?;
        }
        let count = Base0FlatPublicSourceV1::word(&*source, at)?;
        if count != job.prompt_tokens {
            return Err("Flat envelope changes original prompt count".into());
        }
        // No prompt/private execution input is read to serve the committed public logits.
        at = Base0FlatPublicSourceV1::advance(&*source, at, 4 + u64::from(count) * 4)?;
        let len = u64::from(Base0FlatPublicSourceV1::word(&*source, at)?);
        let start = Base0FlatPublicSourceV1::advance(&*source, at, 4)?;
        if len == 0 || Base0FlatPublicSourceV1::advance(&*source, start, len)? != source.len_v1() {
            return Err("Flat capture envelope truncated or has trailing bytes".into());
        }
        Ok(Self { source, start, len })
    }
}
impl PalwOriginalCaptureReaderV1 for Base0OriginalCaptureRangeV1 {
    fn len_v1(&self) -> u64 {
        self.len
    }
    fn read_v1(&self, offset: u64, length: u32) -> Result<Vec<u8>, String> {
        if offset.checked_add(length as u64).is_none_or(|end| end > self.len) {
            return Err("Flat nested capture range exceeded".into());
        }
        self.source.read_v1(self.start + offset, length)
    }
}

/// Own retained file only. This never regenerates material from private execution state.
pub struct Base0OriginalCaptureFileV1 {
    file: Mutex<File>,
    len: u64,
}
impl Base0OriginalCaptureFileV1 {
    pub fn open_v1(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        Ok(Self { file: Mutex::new(file), len })
    }
}
impl PalwOriginalCaptureReaderV1 for Base0OriginalCaptureFileV1 {
    fn len_v1(&self) -> u64 {
        self.len
    }
    fn read_v1(&self, offset: u64, length: u32) -> Result<Vec<u8>, String> {
        if length > FLAT_SOURCE_MAX_READ_V1 || offset.checked_add(length as u64).is_none_or(|end| end > self.len) {
            return Err("Flat capture read exceeds bounded source range".into());
        }
        let mut file = self.file.lock().map_err(|_| "Flat capture file lock poisoned")?;
        file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let mut out = vec![0; length as usize];
        file.read_exact(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }
}

#[derive(Clone, Copy)]
enum Phase {
    Tiles(u32),
    RowCount,
    Rows(u32),
    IdCount,
    Ready,
}
pub struct Base0FlatPublicSourceV1<'a> {
    source: Arc<dyn PalwOriginalCaptureReaderV1 + 'a>,
    offset: u64,
    phase: Phase,
    decode: u32,
    vocab: u32,
    rows: Vec<u64>,
    ids: u64,
    direct: bool,
}
impl<'a> Base0FlatPublicSourceV1<'a> {
    pub fn open_v1(source: Arc<dyn PalwOriginalCaptureReaderV1 + 'a>, binding: &PalwStepBindingV2) -> Result<Self, String> {
        verify_binding_v1(binding).map_err(|e| e.to_string())?;
        if binding.shape_profile.logits_scheme_id != flat_logits_scheme_id_v1()
            || binding.job_context.exact_decode_tokens as usize > kaspa_consensus_core::palw_v2::PALW_V2_MAX_TRACE_EVENTS
            || binding.shape_profile.vocab_size == 0
        {
            return Err("source is not an original native Flat capture".into());
        }
        let magic = Self::read(&*source, 0, 8)?;
        if magic.as_slice() == FLAT_PUBLIC_SOURCE_MAGIC_V1 {
            let expected = public_header_v1(binding);
            if Self::read(&*source, 0, expected.len() as u32)? != expected {
                return Err("Flat sidecar changes original binding/context".into());
            }
            let decode = binding.job_context.exact_decode_tokens;
            let vocab = binding.shape_profile.vocab_size;
            let body_bytes = 96 + u64::from(decode) * (12 + u64::from(vocab) * 4);
            if PUBLIC_HEADER_BYTES_V1 + body_bytes != source.len_v1() {
                return Err("Flat sidecar original preimage length mismatch".into());
            }
            let mut body_header = binding.job_context.context_hash().as_byte_slice().to_vec();
            body_header.extend(u64::from(binding.job_context.declared_prefill_tokens).to_le_bytes());
            body_header.extend(u64::from(decode).to_le_bytes());
            body_header.extend(u64::from(decode).to_le_bytes());
            if Self::read(&*source, PUBLIC_HEADER_BYTES_V1, 88)? != body_header {
                return Err("Flat sidecar original hash header mismatch".into());
            }
            let ids = PUBLIC_HEADER_BYTES_V1 + 88 + u64::from(decode) * (8 + u64::from(vocab) * 4) + 8;
            if Self::read(&*source, ids - 8, 8)? != u64::from(decode).to_le_bytes() {
                return Err("Flat sidecar original ID count mismatch".into());
            }
            return Ok(Self {
                source,
                offset: PUBLIC_HEADER_BYTES_V1,
                phase: Phase::Ready,
                decode,
                vocab,
                rows: Vec::new(),
                ids,
                direct: true,
            });
        }
        let folded = magic.as_slice() == b"MSKFPMV2";
        let mut offset = if folded {
            if Self::read(&*source, 8, 2)?.as_slice() != 2u16.to_le_bytes() {
                return Err("unsupported folded version".into());
            }
            10
        } else {
            0
        };
        let expected = borsh::to_vec(binding).map_err(|e| e.to_string())?;
        // Compare bounded pieces rather than deserializing hostile capture vectors.
        for part in expected.chunks(FLAT_SOURCE_MAX_READ_V1 as usize) {
            if Self::read(&*source, offset, part.len() as u32)? != part {
                return Err("retained capture original binding mismatch".into());
            }
            offset = Self::advance(&*source, offset, part.len() as u64)?;
        }
        let phase = if folded {
            let meta = Self::read(&*source, offset, 16)?;
            let leaves = u64::from_le_bytes(meta[0..8].try_into().unwrap());
            let level = u32::from_le_bytes(meta[8..12].try_into().unwrap());
            let nodes = u32::from_le_bytes(meta[12..16].try_into().unwrap()) as u64;
            if leaves != binding.step_leaf_count
                || level > crate::fp_capture::PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1
                || nodes != leaves.div_ceil(1u64 << level)
            {
                return Err("retained folded tree shape mismatch".into());
            }
            offset = Self::advance(&*source, offset, 16 + nodes * 64)?;
            Phase::RowCount
        } else {
            let count = Self::word(&*source, offset)?;
            if count as u64 > binding.step_leaf_count {
                return Err("retained tile count exceeds original binding".into());
            }
            offset = Self::advance(&*source, offset, 4)?;
            Phase::Tiles(count)
        };
        Ok(Self {
            source,
            offset,
            phase,
            decode: binding.job_context.exact_decode_tokens,
            vocab: binding.shape_profile.vocab_size,
            rows: Vec::with_capacity(binding.job_context.exact_decode_tokens as usize),
            ids: 0,
            direct: false,
        })
    }
    fn read(source: &dyn PalwOriginalCaptureReaderV1, at: u64, n: u32) -> Result<Vec<u8>, String> {
        if n > FLAT_SOURCE_MAX_READ_V1 || at.checked_add(n as u64).is_none_or(|end| end > source.len_v1()) {
            return Err("Flat capture structural read out of range".into());
        }
        let bytes = source.read_v1(at, n)?;
        if bytes.len() != n as usize {
            return Err("Flat capture short read".into());
        }
        Ok(bytes)
    }
    fn word(source: &dyn PalwOriginalCaptureReaderV1, at: u64) -> Result<u32, String> {
        Ok(u32::from_le_bytes(Self::read(source, at, 4)?.try_into().unwrap()))
    }
    fn advance(source: &dyn PalwOriginalCaptureReaderV1, at: u64, bytes: u64) -> Result<u64, String> {
        at.checked_add(bytes).filter(|end| *end <= source.len_v1()).ok_or("Flat capture offset overflow or truncation".into())
    }
    fn index_v1(&mut self) -> Result<bool, String> {
        for _ in 0..FLAT_SOURCE_INDEX_WORK_V1 {
            match self.phase {
                Phase::Tiles(0) => self.phase = Phase::RowCount,
                Phase::Tiles(left) => {
                    // u64 ordinal + u16 version + four u32 coordinates + value count + byte count.
                    let header = Self::read(&*self.source, self.offset, 34)?;
                    let count = u32::from_le_bytes(header[26..30].try_into().unwrap());
                    let bytes = u32::from_le_bytes(header[30..34].try_into().unwrap());
                    if count as u64 * 4 != bytes as u64 {
                        return Err("retained tile scalar length mismatch".into());
                    }
                    self.offset = Self::advance(&*self.source, self.offset, 34 + bytes as u64)?;
                    self.phase = Phase::Tiles(left - 1);
                }
                Phase::RowCount => {
                    if Self::word(&*self.source, self.offset)? != self.decode {
                        return Err("Flat row count mismatch".into());
                    }
                    self.offset = Self::advance(&*self.source, self.offset, 4)?;
                    self.phase = Phase::Rows(0);
                }
                Phase::Rows(row) if row < self.decode => {
                    if Self::word(&*self.source, self.offset)? != self.vocab {
                        return Err("Flat row width mismatch".into());
                    }
                    self.offset = Self::advance(&*self.source, self.offset, 4)?;
                    self.rows.push(self.offset);
                    self.offset = Self::advance(&*self.source, self.offset, self.vocab as u64 * 4)?;
                    self.phase = Phase::Rows(row + 1);
                }
                Phase::Rows(_) => self.phase = Phase::IdCount,
                Phase::IdCount => {
                    if Self::word(&*self.source, self.offset)? != self.decode {
                        return Err("Flat generated ID count mismatch".into());
                    }
                    self.ids = Self::advance(&*self.source, self.offset, 4)?;
                    Self::advance(&*self.source, self.ids, self.decode as u64 * 4)?;
                    self.phase = Phase::Ready;
                }
                Phase::Ready => return Ok(true),
            }
        }
        Ok(matches!(self.phase, Phase::Ready))
    }
}
impl PalwFlatPublicSourceReaderV1 for Base0FlatPublicSourceV1<'_> {
    fn poll_v1(&mut self, cursor: Cursor) -> Result<Option<Action>, String> {
        if !self.index_v1()? {
            return Ok(None);
        }
        let action = match cursor {
            Cursor::RowBegin { pass, row } if pass < 2 && row < self.decode => Action::RowBegin { row, width: self.vocab },
            Cursor::RowPart { pass, row, offset } if pass < 2 && row < self.decode && offset < self.vocab => {
                let words = (self.vocab - offset).min(PALW_FLAT_CONSTRAINT_PART_WORDS_V1 as u32);
                let row_at = if self.direct {
                    PUBLIC_HEADER_BYTES_V1 + 88 + u64::from(row) * (8 + u64::from(self.vocab) * 4) + 8
                } else {
                    self.rows[row as usize]
                };
                if self.direct && Self::read(&*self.source, row_at - 8, 8)? != u64::from(self.vocab).to_le_bytes() {
                    return Err("Flat sidecar original row width mismatch".into());
                }
                let at = row_at + offset as u64 * 4;
                let raw = Self::read(&*self.source, at, words * 4)?;
                Action::RowPart {
                    row,
                    offset,
                    values: raw.chunks_exact(4).map(|v| i32::from_le_bytes(v.try_into().unwrap())).collect(),
                }
            }
            Cursor::Ids { pass, offset } if pass < 2 && offset < self.decode => {
                let words = (self.decode - offset).min(PALW_FLAT_CONSTRAINT_PART_WORDS_V1 as u32);
                let raw = Self::read(&*self.source, self.ids + offset as u64 * 4, words * 4)?;
                Action::IdsPart { offset, ids: raw.chunks_exact(4).map(|v| u32::from_le_bytes(v.try_into().unwrap())).collect() }
            }
            Cursor::Finish { pass } if pass < 2 => Action::Finish,
            _ => return Err("Flat source cursor outside original capture".into()),
        };
        Ok(Some(action))
    }
}

fn public_header_v1(binding: &PalwStepBindingV2) -> Vec<u8> {
    let mut bytes = FLAT_PUBLIC_SOURCE_MAGIC_V1.to_vec();
    bytes.extend(binding.committed_execution_root.as_byte_slice());
    bytes.extend(binding.job_context.context_hash().as_byte_slice());
    bytes.extend(binding.full_logits_trace_root.as_byte_slice());
    bytes.extend(binding.job_context.declared_prefill_tokens.to_le_bytes());
    bytes.extend(binding.job_context.exact_decode_tokens.to_le_bytes());
    bytes.extend(binding.shape_profile.vocab_size.to_le_bytes());
    bytes
}

struct BorrowedCaptureV1<'a>(&'a [u8]);
impl PalwOriginalCaptureReaderV1 for BorrowedCaptureV1<'_> {
    fn len_v1(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_v1(&self, offset: u64, length: u32) -> Result<Vec<u8>, String> {
        if length > FLAT_SOURCE_MAX_READ_V1 {
            return Err("borrowed Flat read exceeds scratch".into());
        }
        let start = usize::try_from(offset).map_err(|_| "borrowed capture offset overflow")?;
        let end = start.checked_add(length as usize).ok_or("borrowed capture range overflow")?;
        self.0.get(start..end).map(<[u8]>::to_vec).ok_or("borrowed capture truncated".into())
    }
}

/// Normal producer capture-save projection, before publishing its obligation. Only public
/// logits/IDs are copied in 16KiB chunks; no whole matrix decode/clone and no execution occurs.
/// The original keyed EOF is checked before the caller atomically publishes the sidecar.
pub fn write_flat_public_source_v1(material: &[u8], out: &mut dyn Write) -> Result<bool, String> {
    let binding = capture_binding_v1(material)?;
    if binding.shape_profile.logits_scheme_id != flat_logits_scheme_id_v1() {
        return Ok(false);
    }
    let mut reader = Base0FlatPublicSourceV1::open_v1(Arc::new(BorrowedCaptureV1(material)), &binding)?;
    while !reader.index_v1()? {}
    write_projected_v1(&binding, &mut reader, out)
}

fn capture_binding_v1(material: &[u8]) -> Result<PalwStepBindingV2, String> {
    use borsh::BorshDeserialize;
    let body = if let Some(body) = material.strip_prefix(b"MSKFPMV2") {
        if body.get(..2) != Some(&2u16.to_le_bytes()) {
            return Err("original folded capture version mismatch".into());
        }
        &body[2..]
    } else {
        material
    };
    // This is the already executed local capture. Deserialize only its public binding header,
    // never the matrix/checkpoint vectors following it.
    let binding = PalwStepBindingV2::deserialize_reader(&mut &body[..]).map_err(|e| format!("original capture binding: {e}"))?;
    verify_binding_v1(&binding).map_err(|e| e.to_string())?;
    Ok(binding)
}

pub fn original_public_binding_header_v1(
    material: &[u8],
) -> Result<kaspa_consensus_core::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1, String> {
    use kaspa_consensus_core::palw_public_binding_v1::{PalwPublicExecutionBindingHeaderV1, PalwPublicLegacyBindingHeaderV1};
    let binding = capture_binding_v1(material)?;
    let header = PalwPublicExecutionBindingHeaderV1::Legacy(PalwPublicLegacyBindingHeaderV1::of_binding_v1(&binding));
    header.check_wire_v1()?;
    Ok(header)
}

fn write_projected_v1(
    binding: &PalwStepBindingV2,
    reader: &mut Base0FlatPublicSourceV1<'_>,
    out: &mut dyn Write,
) -> Result<bool, String> {
    out.write_all(&public_header_v1(binding)).map_err(|e| e.to_string())?;
    let mut hash = blake2b_simd::Params::new()
        .hash_length(64)
        .key(kaspa_consensus_core::palw_step_refute::PALW_BASE0_DOMAIN_LOGITS_TRACE)
        .to_state();
    fn emit(out: &mut dyn Write, hash: &mut blake2b_simd::State, bytes: &[u8]) -> Result<(), String> {
        out.write_all(bytes).map_err(|e| e.to_string())?;
        hash.update(bytes);
        Ok(())
    }
    emit(out, &mut hash, binding.job_context.context_hash().as_byte_slice())?;
    emit(out, &mut hash, &u64::from(binding.job_context.declared_prefill_tokens).to_le_bytes())?;
    let decode = binding.job_context.exact_decode_tokens;
    emit(out, &mut hash, &u64::from(decode).to_le_bytes())?;
    emit(out, &mut hash, &u64::from(decode).to_le_bytes())?;
    let mut scratch = Vec::with_capacity(FLAT_SOURCE_MAX_READ_V1 as usize);
    for row in 0..decode {
        emit(out, &mut hash, &u64::from(reader.vocab).to_le_bytes())?;
        let mut offset = 0;
        while offset < reader.vocab {
            let Some(Action::RowPart { values, .. }) = reader.poll_v1(Cursor::RowPart { pass: 0, row, offset })? else {
                return Err("original Flat row projection lost bounded typed part".into());
            };
            scratch.clear();
            for value in &values {
                scratch.extend(value.to_le_bytes());
            }
            emit(out, &mut hash, &scratch)?;
            offset = offset.checked_add(values.len() as u32).ok_or("Flat writer row offset overflow")?;
        }
    }
    emit(out, &mut hash, &u64::from(decode).to_le_bytes())?;
    let mut offset = 0;
    while offset < decode {
        let Some(Action::IdsPart { ids, .. }) = reader.poll_v1(Cursor::Ids { pass: 0, offset })? else {
            return Err("original Flat ID projection lost bounded typed part".into());
        };
        scratch.clear();
        for id in &ids {
            scratch.extend(id.to_le_bytes());
        }
        emit(out, &mut hash, &scratch)?;
        offset += ids.len() as u32;
    }
    if hash.finalize().as_bytes() != binding.full_logits_trace_root.as_byte_slice() {
        return Err("original Flat sidecar fails committed original EOF".into());
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1},
        palw_flat_constraint_stream_v1::PalwFlatConstraintStreamV1,
        palw_step::PalwStepCoordinateV1,
        palw_step_leg::{PalwStepTileLeafV1, binding_commitment_root_v1},
        palw_step_refute::base0_logits_trace_root_v1,
    };
    use kaspa_hashes::Hash64;

    struct SparseSource {
        spans: Vec<(u64, Vec<u8>)>,
        len: u64,
        reads: Mutex<Vec<(u64, u32)>>,
    }
    impl PalwOriginalCaptureReaderV1 for SparseSource {
        fn len_v1(&self) -> u64 {
            self.len
        }
        fn read_v1(&self, offset: u64, length: u32) -> Result<Vec<u8>, String> {
            assert!(length <= FLAT_SOURCE_MAX_READ_V1);
            assert!(offset + length as u64 <= self.len);
            self.reads.lock().unwrap().push((offset, length));
            let mut out = vec![0; length as usize];
            for (at, bytes) in &self.spans {
                let start = offset.max(*at);
                let end = (offset + length as u64).min(*at + bytes.len() as u64);
                if start < end {
                    out[(start - offset) as usize..(end - offset) as usize]
                        .copy_from_slice(&bytes[(start - at) as usize..(end - at) as usize]);
                }
            }
            Ok(out)
        }
    }
    fn source(bytes: Vec<u8>) -> Arc<SparseSource> {
        Arc::new(SparseSource { len: bytes.len() as u64, spans: vec![(0, bytes)], reads: Mutex::new(Vec::new()) })
    }
    fn binding(decode: u32, vocab: u32, rows: &[Vec<i32>], ids: &[u32]) -> PalwStepBindingV2 {
        let mut profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
        profile.logits_scheme_id = flat_logits_scheme_id_v1();
        profile.vocab_size = vocab;
        let (ctx, _) = crate::produce::base0_rc_job_v1(
            &profile,
            Hash64::from_u64_word(51),
            vocab as usize,
            1,
            decode,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        );
        let mut binding = PalwStepBindingV2 {
            version: kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
            job_context: ctx,
            checkpoint_profile: kaspa_consensus_core::palw_attempt_rules_v1::palw_canonical_checkpoint_profile_v1(&profile),
            state_chunk_map_id: profile.state_chunk_map_id,
            shape_profile: profile,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            step_leaf_count: 130,
            step_merkle_root: Hash64::default(),
            checkpoint_count: 0,
            checkpoint_merkle_root: Hash64::default(),
            committed_execution_root: Hash64::default(),
        };
        binding.full_logits_trace_root = base0_logits_trace_root_v1(&binding.job_context, rows, ids);
        binding.committed_execution_root = binding_commitment_root_v1(&binding);
        binding
    }
    fn poll(reader: &mut Base0FlatPublicSourceV1, cursor: Cursor) -> Action {
        for _ in 0..16 {
            if let Some(action) = reader.poll_v1(cursor).unwrap() {
                return action;
            }
        }
        panic!("bounded fixture index must finish");
    }
    #[test]
    fn dense_original_bytes_stream_to_authenticated_pin_without_private_tensor_reads() {
        let rows = vec![vec![4, -2, 7, 6], vec![1, 9, 2, 3]];
        let ids = vec![2, 1];
        let binding = binding(2, 4, &rows, &ids);
        let tile = PalwStepTileLeafV1 {
            version: 1,
            coord: PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 },
            value_count: 1,
            values_le: 99i32.to_le_bytes().to_vec(),
        };
        let tiles: Vec<_> = (0..130u64).map(|i| (i, tile.clone())).collect();
        let bytes = borsh::to_vec(&(&binding, &tiles, &rows, &ids, vec![vec![vec![7u8; 32]]])).unwrap();
        let public_end = borsh::to_vec(&(&binding, &tiles, &rows, &ids)).unwrap().len() as u64;
        let source = source(bytes);
        let mut reader = Base0FlatPublicSourceV1::open_v1(source.clone(), &binding).unwrap();
        let context = Hash64::from_u64_word(55);
        let prices = kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1().prices;
        let mut court = PalwFlatConstraintStreamV1::new_v1(context, &binding, 0, Some(2), Some(3), prices).unwrap();
        assert!(reader.poll_v1(court.cursor_v1()).unwrap().is_none());
        while court.cursor_v1() != Cursor::Complete {
            let action = poll(&mut reader, court.cursor_v1());
            court.movement_v1(context, &action).unwrap();
        }
        let pin = court.authenticated_pin_v1().unwrap();
        assert_eq!(pin.generated_ids_v1(), ids);
        assert_eq!(pin.committed_value_v1(), Some(7));
        assert_eq!(pin.beat_v1(), Some((3, 6)));
        let binding_bytes = borsh::to_vec(&binding).unwrap().len() as u64;
        for &(at, n) in source.reads.lock().unwrap().iter() {
            assert!(at + n as u64 <= public_end);
            for i in 0..130u64 {
                let private = binding_bytes + 4 + i * 38 + 34;
                assert!(at + n as u64 <= private || at >= private + 4, "must skip private step tensor values");
            }
        }
    }
    #[test]
    fn folded_projection_restart_uses_original_header_and_same_public_rows() {
        let rows = vec![vec![5, 8, 3]];
        let ids = vec![1];
        let binding = binding(1, 3, &rows, &ids);
        let tree = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_v1(&vec![Hash64::default(); 130], 2).unwrap();
        let mut bytes = b"MSKFPMV2".to_vec();
        bytes.extend(
            borsh::to_vec(&(
                2u16,
                &binding,
                &tree,
                &rows,
                &ids,
                Vec::<u32>::new(),
                Vec::<Vec<Vec<u8>>>::new(),
                Vec::<kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2>::new(),
            ))
            .unwrap(),
        );
        let source = source(bytes);
        let mut first = Base0FlatPublicSourceV1::open_v1(source.clone(), &binding).unwrap();
        let cursor = Cursor::RowPart { pass: 1, row: 0, offset: 1 };
        let expected = poll(&mut first, cursor);
        let mut restarted = Base0FlatPublicSourceV1::open_v1(source, &binding).unwrap();
        assert_eq!(poll(&mut restarted, cursor), expected);
        assert_eq!(expected, Action::RowPart { row: 0, offset: 1, values: vec![8, 3] });
        assert!(restarted.poll_v1(Cursor::Ids { pass: 0, offset: 1 }).is_err());
    }
    #[test]
    fn full_u32_row_offsets_are_read_at_without_whole_row_allocation() {
        let mut binding = binding(1, 4, &[vec![0; 4]], &[0]);
        binding.shape_profile.vocab_size = u32::MAX;
        binding.job_context.shape_profile_id = binding.shape_profile.shape_profile_id();
        binding.committed_execution_root = binding_commitment_root_v1(&binding);
        let mut head = borsh::to_vec(&binding).unwrap();
        head.extend(0u32.to_le_bytes());
        head.extend(1u32.to_le_bytes());
        head.extend(u32::MAX.to_le_bytes());
        let row_at = head.len() as u64;
        let ids_count = row_at + u32::MAX as u64 * 4;
        let source = Arc::new(SparseSource {
            spans: vec![(0, head), (ids_count, [1u32.to_le_bytes(), 0u32.to_le_bytes()].concat())],
            len: ids_count + 8,
            reads: Mutex::new(Vec::new()),
        });
        let mut reader = Base0FlatPublicSourceV1::open_v1(source.clone(), &binding).unwrap();
        assert_eq!(
            poll(&mut reader, Cursor::RowPart { pass: 0, row: 0, offset: u32::MAX - 2 }),
            Action::RowPart { row: 0, offset: u32::MAX - 2, values: vec![0; 2] }
        );
        assert_eq!(reader.rows.len(), 1);
        assert!(source.reads.lock().unwrap().iter().all(|(_, n)| *n <= FLAT_SOURCE_MAX_READ_V1));
        // This is structural max-width coverage, not a full maximum-size hash/conviction trial.
    }
    #[test]
    fn wrong_binding_and_truncated_original_bytes_do_not_produce_credentials() {
        let rows = vec![vec![1, 2, 3]];
        let ids = vec![2];
        let binding = binding(1, 3, &rows, &ids);
        let bytes = borsh::to_vec(&(&binding, Vec::<(u64, PalwStepTileLeafV1)>::new(), &rows, &ids)).unwrap();
        let mut wrong = binding.clone();
        wrong.activation_leg_root = Hash64::from_u64_word(99);
        wrong.committed_execution_root = binding_commitment_root_v1(&wrong);
        assert!(Base0FlatPublicSourceV1::open_v1(source(bytes.clone()), &wrong).is_err());
        let mut truncated = bytes;
        truncated.truncate(truncated.len() - 1);
        let mut reader = Base0FlatPublicSourceV1::open_v1(source(truncated), &binding).unwrap();
        assert!(reader.poll_v1(Cursor::RowBegin { pass: 0, row: 0 }).is_err());
    }
    #[test]
    fn original_public_sidecar_restarts_at_direct_offsets_and_rejects_wrong_eof() {
        let rows = vec![vec![1, 8, 2], vec![7, 3, 5]];
        let ids = vec![1, 0];
        let binding = binding(2, 3, &rows, &ids);
        let original =
            borsh::to_vec(&(&binding, Vec::<(u64, PalwStepTileLeafV1)>::new(), &rows, &ids, vec![vec![vec![99u8; 64]]])).unwrap();
        let mut projected = Vec::new();
        assert!(write_flat_public_source_v1(&original, &mut projected).unwrap());
        assert_eq!(projected.len() as u64, PUBLIC_HEADER_BYTES_V1 + 96 + 2 * (12 + 3 * 4));
        let cursor = Cursor::RowPart { pass: 1, row: 1, offset: 1 };
        let mut first = Base0FlatPublicSourceV1::open_v1(source(projected.clone()), &binding).unwrap();
        assert!(first.direct && first.rows.is_empty());
        assert_eq!(first.poll_v1(cursor).unwrap(), Some(Action::RowPart { row: 1, offset: 1, values: vec![3, 5] }));
        let mut restarted = Base0FlatPublicSourceV1::open_v1(source(projected.clone()), &binding).unwrap();
        assert_eq!(restarted.poll_v1(cursor).unwrap(), first.poll_v1(cursor).unwrap());
        let mut wrong_original = original;
        let prefix = borsh::to_vec(&(&binding, Vec::<(u64, PalwStepTileLeafV1)>::new())).unwrap().len();
        wrong_original[prefix + 8] ^= 1;
        assert!(write_flat_public_source_v1(&wrong_original, &mut Vec::new()).is_err());
        // A local sidecar's header is not authority: altered public bytes still fail native EOF.
        projected[(PUBLIC_HEADER_BYTES_V1 + 88 + 8) as usize] ^= 1;
        let mut reader = Base0FlatPublicSourceV1::open_v1(source(projected), &binding).unwrap();
        let context = Hash64::from_u64_word(77);
        let prices = kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1().prices;
        let mut court = PalwFlatConstraintStreamV1::new_v1(context, &binding, 0, Some(1), None, prices).unwrap();
        while !matches!(court.cursor_v1(), Cursor::Finish { .. }) {
            let action = reader.poll_v1(court.cursor_v1()).unwrap().unwrap();
            court.movement_v1(context, &action).unwrap();
        }
        assert!(court.movement_v1(context, &Action::Finish).is_err());
        assert!(court.authenticated_pin_v1().is_err());
    }
    #[test]
    fn fpc_range_checks_original_job_and_never_reexecutes_job_only_material() {
        use kaspa_consensus_core::palw_freeprompt_v3::{PalwFreePromptJobV3, palw_fp_capture_encode_v1, palw_fp_material_encode_v1};
        let job = PalwFreePromptJobV3 {
            version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(1),
            class_id: Hash64::from_u64_word(2),
            executor_bond: kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(3), 0),
            executor_pubkey: vec![4; 32],
            operator_id: Hash64::from_u64_word(5),
            anchor_block: Hash64::from_u64_word(6),
            anchor_daa: 10,
            job_nonce: [7; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::from_u64_word(8),
            prompt_tokens: 2,
            decode_token_limit: 1,
            max_context_tokens: 32,
            privacy_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
            decode: None,
            tail: None,
        };
        let capture = b"original-capture";
        let bytes = palw_fp_capture_encode_v1(&job, &[1, 2], capture);
        let range = Base0OriginalCaptureRangeV1::fp_capture_v1(source(bytes.clone()), &job).unwrap();
        assert_eq!(range.len_v1(), capture.len() as u64);
        assert_eq!(range.read_v1(0, capture.len() as u32).unwrap(), capture);
        assert!(range.read_v1(1, capture.len() as u32).is_err());
        let mut wrong = job.clone();
        wrong.anchor_daa += 1;
        assert!(Base0OriginalCaptureRangeV1::fp_capture_v1(source(bytes), &wrong).is_err());
        assert!(Base0OriginalCaptureRangeV1::fp_capture_v1(source(palw_fp_material_encode_v1(&job, &[1, 2])), &job).is_err());
    }
}
