//! Canonical per-case accepted-event journal, revision 1.
//!
//! This codec establishes framing and data identity only. Native owners must
//! authenticate the complete accepted prefix, canonical transaction/body and
//! every material preimage in its own domain. Neither a decoded record nor its
//! hashes confer admission, computation, funding, settlement or credit authority.
//! Only state/cost-changing accepted events are appended; fully idempotent
//! deliveries do not acquire a new event merely by changing their carrier ID.
use crate::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{CaseId, ChainScope, Digest, ResultId},
    rows::{AcceptedMaterial, AcceptedOrigin},
};
use borsh::{BorshDeserialize, BorshSerialize};

/// Fixed header through the local event-kind byte, with no magic/global tag.
pub const CASE_JOURNAL_HEADER_BYTES: usize = 613;
/// Header plus the two u32 lengths/counts, before any payload or aux entries.
/// A valid record additionally has a nonempty payload.
pub const CASE_JOURNAL_MIN_RECORD_BYTES: usize = CASE_JOURNAL_HEADER_BYTES + 8;
pub const CASE_JOURNAL_AUX_MATERIAL_BYTES: usize = 280;
pub const CASE_JOURNAL_EVENT_DOMAIN: &[u8] = b"misaka/positive/case-journal-event/v2/r1";
pub const CASE_JOURNAL_HISTORY_DOMAIN: &[u8] = b"misaka/positive/case-journal-history/v2/r1";

/// Local journal discriminator, not a new native operation or lifecycle tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum CaseJournalEventKind {
    NativeBody,
    NativeTransaction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CaseJournalHeader {
    pub version: u16,
    pub revision: u16,
    pub scope: ChainScope,
    pub case: CaseId,
    pub result: ResultId,
    pub sequence: u64,
    pub previous: Digest,
    pub origin: AcceptedOrigin,
    pub kind: CaseJournalEventKind,
}
impl CaseJournalHeader {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        if self.sequence == 0 {
            return Err(ContractError::Range);
        }
        if self.sequence == 1 && self.previous != Digest([0; 64]) {
            return Err(ContractError::NonCanonical);
        }
        Ok(())
    }
}

/// Owned encoding DTO only. No allocating BorshDeserialize is provided.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct CaseJournalAux {
    pub material: AcceptedMaterial,
    pub preimage: Vec<u8>,
}
/// Owned encoding DTO, not a recovered-state capability. Decoding uses the
/// borrowed scanner below; callers separately reserve any owned copies.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct CaseJournalRecord {
    pub header: CaseJournalHeader,
    pub payload: Vec<u8>,
    pub aux: Vec<CaseJournalAux>,
}

/// All limits are explicit, finite, nonzero caller policy. A zero aux count is
/// nevertheless valid. Logical heap is ADDITIONAL owned storage, excluding the
/// already retained raw bytes, inline record and fixed workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseJournalLimits {
    pub max_record_bytes: u64,
    pub max_payload_bytes: u64,
    pub max_aux_entries: u32,
    pub max_aux_preimage_bytes: u64,
    /// Sum of the main payload and all aux preimages, excluding framing/refs.
    pub max_total_preimage_bytes: u64,
    pub max_logical_heap_bytes: u64,
}
impl CaseJournalLimits {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.max_record_bytes == 0
            || self.max_payload_bytes == 0
            || self.max_aux_entries == 0
            || self.max_aux_preimage_bytes == 0
            || self.max_total_preimage_bytes == 0
            || self.max_logical_heap_bytes == 0
        {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseJournalQuote {
    pub record_bytes: u64,
    pub payload_bytes: u64,
    pub aux_entries: u32,
    pub aux_preimage_bytes: u64,
    pub total_preimage_bytes: u64,
    /// payload + aux preimages + aux Vec element storage; not raw bytes again.
    pub logical_heap_bytes: u64,
    pub inline_record_bytes: u64,
    /// Logical fixed workspace for one header, one decoded material and two
    /// canonical reference buffers. Not a compiler stack-frame/RSS guarantee.
    pub decode_workspace_bytes: u64,
}

fn quote(
    record_bytes: u64,
    payload_bytes: u64,
    aux_entries: u32,
    aux_preimage_bytes: u64,
    limits: CaseJournalLimits,
) -> Result<CaseJournalQuote, ContractError> {
    let total_preimage_bytes = payload_bytes.checked_add(aux_preimage_bytes).ok_or(ContractError::Budget)?;
    let logical_heap_bytes = (aux_entries as u64)
        .checked_mul(std::mem::size_of::<CaseJournalAux>() as u64)
        .and_then(|n| n.checked_add(total_preimage_bytes))
        .ok_or(ContractError::Budget)?;
    if record_bytes > limits.max_record_bytes
        || payload_bytes > limits.max_payload_bytes
        || aux_entries > limits.max_aux_entries
        || total_preimage_bytes > limits.max_total_preimage_bytes
        || logical_heap_bytes > limits.max_logical_heap_bytes
    {
        return Err(ContractError::Budget);
    }
    Ok(CaseJournalQuote {
        record_bytes,
        payload_bytes,
        aux_entries,
        aux_preimage_bytes,
        total_preimage_bytes,
        logical_heap_bytes,
        inline_record_bytes: std::mem::size_of::<CaseJournalRecord>() as u64,
        decode_workspace_bytes: (std::mem::size_of::<CaseJournalHeader>()
            + std::mem::size_of::<AcceptedMaterial>()
            + 2 * CASE_JOURNAL_AUX_MATERIAL_BYTES) as u64,
    })
}

/// Header bytes borrowed from a fully scanned record, decoded only on demand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseJournalHeaderView<'a> {
    bytes: &'a [u8],
}
impl<'a> CaseJournalHeaderView<'a> {
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
    pub fn decode(&self) -> CaseJournalHeader {
        borsh::from_slice(self.bytes).expect("prevalidated fixed journal header")
    }
}

/// A single borrowed aux item. Its material is a fixed-size DTO, never a receipt
/// capability; native resolution must authenticate its preimage and domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseJournalAuxView<'a> {
    material: &'a [u8],
    preimage: &'a [u8],
}
impl<'a> CaseJournalAuxView<'a> {
    pub fn material_bytes(&self) -> &'a [u8] {
        self.material
    }
    pub fn material(&self) -> AcceptedMaterial {
        borsh::from_slice(self.material).expect("prevalidated fixed accepted material")
    }
    pub fn preimage(&self) -> &'a [u8] {
        self.preimage
    }
}

/// Allocation-free framing scan. Neither a Vec of decoded events nor a Vec of
/// aux references is retained; iteration borrows this record's original bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseJournalRecordView<'a> {
    raw: &'a [u8],
    header: CaseJournalHeaderView<'a>,
    payload: &'a [u8],
    aux: &'a [u8],
    quote: CaseJournalQuote,
}
impl<'a> CaseJournalRecordView<'a> {
    pub fn decode(bytes: &'a [u8], limits: CaseJournalLimits) -> Result<Self, ContractError> {
        limits.validate()?;
        let raw_bytes = u64::try_from(bytes.len()).map_err(|_| ContractError::Budget)?;
        if raw_bytes > limits.max_record_bytes {
            return Err(ContractError::Budget);
        }
        let mut reader = Reader { remaining: bytes };
        let header_bytes = reader.take(CASE_JOURNAL_HEADER_BYTES)?;
        // Borsh formats an allocating error for an unknown enum discriminant.
        // Reject the only fallible fixed-header shape before invoking it.
        if header_bytes[CASE_JOURNAL_HEADER_BYTES - 1] > 1 {
            return Err(ContractError::NonCanonical);
        }
        let header: CaseJournalHeader = borsh::from_slice(header_bytes).map_err(|_| ContractError::NonCanonical)?;
        header.validate()?;
        let payload_len = reader.u32()? as u64;
        if payload_len > limits.max_payload_bytes {
            return Err(ContractError::Budget);
        }
        let payload = reader.take(usize::try_from(payload_len).map_err(|_| ContractError::Budget)?)?;
        validate_payload(header.kind, payload)?;
        let aux_count = reader.u32()?;
        // Check declared-count work/heap before iterating or decoding any aux.
        quote(raw_bytes, payload_len, aux_count, 0, limits)?;
        let minimum_aux =
            (aux_count as u64).checked_mul((CASE_JOURNAL_AUX_MATERIAL_BYTES + 4 + 1) as u64).ok_or(ContractError::Budget)?;
        if minimum_aux > reader.remaining.len() as u64 {
            return Err(ContractError::NonCanonical);
        }
        let aux = reader.remaining;
        let mut previous: Option<&[u8]> = None;
        let mut aux_bytes = 0u64;
        for _ in 0..aux_count {
            let material = reader.take(CASE_JOURNAL_AUX_MATERIAL_BYTES)?;
            if previous.is_some_and(|last| last >= material) {
                return Err(ContractError::NonCanonical);
            }
            previous = Some(material);
            let len = reader.u32()? as u64;
            if len == 0 {
                return Err(ContractError::NonCanonical);
            }
            if len > limits.max_aux_preimage_bytes {
                return Err(ContractError::Budget);
            }
            aux_bytes = aux_bytes.checked_add(len).ok_or(ContractError::Budget)?;
            quote(raw_bytes, payload_len, aux_count, aux_bytes, limits)?;
            reader.take(usize::try_from(len).map_err(|_| ContractError::Budget)?)?;
        }
        if !reader.remaining.is_empty() {
            return Err(ContractError::NonCanonical);
        }
        let quote = quote(raw_bytes, payload_len, aux_count, aux_bytes, limits)?;
        Ok(Self { raw: bytes, header: CaseJournalHeaderView { bytes: header_bytes }, payload, aux, quote })
    }
    pub fn raw_bytes(&self) -> &'a [u8] {
        self.raw
    }
    pub fn header_view(&self) -> CaseJournalHeaderView<'a> {
        self.header
    }
    pub fn header(&self) -> CaseJournalHeader {
        self.header.decode()
    }
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }
    pub fn aux_count(&self) -> u32 {
        self.quote.aux_entries
    }
    pub fn quote(&self) -> CaseJournalQuote {
        self.quote
    }
    pub fn aux(&self) -> CaseJournalAuxIter<'a> {
        CaseJournalAuxIter { reader: Reader { remaining: self.aux }, remaining: self.aux_count() }
    }
    /// Identity of the canonical FULL record, with no additional Vec length
    /// prefix and no head field recursively included in the record itself.
    pub fn head(&self) -> Digest {
        hash_raw(CASE_JOURNAL_EVENT_DOMAIN, self.raw)
    }
    /// The origin is the record origin; the history digest has its own domain
    /// over the SAME full record, not an alias of head or payload digest.
    pub fn history(&self) -> AcceptedMaterial {
        AcceptedMaterial { origin: self.header().origin, digest: hash_raw(CASE_JOURNAL_HISTORY_DOMAIN, self.raw) }
    }
    /// Native owner derives the exact required canonical sorted reference list.
    /// Missing, extra, duplicate, reordered or different references fail. This
    /// does not authenticate any material preimage or its semantic hash domain.
    pub fn require_exact_refs(&self, expected: &[AcceptedMaterial]) -> Result<(), ContractError> {
        if expected.len() as u64 != self.aux_count() as u64 {
            return Err(ContractError::Identity);
        }
        for (actual, expected) in self.aux().zip(expected) {
            let mut encoded = [0u8; CASE_JOURNAL_AUX_MATERIAL_BYTES];
            expected.serialize(&mut encoded.as_mut_slice()).map_err(|_| ContractError::NonCanonical)?;
            if actual.material_bytes() != encoded {
                return Err(ContractError::Identity);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CaseJournalAuxIter<'a> {
    reader: Reader<'a>,
    remaining: u32,
}
impl<'a> Iterator for CaseJournalAuxIter<'a> {
    type Item = CaseJournalAuxView<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let material = self.reader.take(CASE_JOURNAL_AUX_MATERIAL_BYTES).expect("prevalidated aux reference");
        let len = self.reader.u32().expect("prevalidated aux length") as usize;
        let preimage = self.reader.take(len).expect("prevalidated aux preimage");
        self.remaining -= 1;
        Some(CaseJournalAuxView { material, preimage })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining as usize;
        (remaining, Some(remaining))
    }
}
impl ExactSizeIterator for CaseJournalAuxIter<'_> {}
impl std::iter::FusedIterator for CaseJournalAuxIter<'_> {}

impl CaseJournalRecord {
    /// Validate and quote before allocating output. The caller already owns
    /// this DTO; its input allocation is not hidden by the output/raw quote.
    pub fn encode(&self, limits: CaseJournalLimits) -> Result<Vec<u8>, ContractError> {
        limits.validate()?;
        self.header.validate()?;
        validate_payload(self.header.kind, &self.payload)?;
        let payload_len = u32::try_from(self.payload.len()).map_err(|_| ContractError::Budget)?;
        let aux_count = u32::try_from(self.aux.len()).map_err(|_| ContractError::Budget)?;
        let mut raw_bytes = (CASE_JOURNAL_MIN_RECORD_BYTES as u64)
            .checked_add(payload_len as u64)
            .and_then(|n| n.checked_add((aux_count as u64).checked_mul((CASE_JOURNAL_AUX_MATERIAL_BYTES + 4) as u64)?))
            .ok_or(ContractError::Budget)?;
        quote(raw_bytes, payload_len as u64, aux_count, 0, limits)?;
        let mut previous = [0u8; CASE_JOURNAL_AUX_MATERIAL_BYTES];
        let mut has_previous = false;
        let mut aux_bytes = 0u64;
        for aux in &self.aux {
            let len = u32::try_from(aux.preimage.len()).map_err(|_| ContractError::Budget)?;
            if len == 0 {
                return Err(ContractError::NonCanonical);
            }
            if len as u64 > limits.max_aux_preimage_bytes {
                return Err(ContractError::Budget);
            }
            let mut material = [0u8; CASE_JOURNAL_AUX_MATERIAL_BYTES];
            aux.material.serialize(&mut material.as_mut_slice()).map_err(|_| ContractError::NonCanonical)?;
            if has_previous && previous >= material {
                return Err(ContractError::NonCanonical);
            }
            previous = material;
            has_previous = true;
            raw_bytes = raw_bytes.checked_add(len as u64).ok_or(ContractError::Budget)?;
            aux_bytes = aux_bytes.checked_add(len as u64).ok_or(ContractError::Budget)?;
            quote(raw_bytes, payload_len as u64, aux_count, aux_bytes, limits)?;
        }
        let capacity = usize::try_from(raw_bytes).map_err(|_| ContractError::Budget)?;
        let mut out = Vec::new();
        out.try_reserve_exact(capacity).map_err(|_| ContractError::Budget)?;
        if out.capacity() as u64 > limits.max_record_bytes {
            return Err(ContractError::Budget);
        }
        self.serialize(&mut out).map_err(|_| ContractError::NonCanonical)?;
        debug_assert_eq!(out.len(), capacity);
        Ok(out)
    }
}

fn validate_payload(kind: CaseJournalEventKind, payload: &[u8]) -> Result<(), ContractError> {
    if payload.is_empty() {
        return Err(ContractError::NonCanonical);
    }
    if kind == CaseJournalEventKind::NativeBody {
        if payload.len() < 5 {
            return Err(ContractError::NonCanonical);
        }
        if u16::from_le_bytes([payload[0], payload[1]]) != CONTRACT_VERSION
            || u16::from_le_bytes([payload[2], payload[3]]) != CONTRACT_REVISION
        {
            return Err(ContractError::Version);
        }
        if payload[4] > 7 {
            return Err(ContractError::NonCanonical);
        }
    }
    // Full native body/transaction canonical decoding belongs to native code.
    Ok(())
}

fn hash_raw(domain: &[u8], raw: &[u8]) -> Digest {
    Digest(blake2b_simd::Params::new().hash_length(64).key(domain).hash(raw).as_bytes().try_into().expect("64-byte digest"))
}

#[derive(Clone, Debug)]
struct Reader<'a> {
    remaining: &'a [u8],
}
impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], ContractError> {
        let bytes = self.remaining.get(..length).ok_or(ContractError::NonCanonical)?;
        self.remaining = &self.remaining[length..];
        Ok(bytes)
    }
    fn u32(&mut self) -> Result<u32, ContractError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| ContractError::NonCanonical)?))
    }
}
