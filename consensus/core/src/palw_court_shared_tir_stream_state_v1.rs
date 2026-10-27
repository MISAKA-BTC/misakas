//! RFC-0029 candidate rooted prefixes, separate from signed duty completion.
use crate::palw_court_shared_tir_state_v1 as shared;
use crate::palw_court_shared_tir_stream_v1::{
    SharedStreamActionV1 as Action, SharedStreamTargetV1 as Target, SignedSharedStreamV1, check_shared_stream_part_v1,
};
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as PART;
use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Query, PalwTirPublicSourceV1 as Source};
use crate::{
    BlockHash, Hash64,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct SharedStreamFinishedV1 {
    pub block: BlockHash,
    pub daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct SharedTirStreamV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub target: crate::palw_court_shared_tir_stream_v1::SharedStreamTargetV1,
    pub signer: PalwBondKeyV2,
    pub slot_id: Hash64,
    pub stream_id: Hash64,
    pub total_bytes: u64,
    pub body_hash: Hash64,
    pub begun_block: BlockHash,
    pub begun_daa: u64,
    pub begin_intent: Hash64,
    pub last_block: BlockHash,
    pub last_daa: u64,
    pub accepted_bytes: u64,
    pub head: Hash64,
    pub finished: Option<SharedStreamFinishedV1>,
    pub material_until_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct SharedTirChunkV1 {
    pub version: u16,
    pub claim: Hash64,
    pub slot_id: Hash64,
    pub stream_id: Hash64,
    pub offset: u64,
    pub previous: Hash64,
    pub bytes: Vec<u8>,
    pub block: BlockHash,
    pub daa: u64,
    pub intent: Hash64,
    pub material_until_daa: u64,
}
impl BorshDeserialize for SharedTirChunkV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        Ok(Self {
            version: u16::deserialize_reader(r)?,
            claim: Hash64::deserialize_reader(r)?,
            slot_id: Hash64::deserialize_reader(r)?,
            stream_id: Hash64::deserialize_reader(r)?,
            offset: u64::deserialize_reader(r)?,
            previous: Hash64::deserialize_reader(r)?,
            bytes: crate::palw_public_binding_v1::read_bounded_bytes_v1(r, PART)?,
            block: BlockHash::deserialize_reader(r)?,
            daa: u64::deserialize_reader(r)?,
            intent: Hash64::deserialize_reader(r)?,
            material_until_daa: u64::deserialize_reader(r)?,
        })
    }
}
impl SharedTirChunkV1 {
    pub fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1
            || *key != Key::CourtSharedChunk(self.slot_id, self.offset)
            || self.bytes.is_empty()
            || self.bytes.len() > PART
        {
            return Err("shared chunk version/key/part bound".into());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<Hash64, String> {
        self.check(&Key::CourtSharedChunk(self.slot_id, self.offset))?;
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/shared-stream-chunk/v1", self)
    }
    pub(crate) fn check_binding(&self, s: &SharedTirStreamV1) -> Result<u64, String> {
        self.check(&Key::CourtSharedChunk(self.slot_id, self.offset))?;
        if (self.claim, self.slot_id, self.stream_id, self.material_until_daa)
            != (s.claim, s.slot_id, s.stream_id, s.material_until_daa)
            || self.daa < s.begun_daa
        {
            return Err("shared chunk substitutes stream/clock".into());
        }
        check_shared_stream_part_v1(s.total_bytes, self.offset, self.offset, self.bytes.len())
    }
    pub fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        let s = stream(state, self.slot_id)?;
        let end = self.check_binding(s)?;
        if end > s.accepted_bytes || self.daa > s.last_daa || self.offset % PART as u64 != 0 {
            return Err("shared chunk is outside retained prefix".into());
        }
        Ok(())
    }
}
pub(crate) fn stream(state: &PalwChainStateV2, slot: Hash64) -> Result<&SharedTirStreamV1, String> {
    match state.staged_court_row_v1(&Key::CourtSharedStream(slot)) {
        Some(Row::CourtSharedStream(s)) => Ok(s),
        _ => Err("shared stream missing".into()),
    }
}
impl SharedTirStreamV1 {
    pub fn check(&self, key: &Key) -> Result<(), String> {
        let c = self.declaration();
        if self.version != 1
            || *key != Key::CourtSharedStream(self.slot_id)
            || c.slot_id()? != self.slot_id
            || c.declared_stream_id(self.total_bytes, self.body_hash)? != self.stream_id
            || self.accepted_bytes > self.total_bytes
            || self.last_daa < self.begun_daa
            || self.accepted_bytes == 0 && self.head != Hash64::default()
            || self.accepted_bytes != self.total_bytes && self.accepted_bytes % PART as u64 != 0
            || self.finished.as_ref().is_some_and(|f| self.accepted_bytes != self.total_bytes || f.daa < self.last_daa)
        {
            return Err("shared stream version/key/declaration/cursor/clock".into());
        }
        Ok(())
    }
    fn declaration(&self) -> SignedSharedStreamV1 {
        SignedSharedStreamV1 {
            version: self.version,
            claim: self.claim,
            scope: self.scope_id,
            target: self.target.clone(),
            signer: self.signer,
            action: Action::Begin { total_bytes: self.total_bytes, body_hash: self.body_hash },
            signature: Vec::new(),
        }
    }
    pub fn same_opening(&self, old: &Self) -> bool {
        self.version == old.version
            && self.claim == old.claim
            && self.scope_id == old.scope_id
            && self.target == old.target
            && self.signer == old.signer
            && self.slot_id == old.slot_id
            && self.stream_id == old.stream_id
            && self.total_bytes == old.total_bytes
            && self.body_hash == old.body_hash
            && self.begun_block == old.begun_block
            && self.begun_daa == old.begun_daa
            && self.begin_intent == old.begin_intent
            && self.material_until_daa == old.material_until_daa
    }
    /// Historical Ready fact stays rooted; current bond activity is checked at
    /// ingress, not used to invalidate an already accepted public prefix later.
    pub(crate) fn source(&self, state: &PalwChainStateV2) -> Result<Source, String> {
        self.check(&Key::CourtSharedStream(self.slot_id))?;
        let c = shared::context(state, self.claim)?;
        let source = c.source(state)?;
        let (maximum, _) = target_bound(state, c, &source, &self.target)?;
        if self.scope_id != c.scope_id
            || self.total_bytes > maximum
            || self.material_until_daa != c.material_until_daa
            || self.begun_daa < c.original_accepted_daa
            || self.last_daa >= c.hard_daa
            || self.finished.as_ref().is_some_and(|f| f.daa >= c.hard_daa)
        {
            return Err("shared stream substitutes original scope/bound/window".into());
        }
        if let Target::Unit { request_id, .. } = &self.target {
            let Some(Row::CourtSharedQuery(q)) = state.staged_court_row_v1(&Key::CourtSharedQuery(*request_id)) else {
                return Err("shared query missing".into());
            };
            if self.begun_daa < q.opened_daa {
                return Err("shared stream precedes accepted query".into());
            }
        }
        ready_at(state, c, &self.signer, self.begun_daa)?;
        Ok(source)
    }
    /// Checks just the newly linked chunk; no growing-prefix copy on Part.
    pub(crate) fn check_advance(&self, state: &PalwChainStateV2, old: Option<&Self>) -> Result<(), String> {
        self.source(state)?;
        match old {
            None => {
                if self.accepted_bytes != 0
                    || self.finished.is_some()
                    || self.last_daa != self.begun_daa
                    || self.last_block != self.begun_block
                {
                    return Err("shared stream must start empty".into());
                }
            }
            Some(old) => {
                if !self.same_opening(old) || old.finished.is_some() && self != old || self.last_daa < old.last_daa {
                    return Err("shared stream cannot replace/restart/redate opening".into());
                }
                if self.accepted_bytes != old.accepted_bytes {
                    if old.finished.is_some() || self.finished.is_some() {
                        return Err("shared part after EOF".into());
                    }
                    let Some(Row::CourtSharedChunk(chunk)) =
                        state.staged_court_row_v1(&Key::CourtSharedChunk(self.slot_id, old.accepted_bytes))
                    else {
                        return Err("shared advance lost new chunk".into());
                    };
                    if chunk.check_binding(self)? != self.accepted_bytes
                        || chunk.previous != old.head
                        || chunk.digest()? != self.head
                        || (chunk.block, chunk.daa) != (self.last_block, self.last_daa)
                    {
                        return Err("shared advance does not match accepted chunk".into());
                    }
                } else if self.head != old.head || self.last_daa != old.last_daa || self.last_block != old.last_block {
                    return Err("shared unchanged prefix cannot be redated".into());
                }
            }
        }
        if self.finished.is_some() {
            self.check_context(state)?;
        }
        Ok(())
    }
    pub(crate) fn body(&self, state: &PalwChainStateV2) -> Result<crate::palw_native_resource_v1::NativeRetainedV1<Vec<u8>>, String> {
        self.check(&Key::CourtSharedStream(self.slot_id))?;
        let n = usize::try_from(self.accepted_bytes).map_err(|_| "shared prefix exceeds address space")?;
        let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
        q.exact_v1::<u8>(self.accepted_bytes).map_err(|e| e.to_string())?;
        let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(n).map_err(|_| "shared prefix allocation refused")?;
        let mut at = 0;
        let mut head = Hash64::default();
        let mut daa = self.begun_daa;
        let mut block = self.begun_block;
        while at < self.accepted_bytes {
            let Some(Row::CourtSharedChunk(chunk)) = state.staged_court_row_v1(&Key::CourtSharedChunk(self.slot_id, at)) else {
                return Err("shared prefix chunk missing".into());
            };
            let end = chunk.check_binding(self)?;
            if chunk.previous != head || chunk.daa < daa || end > self.accepted_bytes {
                return Err("shared prefix chain/clock mismatch".into());
            }
            head = chunk.digest()?;
            daa = chunk.daa;
            block = chunk.block;
            at = end;
            bytes.extend_from_slice(&chunk.bytes);
        }
        if head != self.head || daa != self.last_daa || block != self.last_block {
            return Err("shared prefix head/context mismatch".into());
        }
        Ok(crate::palw_native_resource_v1::NativeRetainedV1::from_owner_reserved_v1(bytes, lease))
    }
    pub fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        let source = self.source(state)?;
        let bytes = self.body(state)?;
        if self.finished.is_some() {
            self.authenticate_eof(state, &source, &bytes)?;
            match &self.target {
                Target::Unit { ordinal, .. } => {
                    if shared::read_shared_data_v1(state, self.claim, *ordinal)?.is_none() {
                        return Err("shared finished stream lost public data".into());
                    }
                }
                Target::FullIds => {
                    if !matches!(state.staged_court_row_v1(&Key::CourtSharedIds(self.claim)), Some(Row::CourtSharedIds(_))) {
                        return Err("shared finished IDs lost sealed data".into());
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn authenticate_eof(&self, state: &PalwChainStateV2, source: &Source, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() as u64 != self.total_bytes
            || crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(bytes) != self.body_hash
        {
            return Err("shared EOF length/body checksum mismatch".into());
        }
        let c = shared::context(state, self.claim)?;
        match &self.target {
            Target::Unit { ordinal, .. } => {
                let frame =
                    crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, self.claim).ok_or("shared frame missing")?;
                let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, source, c.profile_id)?;
                shared::authenticate_body(state, c, source, set.request_at(*ordinal)?, bytes)
            }
            Target::FullIds => {
                decode_ids_body(source, bytes)?;
                Ok(())
            }
        }
    }
}
pub(crate) fn ready_at(
    state: &PalwChainStateV2,
    c: &shared::SharedTirContextV1,
    signer: &PalwBondKeyV2,
    at: u64,
) -> Result<(), String> {
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, c.claim).ok_or("shared original frame missing")?;
    let Some(Row::TirPreparation(row)) = state.staged_court_row_v1(&frame.preparation_key_v1()) else {
        return Err("shared Ready preparation missing".into());
    };
    crate::palw_tir_public_state_v1::palw_tir_public_source_for_preparation_row_v1(state, row, frame.descriptor_v1().maximum_leaves)?;
    let ready = row.preparation.ready_v1(signer).ok_or("shared stream signer was not original Ready custodian")?;
    if ready.accepted_daa > at {
        return Err("shared stream predates signer Ready".into());
    }
    Ok(())
}
pub(crate) fn target_bound(
    state: &PalwChainStateV2,
    c: &shared::SharedTirContextV1,
    source: &Source,
    target: &Target,
) -> Result<(u64, u64), String> {
    let query = match target {
        Target::Unit { ordinal, request_id } => {
            let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, c.claim).ok_or("shared frame missing")?;
            let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, source, c.profile_id)?;
            if set.request_id(*ordinal)? != *request_id {
                return Err("shared stream query ID substitution".into());
            }
            let q = set.request_at(*ordinal)?;
            if matches!(q, Query::Ids { .. }) {
                return Err("shared stream IDs require one FullIds target".into());
            }
            let Some(Row::CourtSharedQuery(row)) = state.staged_court_row_v1(&Key::CourtSharedQuery(*request_id)) else {
                return Err("shared stream requires native query".into());
            };
            row.check_context(state)?;
            q
        }
        Target::FullIds => Query::Ids { offset: 0 },
    };
    crate::palw_tir_public_owned_v1::request_body_bound_v1(source, query)
}
/// Exact typed owners for the provisional sequence and per-chunk decode work.
/// This adds no authenticated flag: the original outer root is checked at EOF.
pub(crate) fn decode_ids_body(
    source: &Source,
    body: &[u8],
) -> Result<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedIdsV1>, String>
{
    let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.exact_v1::<u32>(u64::from(source.decode_count_v1())).map_err(|e| e.to_string())?;
    let _ids = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    ids.try_reserve_exact(source.decode_count_v1() as usize).map_err(|_| "shared IDs allocation refused")?;
    let mut at = 0usize;
    let mut rows = None;
    for offset in (0..source.decode_count_v1()).step_by(crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize) {
        let request = Query::Ids { offset };
        let bound = source.unit_bound_v1(&request)?;
        let _decode =
            crate::palw_native_resource_v1::reserve_native_requests_v1(bound.resident_ram_bytes).map_err(|e| e.to_string())?;
        let len = usize::try_from(bound.maximum_response_bytes).map_err(|_| "shared ID chunk address space")?;
        let end = at.checked_add(len).filter(|end| *end <= body.len()).ok_or("shared ID body truncated")?;
        let crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::Ids { rows_root, ids: chunk } =
            source.decode_response_v1(&request, &body[at..end])?
        else {
            return Err("shared ID body type".into());
        };
        if chunk.len()
            != (source.decode_count_v1() - offset).min(crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1) as usize
            || rows.is_some_and(|r| r != rows_root)
        {
            return Err("shared ID canonical count/root mismatch".into());
        }
        rows = Some(rows_root);
        ids.extend_from_slice(&chunk);
        at = end;
    }
    if at != body.len() {
        return Err("shared ID body trailing bytes".into());
    }
    source.retain_complete_ids_v1(rows.ok_or("shared empty ID stream")?, &ids)
}
