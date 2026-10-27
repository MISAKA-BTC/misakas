//! RFC-0029 rooted data candidate. No production installer, lifecycle tag,
//! complete reservation certificate, signer-duty completion or default right.
use crate::palw_court_tir_requests_v1::TirSharedRequestSetV1;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2};
use crate::{BlockHash, Hash64};
use borsh::{BorshDeserialize, BorshSerialize};
pub fn shared_native_profile_id_v1(service_profile: Hash64) -> Hash64 {
    Hash64::from_bytes(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"misaka-palw/shared-native-profile/v1")
            .hash(service_profile.as_byte_slice())
            .as_bytes()
            .try_into()
            .expect("64"),
    )
}
/// Context is NOT a paid-admission or timely-notice certificate. Only an
/// eventual complete original coordinator may install this on a real network.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SharedTirContextV1 {
    pub version: u16,
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub rules_id: Hash64,
    pub profile_id: Hash64,
    pub scope_id: Hash64,
    pub queries: u64,
    pub original_accepted_daa: u64,
    pub hard_daa: u64,
    pub material_until_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SharedTirBodyV1 {
    pub block: BlockHash,
    pub accepted_daa: u64,
    pub digest: Hash64,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedTirQueryV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub ordinal: u64,
    pub request_id: Hash64,
    pub opened_block: BlockHash,
    pub opened_daa: u64,
    pub opened_by: PalwBondKeyV2,
    pub intent_digest: Hash64,
    pub material_until_daa: u64,
    pub body: Option<SharedTirBodyV1>,
    /// F03b2: version 2 rows record their deterministic service slot; version 1 rows carry `None`.
    pub service: Option<SharedServiceSlotV1>,
}
/// **F03b2 (PROVISIONAL estimate): the reserved service slot of one shared request.** `packets = ⌈max response bytes / 34,000⌉`
/// of its coordinate; served in `[start_daa, end_daa]` with `PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1` packets per DAA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SharedServiceSlotV1 {
    pub packets: u64,
    pub start_daa: u64,
    pub end_daa: u64,
}
pub const SHARED_QUERY_SERVICE_VERSION_V2: u16 = 2;
// Version 1 rows keep their exact bytes; version 2 appends the bare slot (no Option tag).
impl BorshSerialize for SharedTirQueryV1 {
    fn serialize<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        (
            self.version,
            self.claim,
            self.scope_id,
            self.ordinal,
            self.request_id,
            self.opened_block,
            self.opened_daa,
            self.opened_by,
            self.intent_digest,
            self.material_until_daa,
        )
            .serialize(w)?;
        self.body.serialize(w)?;
        match (self.version, &self.service) {
            (1, None) => Ok(()),
            (SHARED_QUERY_SERVICE_VERSION_V2, Some(s)) => s.serialize(w),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "shared query version/service")),
        }
    }
}
impl BorshDeserialize for SharedTirQueryV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        let version = u16::deserialize_reader(r)?;
        if version != 1 && version != SHARED_QUERY_SERVICE_VERSION_V2 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "shared query version"));
        }
        let claim = Hash64::deserialize_reader(r)?;
        let scope_id = Hash64::deserialize_reader(r)?;
        let ordinal = u64::deserialize_reader(r)?;
        let request_id = Hash64::deserialize_reader(r)?;
        let opened_block = BlockHash::deserialize_reader(r)?;
        let opened_daa = u64::deserialize_reader(r)?;
        let opened_by = PalwBondKeyV2::deserialize_reader(r)?;
        let intent_digest = Hash64::deserialize_reader(r)?;
        let material_until_daa = u64::deserialize_reader(r)?;
        let body = Option::<SharedTirBodyV1>::deserialize_reader(r)?;
        let service =
            if version == SHARED_QUERY_SERVICE_VERSION_V2 { Some(SharedServiceSlotV1::deserialize_reader(r)?) } else { None };
        Ok(Self {
            version,
            claim,
            scope_id,
            ordinal,
            request_id,
            opened_block,
            opened_daa,
            opened_by,
            intent_digest,
            material_until_daa,
            body,
            service,
        })
    }
}
/// Packets of one coordinate: `⌈maximum_response_bytes / PALW_G14_PACKET_BYTES_V1⌉`, at least 1. Size never refuses; it is charged.
pub fn shared_coordinate_packets_v1(
    source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    query: &crate::palw_tir_public_source_v1::PalwTirPublicRequestV1,
) -> Result<u64, String> {
    let bytes = source.unit_bound_v1(query)?.maximum_response_bytes;
    Ok(bytes.div_ceil(crate::palw_g14_values_v1::PALW_G14_PACKET_BYTES_V1).max(1))
}
/// `Q` in packets: the sum over the whole closed universe (every branch's every coordinate at its maximum size).
pub fn shared_universe_packets_v1(
    source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    set: &TirSharedRequestSetV1,
) -> Result<u64, String> {
    let mut total = 0u64;
    for i in 0..set.len() {
        total =
            total.checked_add(shared_coordinate_packets_v1(source, &set.request_at(i)?)?).ok_or("shared universe packets overflow")?;
    }
    Ok(total)
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SharedTirIdsV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub rows_root: Hash64,
    pub ids: Vec<u32>,
    pub block: BlockHash,
    pub accepted_daa: u64,
    pub material_until_daa: u64,
}
pub fn shared_body_digest_v1(bytes: &[u8]) -> Hash64 {
    Hash64::from_bytes(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"misaka-palw/shared-source-body/v1")
            .hash(bytes)
            .as_bytes()
            .try_into()
            .expect("64"),
    )
}
impl SharedTirContextV1 {
    pub fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1
            || *key != Key::CourtSharedContext(self.claim)
            || self.queries == 0
            || self.original_accepted_daa.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA) != Some(self.hard_daa)
        {
            return Err("invalid shared context version/key/clock".into());
        }
        Ok(())
    }
    pub(crate) fn check_binding(&self, service: &crate::palw_court_service_state_v2::CourtServiceStateV2) -> Result<(), String> {
        let b = service.claims.get(&self.claim).ok_or("shared context lost original service binding")?;
        if self.profile_id != shared_native_profile_id_v1(service.profile_id)
            || (self.allocation_id, self.source_id, self.original_accepted_daa, self.hard_daa)
                != (b.allocation_id, b.source_id, b.original_accepted_daa, b.hard_deadline_daa)
        {
            return Err("shared context substitutes service namespace/original scope".into());
        }
        Ok(())
    }
    pub fn source(&self, state: &PalwChainStateV2) -> Result<crate::palw_tir_public_source_v1::PalwTirPublicSourceV1, String> {
        self.check(&Key::CourtSharedContext(self.claim))?;
        self.check_binding(state.court_service_state_v1().ok_or("shared context has no service book")?)?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, self.claim)
            .ok_or("shared context lost original frame")?;
        let p = frame.plan_v1();
        if (p.allocation_id, p.source_id, p.rules_id, p.terms.accepted_daa, p.terms.material_until_daa)
            != (self.allocation_id, self.source_id, self.rules_id, self.original_accepted_daa, self.material_until_daa)
        {
            return Err("shared context substitutes retained original".into());
        }
        let record = state.tir_class_v1(&frame.original_v1().job.class_id).ok_or("shared original class is absent")?;
        let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, record.facts.class_id)
            .ok_or("shared original registered layout is absent")?;
        let source = frame.source_v1(record, layout, frame.descriptor_v1().maximum_leaves)?;
        let set = TirSharedRequestSetV1::new(frame, &source, self.profile_id)?;
        if self.scope_id != set.scope_id()? || self.queries != set.len() {
            return Err("shared context substitutes closed universe".into());
        }
        Ok(source)
    }
    /// Read-only original-bound parameter identity check. The caller admits source
    /// reconstruction/CPU/IO separately; known program/index owners are reserved here.
    /// No insertion, delivery/custody, timely notice, payer or eligibility follows.
    pub(crate) fn check_parameter_opening(
        &self,
        state: &PalwChainStateV2,
        scope: Hash64,
        opening: &crate::palw_artifact::PalwArtifactOpeningV1,
    ) -> Result<(), String> {
        self.with_parameter_verifier(state, scope, |v| v.check(opening).map_err(|e| e.to_string()))
    }
    /// The input is one complete existing artifact opening, never an incomplete-prefix slot.
    pub(crate) fn decode_parameter_opening(
        &self,
        state: &PalwChainStateV2,
        scope: Hash64,
        leaf: u32,
        bytes: &[u8],
    ) -> Result<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_artifact::PalwArtifactOpeningV1>, String> {
        self.with_parameter_verifier(state, scope, |v| v.decode(leaf, bytes))
    }
    pub(crate) fn with_parameter_verifier<T>(
        &self,
        state: &PalwChainStateV2,
        scope: Hash64,
        check: impl FnOnce(&crate::palw_tir_court_v1::TirInventoryOpeningVerifierV1<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        if scope != self.scope_id {
            return Err("shared parameter scope substitution".into());
        }
        // Require this exact retained row, not a caller-constructed context with valid fields.
        if context(state, self.claim)? != self {
            return Err("shared parameter context is not retained".into());
        }
        let source = self.source(state)?;
        let record = state
            .tir_class_v1(&source.header_v1().job_context.shape_profile_id)
            .ok_or("shared parameter registered program absent")?;
        let program =
            crate::palw_native_resource_v1::decode_native_tir_program_v1(record.program.as_slice()).map_err(|e| e.to_string())?;
        let requests = crate::palw_tir_court_v1::TirInventoryOpeningVerifierV1::index_requests(&program).map_err(|e| e.to_string())?;
        let _index = crate::palw_native_resource_v1::reserve_native_requests_v1(requests).map_err(|e| e.to_string())?;
        let verifier = crate::palw_tir_court_v1::TirInventoryOpeningVerifierV1::new(&program, source.header_v1().artifact_root)
            .ok_or("shared parameter inventory is not derivable")?;
        check(&verifier)
    }
    pub fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.source(state).map(|_| ())
    }
}
/// **ADR-0192 A/A (lane F03b): the claim-common localisation frontier, derived from the original alone.** Every field is a
/// function of the retained original frame, its registered class/layout and the claim's court service binding (installed at the
/// same accepting point by F02's admission or the RFC-0035 funded all-branch book). Nothing depends on an owner, filer, requester
/// or relay, so a coalition holding every owner slot cannot change, consume or pre-empt it: the closed request universe
/// (`queries`: every legal coordinate of every branch the source admits) is fixed at acceptance. Data identity only.
pub fn derive_shared_tir_context_v1(state: &PalwChainStateV2, claim: Hash64) -> Result<SharedTirContextV1, String> {
    let service = state.court_service_state_v1().ok_or("shared frontier: no court service book")?;
    let binding = service.claims.get(&claim).ok_or("shared frontier: original has no service binding")?;
    let frame =
        crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim).ok_or("shared frontier: original frame absent")?;
    let p = frame.plan_v1();
    if p.original_claim != claim
        || (p.allocation_id, p.source_id, p.terms.accepted_daa)
            != (binding.allocation_id, binding.source_id, binding.original_accepted_daa)
    {
        return Err("shared frontier: service binding names another original".into());
    }
    let record = state.tir_class_v1(&frame.original_v1().job.class_id).ok_or("shared frontier: original class absent")?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, record.facts.class_id)
        .ok_or("shared frontier: registered layout absent")?;
    let source = frame.source_v1(record, layout, frame.descriptor_v1().maximum_leaves)?;
    let profile_id = shared_native_profile_id_v1(service.profile_id);
    let set = TirSharedRequestSetV1::new(frame, &source, profile_id)?;
    let c = SharedTirContextV1 {
        version: 1,
        claim,
        allocation_id: p.allocation_id,
        source_id: p.source_id,
        rules_id: p.rules_id,
        profile_id,
        scope_id: set.scope_id()?,
        queries: set.len(),
        original_accepted_daa: p.terms.accepted_daa,
        hard_daa: binding.hard_deadline_daa,
        material_until_daa: p.terms.material_until_daa,
    };
    c.check(&Key::CourtSharedContext(claim))?;
    Ok(c)
}
/// F03b2: `Q` in packets for a (derived or installed) context — every coordinate of every branch at its maximum size.
pub fn shared_frontier_universe_packets_v1(state: &PalwChainStateV2, c: &SharedTirContextV1) -> Result<u64, String> {
    let source = c.source(state)?;
    let frame =
        crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, c.claim).ok_or("shared frontier: original frame absent")?;
    shared_universe_packets_v1(&source, &TirSharedRequestSetV1::new(frame, &source, c.profile_id)?)
}
pub(crate) fn context(state: &PalwChainStateV2, claim: Hash64) -> Result<&SharedTirContextV1, String> {
    match state.staged_court_row_v1(&Key::CourtSharedContext(claim)) {
        Some(Row::CourtSharedContext(c)) => Ok(c),
        _ => Err("no installed native shared context".into()),
    }
}
impl SharedTirQueryV1 {
    pub fn check(&self, key: &Key) -> Result<(), String> {
        let service_ok = match (self.version, &self.service) {
            (1, None) => true,
            (SHARED_QUERY_SERVICE_VERSION_V2, Some(s)) => s.packets > 0 && s.start_daa >= self.opened_daa && s.end_daa >= s.start_daa,
            _ => false,
        };
        if !service_ok
            || *key != Key::CourtSharedQuery(self.request_id)
            || self
                .body
                .as_ref()
                .is_some_and(|b| b.bytes.is_empty() || b.accepted_daa < self.opened_daa || b.digest != shared_body_digest_v1(&b.bytes))
        {
            return Err("invalid shared query shape/key/body".into());
        }
        Ok(())
    }
    pub(crate) fn same_opening(&self, old: &Self) -> bool {
        (
            self.version,
            self.claim,
            self.scope_id,
            self.ordinal,
            self.request_id,
            self.opened_block,
            self.opened_daa,
            self.opened_by,
            self.intent_digest,
            self.material_until_daa,
            self.service,
        ) == (
            old.version,
            old.claim,
            old.scope_id,
            old.ordinal,
            old.request_id,
            old.opened_block,
            old.opened_daa,
            old.opened_by,
            old.intent_digest,
            old.material_until_daa,
            old.service,
        )
    }
    pub fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.check(&Key::CourtSharedQuery(self.request_id))?;
        let c = context(state, self.claim)?;
        let source = c.source(state)?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, self.claim).ok_or("shared query lost frame")?;
        let set = TirSharedRequestSetV1::new(frame, &source, c.profile_id)?;
        if (self.scope_id, self.material_until_daa) != (c.scope_id, c.material_until_daa)
            || self.opened_daa < c.original_accepted_daa
            || self.opened_daa >= c.hard_daa
            || self.request_id != set.request_id(self.ordinal)?
        {
            return Err("shared query substitutes original coordinate/clock".into());
        }
        if let Some(s) = &self.service {
            if s.end_daa >= c.hard_daa || s.packets != shared_coordinate_packets_v1(&source, &set.request_at(self.ordinal)?)? {
                return Err("shared query service slot exceeds hard or misstates its packets".into());
            }
        }
        if let Some(body) = &self.body {
            if body.accepted_daa >= c.hard_daa {
                return Err("shared data exceeds original hard clock".into());
            }
            authenticate_body(state, c, &source, set.request_at(self.ordinal)?, &body.bytes)?;
        }
        // Historical requester identity was checked at acceptance. A requester
        // exiting later must not invalidate public data or transfer it to an owner.
        Ok(())
    }
}
impl SharedTirIdsV1 {
    pub fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1 || *key != Key::CourtSharedIds(self.claim) {
            return Err("invalid shared ID row version/key".into());
        }
        Ok(())
    }
    /// Typed counterpart for the actual shared-IDs dispatcher. The compatibility
    /// String validator remains available to existing reload/build consumers.
    pub(crate) fn check_context_classified_v2(
        &self,
        state: &PalwChainStateV2,
    ) -> Result<(), crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        use crate::palw_native_resource_v1::NativeDecodeErrorV1 as Fault;
        self.check(&Key::CourtSharedIds(self.claim)).map_err(Fault::Decode)?;
        let c = context(state, self.claim).map_err(Fault::Decode)?;
        let source = c.source(state).map_err(Fault::Decode)?;
        if self.scope_id != c.scope_id
            || self.material_until_daa != c.material_until_daa
            || self.accepted_daa < c.original_accepted_daa
            || self.accepted_daa >= c.hard_daa
        {
            return Err(Fault::Decode("shared ID row substitutes context/clock".into()));
        }
        source.validate_complete_ids_classified_v2(self.rows_root, &self.ids)
    }
    pub fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.check(&Key::CourtSharedIds(self.claim))?;
        let c = context(state, self.claim)?;
        let source = c.source(state)?;
        if self.scope_id != c.scope_id
            || self.material_until_daa != c.material_until_daa
            || self.accepted_daa < c.original_accepted_daa
            || self.accepted_daa >= c.hard_daa
        {
            return Err("shared ID row substitutes context/clock".into());
        }
        source.validate_complete_ids_v1(self.rows_root, &self.ids)
    }
}
/// Step roots do not depend on decoded IDs. Trace roots still require a fresh
/// original outer-root check, but an owned copy of the ID sequence is unneeded.
fn rows_anchor_for_query(
    state: &PalwChainStateV2,
    c: &SharedTirContextV1,
    source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    query: crate::palw_tir_public_source_v1::PalwTirPublicRequestV1,
) -> Result<Option<crate::palw_tir_public_source_v1::PalwTirPublicRowsAnchorV1>, String> {
    use crate::palw_tir_public_source_v1::PalwTirPublicRequestV1 as Q;
    match query {
        Q::StepNode { .. } | Q::StepLeaf { .. } | Q::StepRun { .. } | Q::Ids { .. } => Ok(None),
        Q::RowsNode { .. } | Q::TileNode { .. } | Q::TraceTile { .. } => {
            match state.staged_court_row_v1(&Key::CourtSharedIds(c.claim)) {
                Some(Row::CourtSharedIds(row)) => {
                    row.check(&Key::CourtSharedIds(c.claim))?;
                    if row.scope_id != c.scope_id {
                        return Err("shared body substitutes ID context".into());
                    }
                    source.rows_anchor_from_complete_ids_v1(row.rows_root, &row.ids).map(Some)
                }
                _ => Ok(None),
            }
        }
    }
}
pub(crate) fn authenticate_body(
    state: &PalwChainStateV2,
    c: &SharedTirContextV1,
    source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    query: crate::palw_tir_public_source_v1::PalwTirPublicRequestV1,
    bytes: &[u8],
) -> Result<(), String> {
    let bound = source.unit_bound_v1(&query)?;
    let _scratch = crate::palw_native_resource_v1::reserve_native_requests_v1(bound.resident_ram_bytes).map_err(|e| e.to_string())?;
    let response = source.decode_response_v1(&query, bytes)?;
    let rows = rows_anchor_for_query(state, c, source, query)?;
    source.authenticate_with_rows_v1(&query, &response, None, rows.as_ref())?;
    Ok(())
}

/// A runtime data capability/view, never a serializable answer or admission flag.
pub enum RootedSharedDataV1<'a> {
    Unit(crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedUnitV1>),
    Ids { rows_root: Hash64, ids: &'a [u32] },
}
pub fn read_shared_data_v1(state: &PalwChainStateV2, claim: Hash64, ordinal: u64) -> Result<Option<RootedSharedDataV1<'_>>, String> {
    let c = context(state, claim)?;
    let source = c.source(state)?;
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim).ok_or("shared read lost original")?;
    let set = TirSharedRequestSetV1::new(frame, &source, c.profile_id)?;
    let query = set.request_at(ordinal)?;
    let key = Key::CourtSharedQuery(set.request_id(ordinal)?);
    let Some(Row::CourtSharedQuery(row)) = state.staged_court_row_v1(&key) else {
        return Ok(None);
    };
    row.check_context(state)?;
    if let crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::Ids { offset } = query {
        if let Some(Row::CourtSharedIds(ids)) = state.staged_court_row_v1(&Key::CourtSharedIds(claim)) {
            ids.check_context(state)?;
            let start = offset as usize;
            let count = ids
                .ids
                .len()
                .checked_sub(start)
                .ok_or("shared read ID offset")?
                .min(crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize);
            let end = start.checked_add(count).ok_or("shared read ID end overflow")?;
            return Ok(Some(RootedSharedDataV1::Ids { rows_root: ids.rows_root, ids: &ids.ids[start..end] }));
        }
        return Ok(None);
    }
    let Some(body) = &row.body else {
        return Ok(None);
    };
    let bound = source.unit_bound_v1(&query)?;
    let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(bound.resident_ram_bytes).map_err(|e| e.to_string())?;
    let response = source.decode_response_v1(&query, &body.bytes)?;
    let rows = rows_anchor_for_query(state, c, &source, query)?;
    let unit = source.authenticate_with_rows_v1(&query, &response, None, rows.as_ref())?;
    Ok(Some(RootedSharedDataV1::Unit(crate::palw_native_resource_v1::NativeRetainedV1::from_owner_reserved_v1(unit, lease))))
}
