//! Explicit-limit capacity preflight for the shared per-case journal codec.
//! A record is a length-delimited input, never an allocating Borsh decode of a
//! journal-wide `Vec<Record>`. Framing and identity grant no native authority.
use super::wire::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1};
use super::{NativeCoverageV1, NativeHeapV1, native_enum, native_struct};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    journal::{
        CASE_JOURNAL_AUX_MATERIAL_BYTES, CASE_JOURNAL_HEADER_BYTES, CASE_JOURNAL_MIN_RECORD_BYTES, CaseJournalAux,
        CaseJournalEventKind, CaseJournalHeader, CaseJournalLimits, CaseJournalRecord,
    },
    rows::AcceptedMaterial,
};
use std::{
    io::{self, Read},
    mem::size_of,
};

const STREAM_CHUNK_BYTES: usize = 8192;
/// One record's fixed logical workspace: encoded and decoded fixed header,
/// one decoded material, two canonical-reference comparison slots, a bounded
/// stream chunk and the native-body prefix. No record-sized temporary Vec is
/// needed. This is allocation-request accounting, not a stack/RSS guarantee.
pub const POSITIVE_JOURNAL_DECODE_WORKSPACE_BYTES_V1: usize = CASE_JOURNAL_HEADER_BYTES
    + size_of::<CaseJournalHeader>()
    + size_of::<AcceptedMaterial>()
    + 2 * CASE_JOURNAL_AUX_MATERIAL_BYTES
    + STREAM_CHUNK_BYTES
    + 5;

native_enum!(CaseJournalEventKind; NativeBody, NativeTransaction);
native_struct!(CaseJournalHeader; version, revision, scope, case, result, sequence, previous, origin, kind);
native_struct!(CaseJournalAux; material, preimage);

impl NativeHeapV1 for CaseJournalRecord {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        // Exhaustive ownership inventory: a future shared field cannot silently
        // bypass admission. `size_of::<Self>()` belongs to the containing owner
        // under NativeHeap's convention, even for the shortest event variant.
        let Self { header, payload, aux } = self;
        header
            .native_heap_v1(path)
            .add::<Self>(payload.native_heap_v1(path), path)
            .add::<Self>(aux.native_heap_v1(path), path)
            .add::<Self>(NativeCoverageV1::Bounded { bytes: POSITIVE_JOURNAL_DECODE_WORKSPACE_BYTES_V1 as u64 }, path)
    }
    fn native_members_v1(&self) -> Option<u64> {
        let Self { header, payload, aux } = self;
        header.native_members_v1()?.checked_add(payload.native_members_v1()?)?.checked_add(aux.native_members_v1()?)
    }
    fn native_components_v1(&self) -> Vec<super::NativeFieldInventoryV1> {
        let Self { header, payload, aux } = self;
        vec![
            super::NativeFieldInventoryV1::of("CaseJournalRecord.header", header),
            super::NativeFieldInventoryV1::of("CaseJournalRecord.payload", payload),
            super::NativeFieldInventoryV1::of("CaseJournalRecord.aux", aux),
            super::NativeFieldInventoryV1 {
                component: "CaseJournalRecord.decode-workspace".into(),
                native_type: std::any::type_name::<Self>().into(),
                inline_bytes: 0,
                entries: 1,
                heap: NativeCoverageV1::Bounded { bytes: POSITIVE_JOURNAL_DECODE_WORKSPACE_BYTES_V1 as u64 },
            },
        ]
    }
}

impl NativeWireScanV1 for CaseJournalEventKind {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 | 1 => Ok(NativeWireShapeV1::default()),
            _ => Err(journal_bad("unknown case-journal event kind")),
        }
    }
}
impl NativeWireScanV1 for CaseJournalHeader {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        scan_header(reader)?;
        Ok(NativeWireShapeV1::default())
    }
}
// The shared codec has no adopted global profile ceilings. The generic trait
// cannot supply caller policy, so refuse before touching untrusted input.
impl NativeWireScanV1 for CaseJournalRecord {
    fn scan_native_wire_v1<R: Read>(_: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        Err(io::ErrorKind::Unsupported.into())
    }
}
impl NativeWireScanV1 for CaseJournalAux {
    fn scan_native_wire_v1<R: Read>(_: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

// Keep local rejection allocation-free too. Diagnostic strings below document
// the rejected invariant without allocating an io::Error payload before admission.
fn journal_bad(_invariant: &'static str) -> io::Error {
    io::ErrorKind::InvalidData.into()
}

fn scan_header<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<CaseJournalHeader> {
    reader.step()?;
    let mut bytes = [0u8; CASE_JOURNAL_HEADER_BYTES];
    reader.read_exact(&mut bytes)?;
    // Borsh's unknown-enum diagnostic allocates. Reject the raw fixed-header
    // discriminator before invoking its otherwise fixed, allocation-free DTO
    // decoder. The exhaustive kind heap visitor also pins the schema variants.
    if bytes[CASE_JOURNAL_HEADER_BYTES - 1] > 1 {
        return Err(journal_bad("unknown case-journal event kind"));
    }
    let header: CaseJournalHeader = borsh::from_slice(&bytes).map_err(|_| journal_bad("invalid fixed case-journal header"))?;
    header.validate().map_err(|_| journal_bad("case-journal header version/sequence/previous mismatch"))?;
    Ok(header)
}

fn checked_heap(payload: u64, count: u32, preimages: u64, limits: CaseJournalLimits) -> io::Result<u64> {
    let total = payload.checked_add(preimages).ok_or_else(|| journal_bad("case-journal preimage total overflow"))?;
    let heap = u64::from(count)
        .checked_mul(size_of::<CaseJournalAux>() as u64)
        .and_then(|n| n.checked_add(total))
        .ok_or_else(|| journal_bad("case-journal logical heap overflow"))?;
    if total > limits.max_total_preimage_bytes || heap > limits.max_logical_heap_bytes {
        return Err(journal_bad("case-journal aggregate preimage/heap limit exceeded"));
    }
    Ok(total)
}

/// Scan exactly one externally length-delimited record without allocating any
/// record/payload/aux Vec. The caller sets `reader.remaining()` to that frame's
/// actual length; length is checked against policy BEFORE the first read.
/// The returned heap includes one typed inline record, owned Vec capacity
/// envelopes and one fixed decoder/comparison workspace. It excludes the raw
/// frame already retained by the caller, which must be charged separately.
/// Full native body/transaction decoding, exact required material references,
/// material domains, accepted ancestry and replay authority remain native work.
pub fn scan_positive_journal_record_v1<R: Read>(
    reader: &mut NativeWireReaderV1<R>,
    limits: CaseJournalLimits,
) -> io::Result<NativeWireShapeV1> {
    limits.validate().map_err(|_| journal_bad("case-journal limits must be explicit and nonzero"))?;
    let bytes = reader.remaining();
    if bytes > limits.max_record_bytes || bytes < CASE_JOURNAL_MIN_RECORD_BYTES as u64 {
        return Err(journal_bad("case-journal frame exceeds record bounds"));
    }
    let header = scan_header(reader)?;
    let payload = u64::from(reader.word()?);
    if payload == 0 || payload > limits.max_payload_bytes || payload.checked_add(4).is_none_or(|n| n > reader.remaining()) {
        return Err(journal_bad("case-journal payload length exceeds frame/policy"));
    }
    checked_heap(payload, 0, 0, limits)?;
    match header.kind {
        CaseJournalEventKind::NativeBody => {
            if payload < 5 {
                return Err(journal_bad("case-journal native body prefix is truncated"));
            }
            let mut prefix = [0u8; 5];
            reader.read_exact(&mut prefix)?;
            if u16::from_le_bytes([prefix[0], prefix[1]]) != CONTRACT_VERSION
                || u16::from_le_bytes([prefix[2], prefix[3]]) != CONTRACT_REVISION
                || prefix[4] > 7
            {
                return Err(journal_bad("case-journal native body version/kind mismatch"));
            }
            reader.skip(payload - 5)?;
        }
        CaseJournalEventKind::NativeTransaction => reader.skip(payload)?,
    }
    let count = reader.word()?;
    if count > limits.max_aux_entries
        || u64::from(count).checked_mul((CASE_JOURNAL_AUX_MATERIAL_BYTES + 5) as u64).is_none_or(|n| n > reader.remaining())
    {
        return Err(journal_bad("case-journal aux count exceeds frame/policy"));
    }
    checked_heap(payload, count, 0, limits)?;
    let mut previous = [0u8; CASE_JOURNAL_AUX_MATERIAL_BYTES];
    let mut has_previous = false;
    let mut preimages = 0u64;
    for _ in 0..count {
        reader.step()?;
        let mut material = [0u8; CASE_JOURNAL_AUX_MATERIAL_BYTES];
        reader.read_exact(&mut material)?;
        if has_previous && previous >= material {
            return Err(journal_bad("case-journal aux references are not strictly increasing"));
        }
        previous = material;
        has_previous = true;
        let length = u64::from(reader.word()?);
        if length == 0 || length > limits.max_aux_preimage_bytes || length > reader.remaining() {
            return Err(journal_bad("case-journal aux preimage exceeds frame/policy"));
        }
        preimages = preimages.checked_add(length).ok_or_else(|| journal_bad("case-journal aux total overflow"))?;
        checked_heap(payload, count, preimages, limits)?;
        reader.skip(length)?;
    }
    if reader.remaining() != 0 {
        return Err(journal_bad("case-journal record has trailing bytes"));
    }
    let total = checked_heap(payload, count, preimages, limits)?;
    NativeWireShapeV1 {
        collection_entries: u64::from(count),
        outer_entries: u64::from(count),
        byte_entries: total,
        ..Default::default()
    }
    .allocation(2, usize::try_from(total).map_err(|_| journal_bad("case-journal payload exceeds address space"))?)?
    .allocation(2 * u64::from(count), size_of::<CaseJournalAux>())?
    .allocation(1, size_of::<CaseJournalRecord>())?
    .allocation(1, POSITIVE_JOURNAL_DECODE_WORKSPACE_BYTES_V1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_positive_contract::journal::CaseJournalRecordView;
    use std::io::Cursor;

    fn limits() -> CaseJournalLimits {
        CaseJournalLimits {
            max_record_bytes: 16_384,
            max_payload_bytes: 1024,
            max_aux_entries: 4,
            max_aux_preimage_bytes: 1024,
            max_total_preimage_bytes: 2048,
            max_logical_heap_bytes: 8192,
        }
    }
    fn record(kind: u8, aux: &[u8]) -> Vec<u8> {
        // Fixed-width canonical header: versions at 0/2, sequence at 324,
        // kind at 612. All identity/origin fields are zero framing fixtures;
        // no accepted-origin or native authority is asserted by this codec.
        let mut bytes = vec![0u8; CASE_JOURNAL_HEADER_BYTES];
        bytes[..2].copy_from_slice(&CONTRACT_VERSION.to_le_bytes());
        bytes[2..4].copy_from_slice(&CONTRACT_REVISION.to_le_bytes());
        bytes[324..332].copy_from_slice(&1u64.to_le_bytes());
        bytes[612] = kind;
        let payload = if kind == 0 { vec![2, 0, 1, 0, 5] } else { vec![9] };
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&payload);
        bytes.extend_from_slice(&(aux.len() as u32).to_le_bytes());
        for &tag in aux {
            let mut reference = [0u8; CASE_JOURNAL_AUX_MATERIAL_BYTES];
            reference[0] = tag;
            bytes.extend_from_slice(&reference);
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.push(tag);
        }
        bytes
    }
    fn scan(bytes: &[u8], limits: CaseJournalLimits) -> io::Result<NativeWireShapeV1> {
        scan_positive_journal_record_v1(&mut NativeWireReaderV1::new(bytes, bytes.len() as u64), limits)
    }

    #[test]
    fn explicit_stream_matches_shared_framing_and_reserves_short_event_inline() {
        for kind in 0..=1 {
            for aux in [&[][..], &[1, 2][..]] {
                let bytes = record(kind, aux);
                let quote = CaseJournalRecordView::decode(&bytes, limits()).unwrap().quote();
                let shape = scan(&bytes, limits()).unwrap();
                assert_eq!(shape.byte_entries, quote.total_preimage_bytes);
                assert_eq!(shape.collection_entries, u64::from(quote.aux_entries));
                assert_eq!(
                    shape.heap,
                    2 * quote.logical_heap_bytes
                        + size_of::<CaseJournalRecord>() as u64
                        + POSITIVE_JOURNAL_DECODE_WORKSPACE_BYTES_V1 as u64
                );
            }
        }
    }

    #[test]
    fn record_limit_and_missing_generic_policy_refuse_before_reading() {
        let bytes = record(1, &[]);
        let mut policy = limits();
        policy.max_record_bytes = bytes.len() as u64 - 1;
        let mut reader = NativeWireReaderV1::new(Cursor::new(&bytes), bytes.len() as u64);
        assert!(scan_positive_journal_record_v1(&mut reader, policy).is_err());
        assert_eq!(reader.into_inner().position(), 0);
        let mut reader = NativeWireReaderV1::new(Cursor::new(&bytes), bytes.len() as u64);
        assert_eq!(CaseJournalRecord::scan_native_wire_v1(&mut reader).unwrap_err().kind(), io::ErrorKind::Unsupported);
        assert_eq!(reader.into_inner().position(), 0);
    }

    #[test]
    fn streamed_negative_cases_agree_with_shared_borrowed_codec() {
        let valid = record(0, &[1, 2]);
        let mut bad = Vec::new();
        bad.push(record(1, &[2, 1]));
        bad.push(record(1, &[1, 1]));
        bad.push(record(2, &[]));
        bad.push(valid[..valid.len() - 1].to_vec());
        let mut trailing = valid.clone();
        trailing.push(0);
        bad.push(trailing);
        let mut sequence = valid.clone();
        sequence[324..332].fill(0);
        bad.push(sequence);
        let mut previous = valid.clone();
        previous[332] = 1;
        bad.push(previous);
        let mut native_kind = valid;
        native_kind[CASE_JOURNAL_HEADER_BYTES + 4 + 4] = 8;
        bad.push(native_kind);
        for bytes in bad {
            assert!(CaseJournalRecordView::decode(&bytes, limits()).is_err());
            assert!(scan(&bytes, limits()).is_err());
        }
    }

    #[test]
    fn declared_count_and_aggregate_limits_bound_work_before_iteration() {
        let bytes = record(1, &[1, 2]);
        for policy in [
            CaseJournalLimits { max_aux_entries: 1, ..limits() },
            CaseJournalLimits { max_total_preimage_bytes: 2, ..limits() },
            CaseJournalLimits { max_logical_heap_bytes: 1, ..limits() },
            CaseJournalLimits { max_aux_preimage_bytes: 0, ..limits() },
        ] {
            assert!(CaseJournalRecordView::decode(&bytes, policy).is_err());
            assert!(scan(&bytes, policy).is_err());
        }
        let mut huge_count = record(1, &[]);
        let at = huge_count.len() - 4;
        huge_count[at..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(scan(&huge_count, CaseJournalLimits { max_aux_entries: u32::MAX, ..limits() }).is_err());
    }
}
