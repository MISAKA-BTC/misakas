//! Literal, ordered source history retention. This is a cache, not accepted
//! provenance authority. Source op3/op4 material digest is the ACTUAL carrier
//! TxId under A0's source-specific rule; other material domains are unchanged.
use super::*;
use borsh::BorshSerialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceHistoryKindV2 {
    Request,
    Response,
}
pub struct SourceHistoryEntryV2 {
    pub material: AcceptedMaterial,
    pub kind: SourceHistoryKindV2,
    body: Vec<u8>,
}
impl SourceHistoryEntryV2 {
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}
#[derive(Default)]
pub struct NativeSourceHistoryV2 {
    entries: Vec<SourceHistoryEntryV2>,
    origins: BTreeMap<[u8; 216], usize>,
    bytes: u64,
}
impl NativeSourceHistoryV2 {
    pub fn entries(&self) -> &[SourceHistoryEntryV2] {
        &self.entries
    }
    pub fn retained_bytes(&self) -> u64 {
        self.bytes
    }
    pub fn resolve(&self, material: &AcceptedMaterial) -> Result<&[u8], SourceErrorV2> {
        let key = origin_key(material)?;
        let entry = self.origins.get(&key).and_then(|index| self.entries.get(*index)).ok_or(SourceErrorV2::History)?;
        if entry.material != *material {
            return Err(SourceErrorV2::Binding);
        }
        Ok(entry.body())
    }
    /// Checks literal identity/origin/order without retaining the delivery or
    /// consuming history capacity. NoOp carriers still pass this check.
    pub(super) fn check(&self, material: AcceptedMaterial, kind: SourceHistoryKindV2, body: &[u8]) -> Result<bool, SourceErrorV2> {
        // Existing native source op3/op4 carriers contain one court operation.
        if material.origin.event_ordinal != 0 || material.digest.0 != material.origin.carrier_transaction.0 {
            return Err(SourceErrorV2::Binding);
        }
        let key = origin_key(&material)?;
        if let Some(index) = self.origins.get(&key) {
            let entry = &self.entries[*index];
            if entry.material != material || entry.kind != kind || entry.body() != body {
                return Err(SourceErrorV2::Binding);
            }
            return Ok(false);
        }
        if let Some(last) = self.entries.last() {
            let prior = last.material.origin;
            let next = material.origin;
            let ordered = if prior.accepting_block == next.accepting_block {
                prior.accepted_daa == next.accepted_daa
                    && prior.accepted_blue_score == next.accepted_blue_score
                    && (prior.accepted_tx_ordinal, prior.event_ordinal) < (next.accepted_tx_ordinal, next.event_ordinal)
            } else {
                prior.accepted_blue_score < next.accepted_blue_score && prior.accepted_daa <= next.accepted_daa
            };
            if !ordered {
                return Err(SourceErrorV2::Order);
            }
        }
        Ok(true)
    }
    pub(super) fn prepare(
        &self,
        material: AcceptedMaterial,
        kind: SourceHistoryKindV2,
        body: &[u8],
        limits: SourceLimitsV2,
    ) -> Result<Option<SourceHistoryEntryV2>, SourceErrorV2> {
        if !self.check(material, kind, body)? {
            return Ok(None);
        }
        if body.is_empty()
            || body.len() as u64 > limits.max_native_body_bytes
            || self.entries.len() as u64 >= limits.max_history_events
            || self.bytes.checked_add(body.len() as u64).filter(|n| *n <= limits.max_history_bytes).is_none()
        {
            return Err(SourceErrorV2::Budget);
        }
        let mut retained = Vec::new();
        retained.try_reserve_exact(body.len()).map_err(|_| SourceErrorV2::Budget)?;
        retained.extend_from_slice(body);
        Ok(Some(SourceHistoryEntryV2 { material, kind, body: retained }))
    }
    pub(super) fn apply(&mut self, entry: SourceHistoryEntryV2) {
        let key = origin_key(&entry.material).expect("fixed accepted origin serializes");
        self.bytes += entry.body.len() as u64;
        self.origins.insert(key, self.entries.len());
        self.entries.push(entry);
    }
}
fn origin_key(material: &AcceptedMaterial) -> Result<[u8; 216], SourceErrorV2> {
    // Existing AcceptedOrigin Borsh fields: three 64-byte hashes, two u32,
    // two u64. Reuse those exact bytes without allocating on NoOp checks.
    let mut key = [0u8; 216];
    let mut remaining = &mut key[..];
    material.origin.serialize(&mut remaining).map_err(|_| SourceErrorV2::History)?;
    if !remaining.is_empty() {
        return Err(SourceErrorV2::History);
    }
    Ok(key)
}

/// Checks A4's existing op3 literal, without assigning a new tag or domain.
pub(super) fn check_request_literal(event: &SourceRequestEventV2<'_>) -> Result<(), SourceErrorV2> {
    let mut expected = [0u8; 4909];
    expected[..5].copy_from_slice(&[2, 0, 1, 0, 3]);
    let mut remaining = &mut expected[5..];
    event.request.serialize(&mut remaining).map_err(|_| SourceErrorV2::History)?;
    event.requester.serialize(&mut remaining).map_err(|_| SourceErrorV2::History)?;
    std::io::Write::write_all(&mut remaining, event.signature).map_err(|_| SourceErrorV2::History)?;
    if !remaining.is_empty() || event.native_body != expected {
        return Err(SourceErrorV2::Binding);
    }
    Ok(())
}
/// op4 carries only case + accepted RequestId + raw existing-G14 Vec body.
/// Provider identity must come from actual carrier/delegation authority.
pub(super) fn response_data<'a>(case: CaseId, event: &SourceResponseEventV2<'a>) -> Result<&'a [u8], SourceErrorV2> {
    let (request, raw) = response_body_data(case, event.native_body)?;
    if request != event.request || event.g14_data_range != (137, event.native_body.len() as u64) {
        return Err(SourceErrorV2::Binding);
    }
    Ok(raw)
}
/// Borrow the raw G14 bytes from a retained, literal native op4. No copies or
/// new request/receipt identities are created when comparing a completed unit.
pub(crate) fn response_body_data(case: CaseId, body: &[u8]) -> Result<(RequestId, &[u8]), SourceErrorV2> {
    if body.len() < 137 || body[..5] != [2, 0, 1, 0, 4] || body[5..69] != case.0.0 {
        return Err(SourceErrorV2::Binding);
    }
    let len = u32::from_le_bytes(body[133..137].try_into().expect("4")) as u64;
    let end = 137u64.checked_add(len).ok_or(SourceErrorV2::Budget)?;
    if end != body.len() as u64 {
        return Err(SourceErrorV2::Binding);
    }
    let request = RequestId(Digest(body[69..133].try_into().expect("64")));
    Ok((request, &body[137..]))
}
