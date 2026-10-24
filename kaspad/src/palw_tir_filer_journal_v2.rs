//! same-wire staged journal codec and local resource owners.
//! Loaded evidence is opaque; no duty, signer, payment or claim authority is restored.
use super::{PALW_TIR_PUBLIC_FILER_MAGIC_V1, PALW_TIR_PUBLIC_FILER_PENDING_V1, PalwTirPublicPendingFilingV1};
use crate::palw_memory_ledger::{PalwMemoryReservationKeyV1, PalwMemoryReservationV1, host_ledger_v1};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_court_v2::PalwCourtVerdictProofV2,
    palw_native_resource_v1::{
        NativeDemandLeaseV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, preflight_native_value_v1,
        reserve_native_requests_v1,
    },
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
    palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,
};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
};

#[derive(Debug)]
pub(crate) enum JournalFaultV2 {
    Resource(String),
    Io(String),
    Changed,
    Malformed(String),
}
impl std::fmt::Display for JournalFaultV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resource(e) => write!(f, "journal local resource: {e}"),
            Self::Io(e) => write!(f, "journal IO: {e}"),
            Self::Changed => write!(f, "journal changed during read"),
            Self::Malformed(e) => write!(f, "malformed journal: {e}"),
        }
    }
}
fn codec(e: io::Error) -> JournalFaultV2 {
    if matches!(e.kind(), io::ErrorKind::OutOfMemory | io::ErrorKind::Unsupported | io::ErrorKind::TimedOut) {
        JournalFaultV2::Resource(e.to_string())
    } else {
        JournalFaultV2::Malformed(e.to_string())
    }
}
fn invalid(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}
fn reserve(bytes: u64, role: &'static str) -> Result<(Option<NativeDemandLeaseV1>, PalwMemoryReservationV1), JournalFaultV2> {
    let native = reserve_native_requests_v1(bytes).map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
    let host = host_ledger_v1()
        .reserve(PalwMemoryReservationKeyV1 { role, class_id: Default::default(), job: Default::default() }, bytes)
        .map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
    Ok((native, host))
}
/// Actual byte buffer and both admission families; storage drops first.
pub(super) struct JournalBytesV2 {
    pub bytes: Vec<u8>,
    native: Option<NativeDemandLeaseV1>,
    host: PalwMemoryReservationV1,
    file: Option<(std::fs::File, FileStampV2)>,
}
struct JournalBodyV2(Vec<PalwTirPublicPendingFilingV1>);
impl NativeWireScanV1 for JournalBodyV2 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        r.step()?;
        let count = r.word()? as u64;
        if count > PALW_TIR_PUBLIC_FILER_PENDING_V1 as u64 {
            return Err(invalid("pending journal count exceeds existing sixteen-entry schema"));
        }
        let mut shape = NativeWireShapeV1::default().allocation(count, std::mem::size_of::<PalwTirPublicPendingFilingV1>())?;
        for _ in 0..count {
            shape = shape.merge(Hash64::scan_native_wire_v1(r)?)?;
            shape = shape.merge(PalwTirPublicConvictionEvidenceV1::scan_native_wire_v1(r)?)?;
            shape = shape.merge(u64::scan_native_wire_v1(r)?)?;
            shape = shape.merge(u8::scan_native_wire_v1(r)?)?;
            shape = shape.merge(u64::scan_native_wire_v1(r)?)?;
        }
        Ok(shape)
    }
}
impl BorshDeserialize for JournalBodyV2 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let count = u32::deserialize_reader(r)? as usize;
        if count > PALW_TIR_PUBLIC_FILER_PENDING_V1 {
            return Err(invalid("pending journal count exceeds schema"));
        }
        let mut entries = Vec::new();
        entries.try_reserve_exact(count).map_err(|e| io::Error::new(io::ErrorKind::OutOfMemory, e.to_string()))?;
        for _ in 0..count {
            let claim = Hash64::deserialize_reader(r)?;
            // This is the PRIVATE journal codec, byte-identical to the public
            // enum but not its single-control Output-vector policy bound.
            let evidence = match u8::deserialize_reader(r)? {
                0 => {
                    PalwTirPublicConvictionEvidenceV1::Arithmetic { proof: Box::new(PalwCourtVerdictProofV2::deserialize_reader(r)?) }
                }
                1 => {
                    let rows_root = Hash64::deserialize_reader(r)?;
                    let count = u32::deserialize_reader(r)? as usize;
                    let mut ids = Vec::new();
                    ids.try_reserve_exact(count).map_err(|e| io::Error::new(io::ErrorKind::OutOfMemory, e.to_string()))?;
                    for _ in 0..count {
                        ids.push(u32::deserialize_reader(r)?);
                    }
                    PalwTirPublicConvictionEvidenceV1::Output { rows_root, ids }
                }
                2 => {
                    PalwTirPublicConvictionEvidenceV1::DecodeToken { proof: Box::new(PalwCourtVerdictProofV2::deserialize_reader(r)?) }
                }
                _ => return Err(invalid("unknown journal evidence tag")),
            };
            entries.push(PalwTirPublicPendingFilingV1 {
                claim,
                evidence,
                found_daa: u64::deserialize_reader(r)?,
                sends: u8::deserialize_reader(r)?,
                staged_offset: u64::deserialize_reader(r)?,
            });
        }
        Ok(Self(entries))
    }
}
/// Retained expanded storage; this token has no accepted-state authority.
pub(super) struct JournalDecodedV2 {
    pub entries: Vec<PalwTirPublicPendingFilingV1>,
    pub native: Option<NativeDemandLeaseV1>,
    pub host: PalwMemoryReservationV1,
}
pub(super) fn decode_reserved_v2(raw: &JournalBytesV2) -> Result<JournalDecodedV2, JournalFaultV2> {
    let body = raw
        .bytes
        .strip_prefix(PALW_TIR_PUBLIC_FILER_MAGIC_V1)
        .ok_or_else(|| JournalFaultV2::Malformed("unknown journal magic".into()))?;
    let shape = preflight_native_value_v1::<JournalBodyV2>(body, body.len() as u64).map_err(codec)?;
    let (native, host) = reserve(shape.logical_requests_v1(), "public-tir-journal-decoded")?;
    // Existing check_staged clones at most one proof. A second complete TYPED
    // decoded-body envelope bounds that copy, independently of codec length.
    let _check = reserve(shape.logical_requests_v1(), "public-tir-journal-shape-scratch")?;
    let JournalBodyV2(entries) = borsh::from_slice(body).map_err(codec)?;
    for entry in &entries {
        entry.evidence.check_staged_v1().map_err(JournalFaultV2::Malformed)?;
    }
    Ok(JournalDecodedV2 { entries, native, host })
}
struct BorrowedJournalV2<'a>(&'a BTreeMap<Hash64, PalwTirPublicPendingFilingV1>);
impl BorshSerialize for BorrowedJournalV2<'_> {
    fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
        u32::try_from(self.0.len()).map_err(|_| invalid("journal count overflow"))?.serialize(w)?;
        for entry in self.0.values() {
            entry.serialize(w)?;
        }
        Ok(())
    }
}
pub(super) fn encode_reserved_v2(entries: &BTreeMap<Hash64, PalwTirPublicPendingFilingV1>) -> Result<JournalBytesV2, JournalFaultV2> {
    if entries.len() > PALW_TIR_PUBLIC_FILER_PENDING_V1 {
        return Err(JournalFaultV2::Malformed("journal entry count exceeds schema".into()));
    }
    let view = BorrowedJournalV2(entries);
    let body = borsh::object_length(&view).map_err(codec)?;
    let total = body
        .checked_add(PALW_TIR_PUBLIC_FILER_MAGIC_V1.len())
        .ok_or_else(|| JournalFaultV2::Resource("journal encoded length overflow".into()))?;
    let mut q = PalwMaterialConstructionRequestsV2::new();
    q.vec_v1::<u8>(total as u64).map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
    let (native, host) = reserve(q.bytes_v1(), "public-tir-journal-encoded")?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(total).map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
    bytes.extend_from_slice(PALW_TIR_PUBLIC_FILER_MAGIC_V1);
    borsh::to_writer(&mut bytes, &view).map_err(codec)?;
    if bytes.len() != total {
        return Err(JournalFaultV2::Changed);
    }
    Ok(JournalBytesV2 { bytes, native, host, file: None })
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileStampV2 {
    len: u64,
    modified: std::time::SystemTime,
    dev: u64,
    ino: u64,
}
fn stamp(meta: &std::fs::Metadata) -> Result<FileStampV2, JournalFaultV2> {
    if !meta.is_file() {
        return Err(JournalFaultV2::Io("journal is not a regular file".into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(FileStampV2 {
            len: meta.len(),
            modified: meta.modified().map_err(|e| JournalFaultV2::Io(e.to_string()))?,
            dev: meta.dev(),
            ino: meta.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        Err(JournalFaultV2::Resource("journal stable-file identity is unsupported on this target".into()))
    }
}
fn read_reserved_with_v2(path: &std::path::Path, after_stat: impl FnOnce()) -> Result<Option<JournalBytesV2>, JournalFaultV2> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(JournalFaultV2::Io(e.to_string())),
    };
    let before = stamp(&file.metadata().map_err(|e| JournalFaultV2::Io(e.to_string()))?)?;
    let size =
        usize::try_from(before.len).map_err(|_| JournalFaultV2::Resource("journal length exceeds target address space".into()))?;
    let mut q = PalwMaterialConstructionRequestsV2::new();
    q.vec_v1::<u8>(before.len).map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
    let (native, host) = reserve(q.bytes_v1(), "public-tir-journal-file")?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(size).map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
    bytes.resize(size, 0);
    after_stat();
    if let Err(e) = file.read_exact(&mut bytes) {
        return Err(if e.kind() == io::ErrorKind::UnexpectedEof {
            JournalFaultV2::Changed
        } else {
            JournalFaultV2::Io(e.to_string())
        });
    }
    let mut extra = [0u8; 1];
    if file.read(&mut extra).map_err(|e| JournalFaultV2::Io(e.to_string()))? != 0 {
        return Err(JournalFaultV2::Changed);
    }
    let after = stamp(&file.metadata().map_err(|e| JournalFaultV2::Io(e.to_string()))?)?;
    let current = stamp(&std::fs::metadata(path).map_err(|e| JournalFaultV2::Io(e.to_string()))?)?;
    if before != after || before != current {
        return Err(JournalFaultV2::Changed);
    }
    Ok(Some(JournalBytesV2 { bytes, native, host, file: Some((file, before)) }))
}
fn quarantine_stable_v2(path: &std::path::Path, raw: &JournalBytesV2) -> Result<(), JournalFaultV2> {
    let Some((file, before)) = &raw.file else { return Err(JournalFaultV2::Changed) };
    let after = stamp(&file.metadata().map_err(|e| JournalFaultV2::Io(e.to_string()))?)?;
    let current = stamp(&std::fs::metadata(path).map_err(|e| JournalFaultV2::Io(e.to_string()))?)?;
    if *before != after || *before != current {
        return Err(JournalFaultV2::Changed);
    }
    std::fs::rename(path, path.with_extension("v1.unreadable")).map_err(|e| JournalFaultV2::Io(e.to_string()))
}
pub(super) fn load_attempt_v2(path: &std::path::Path) -> Result<Option<JournalDecodedV2>, JournalFaultV2> {
    let Some(raw) = read_reserved_with_v2(path, || {})? else { return Ok(None) };
    match decode_reserved_v2(&raw) {
        Ok(decoded) => {
            // Persisted source is a map: duplicate keys were never a legitimate
            // encoder output, and may not silently replace an earlier finding.
            for (i, entry) in decoded.entries.iter().enumerate() {
                if decoded.entries[..i].iter().any(|old| old.claim == entry.claim) {
                    quarantine_stable_v2(path, &raw)?;
                    return Ok(None);
                }
            }
            Ok(Some(decoded))
        }
        Err(JournalFaultV2::Malformed(_)) => {
            quarantine_stable_v2(path, &raw)?;
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
/// Pooled payload quota is referenced only by the entries loaded from it.
/// The last loaded entry's removal releases it, independently of book capacity.
pub(super) struct ReloadPayloadV2 {
    _native: Option<NativeDemandLeaseV1>,
    _host: PalwMemoryReservationV1,
}
impl JournalDecodedV2 {
    pub(super) fn into_parts_v2(self) -> (Vec<PalwTirPublicPendingFilingV1>, ReloadPayloadV2) {
        (self.entries, ReloadPayloadV2 { _native: self.native, _host: self.host })
    }
}
pub(super) fn write_atomic_v2(path: &std::path::Path, raw: &JournalBytesV2) -> Result<(), JournalFaultV2> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| JournalFaultV2::Io(e.to_string()))?;
    }
    let temporary = path.with_extension("v1.tmp");
    std::fs::write(&temporary, &raw.bytes)
        .and_then(|()| std::fs::rename(&temporary, path))
        .map_err(|e| JournalFaultV2::Io(e.to_string()))
}

#[cfg(test)]
#[path = "palw_tir_filer_journal_tests_v2.rs"]
mod tests;
