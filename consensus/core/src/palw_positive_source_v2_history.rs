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
    origins: BTreeMap<Vec<u8>, usize>,
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
    pub(super) fn prepare(
        &self,
        material: AcceptedMaterial,
        kind: SourceHistoryKindV2,
        body: &[u8],
        limits: SourceLimitsV2,
    ) -> Result<Option<SourceHistoryEntryV2>, SourceErrorV2> {
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
            return Ok(None);
        }
        if body.is_empty()
            || body.len() as u64 > limits.max_native_body_bytes
            || self.entries.len() as u64 >= limits.max_history_events
            || self.bytes.checked_add(body.len() as u64).filter(|n| *n <= limits.max_history_bytes).is_none()
        {
            return Err(SourceErrorV2::Budget);
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
fn origin_key(material: &AcceptedMaterial) -> Result<Vec<u8>, SourceErrorV2> {
    borsh::to_vec(&material.origin).map_err(|_| SourceErrorV2::History)
}

/// Checks A4's existing op3 literal, without assigning a new tag or domain.
pub(super) fn check_request_literal(event: &SourceRequestEventV2<'_>) -> Result<(), SourceErrorV2> {
    let mut expected = Vec::with_capacity(4909);
    expected.extend_from_slice(&2u16.to_le_bytes());
    expected.extend_from_slice(&1u16.to_le_bytes());
    expected.push(3);
    event.request.serialize(&mut expected).map_err(|_| SourceErrorV2::History)?;
    event.requester.serialize(&mut expected).map_err(|_| SourceErrorV2::History)?;
    expected.extend_from_slice(event.signature);
    if event.native_body != expected {
        return Err(SourceErrorV2::Binding);
    }
    Ok(())
}
/// op4 carries only case + accepted RequestId + raw existing-G14 Vec body.
/// Provider identity must come from actual carrier/delegation authority.
pub(super) fn response_data<'a>(case: CaseId, event: &SourceResponseEventV2<'a>) -> Result<&'a [u8], SourceErrorV2> {
    let body = event.native_body;
    if body.len() < 137 || body[..5] != [2, 0, 1, 0, 4] || body[5..69] != case.0.0 || body[69..133] != event.request.0.0 {
        return Err(SourceErrorV2::Binding);
    }
    let len = u32::from_le_bytes(body[133..137].try_into().expect("4")) as u64;
    let end = 137u64.checked_add(len).ok_or(SourceErrorV2::Budget)?;
    if end != body.len() as u64 || event.g14_data_range != (137, end) {
        return Err(SourceErrorV2::Binding);
    }
    Ok(&body[137..])
}
