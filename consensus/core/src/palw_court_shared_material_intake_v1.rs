//! RFC-0032 A / ADR-0199: non-exclusive terminal material ingress (lane
//! material-intake, 2026-10-22). Rooted, re-authenticated material for the
//! existing shared-court builders: parameter openings, prompt (Flat list or
//! Merkle tiles), source units and the complete generated-ID sequence.
//!
//! * ANY eligible party (an Active bond at/above the existing floor) submits a
//!   body in multi-block parts. A draft is keyed by (claim, draft) where the
//!   draft ID binds the literal submitter, so partial bodies of different
//!   submitters coexist. No draft is a slot for the target: the FIRST complete
//!   authenticated body becomes the canonical row; every other draft for the
//!   same target is then refused further parts, released at Finish or swept at
//!   its expiry.
//! * Byte budgets are derived from the retained original's native requirements
//!   (request-set unit bounds, the input cover's parameter union at its exact
//!   registered opening width, the prompt's native form and the complete-ID
//!   body). A declared size is never authority: it is checked against the
//!   per-target native bound and against the submitter's per-claim budget.
//! * Funding of storage belongs to the unadopted fee schedule. Production
//!   resolution is always typed-missing ([`MaterialIntakeMissingV1::Schedule`]),
//!   so no intake folds on any network. Tests inject a schedule.
//!
//! Authentication is data identity only (existing Merkle commitments/openings),
//! never a computation proof, a duty completion, a custody grant or a credit.
use crate::Hash64;
use crate::BlockHash;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2};
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_court_shared_tir_state_v1 as shared;
use crate::palw_tir_public_source_v1::{PalwTirPublicSourceV1 as Source, PalwTirPublicRequestV1 as Query};
use crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as PART;
use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1 as PromptForm, PalwPromptIdsOpeningV1};
use borsh::{BorshDeserialize, BorshSerialize};

pub const MATERIAL_INTAKE_CONTEXT_V1: &[u8] = b"misaka-palw/material-intake/v1/mldsa87";

/// One logical material coordinate of one original. Semantic tables (EOG /
/// rendering) are NOT a target: their per-claim byte requirement is not
/// derivable from the original today (see lane notes); they stay borrowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum MaterialTargetV1 {
    Parameter { leaf: u32 },
    PromptFlat,
    PromptTile { tile: u32 },
    Unit { ordinal: u64 },
    FullIds,
}
impl MaterialTargetV1 {
    pub fn id_v1(&self, claim: Hash64) -> Result<Hash64, String> {
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/material-target/v1", &(claim, self))
    }
}
pub fn material_draft_id_v1(claim: Hash64, target: MaterialTargetV1, submitter: PalwBondKeyV2, total_bytes: u64, body_hash: Hash64)
    -> Result<Hash64, String> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/material-draft/v1",
        &(claim, target, submitter, total_bytes, body_hash))
}

// ---------------------------------------------------------------------------
// The unadopted storage fee schedule (typed-missing in production).
// ---------------------------------------------------------------------------

/// Values the intake needs that are NOT adopted: storage price, draft TTL and
/// the explicit exploration budget of the original's input cover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialIntakeScheduleV1 {
    pub sompi_per_byte: u64,
    pub draft_ttl_daa: u64,
    pub cover: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialIntakeMissingV1 {
    /// The storage fee schedule (price, payer, TTL) is not adopted.
    Schedule,
}
#[cfg(test)]
thread_local! {
    static TEST_SCHEDULE: std::cell::Cell<Option<MaterialIntakeScheduleV1>> = const { std::cell::Cell::new(None) };
}
/// TEST ONLY: per-thread injection; no non-test build can reach a value.
#[cfg(test)]
pub(crate) fn set_material_intake_test_schedule_v1(schedule: Option<MaterialIntakeScheduleV1>) {
    TEST_SCHEDULE.with(|c| c.set(schedule));
}
/// Production: **V5 — USER-APPROVED 2026-10-26 (estimate used as is; 「目安の値をそのまま使う」承認). Review condition: replace by measurement.** Basis: lane VAL [`crate::palw_g14_values_v1::PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1`].
/// A test may still inject its own schedule per thread.
pub fn material_intake_schedule_v1() -> Result<MaterialIntakeScheduleV1, MaterialIntakeMissingV1> {
    #[cfg(test)]
    if let Some(schedule) = TEST_SCHEDULE.with(|c| c.get()) {
        return Ok(schedule);
    }
    Ok(crate::palw_g14_values_v1::PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1)
}

// ---------------------------------------------------------------------------
// Signed control.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MaterialActionV1 {
    Begin { total_bytes: u64, body_hash: Hash64 },
    Part { draft_id: Hash64, offset: u64, bytes: Vec<u8> },
    Finish { draft_id: Hash64 },
}
/// A submitter's signed intake step. The signature authenticates the requester,
/// never the material: material is authenticated against the original.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignedMaterialIntakeV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub target: MaterialTargetV1,
    pub submitter: PalwBondKeyV2,
    pub action: MaterialActionV1,
    pub signature: Vec<u8>,
}
impl SignedMaterialIntakeV1 {
    pub fn signing_message(&self, network: Hash64) -> Result<Hash64, String> {
        if self.version != 1 {
            return Err("material intake version".into());
        }
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/material-intake/v1",
            &(network, self.version, self.claim, self.scope_id, self.target, self.submitter, &self.action))
    }
    pub fn verify_v1(&self, network: Hash64, pubkey: &[u8]) -> Result<(), String> {
        if self.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
            return Err("material intake signature length".into());
        }
        if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(pubkey,
            self.signing_message(network)?.as_byte_slice(), &self.signature, MATERIAL_INTAKE_CONTEXT_V1) {
            return Err("material intake signature invalid".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rooted rows (appended staged-court key/row tags; no old tag changes).
// ---------------------------------------------------------------------------

/// One submitter's incomplete body. Not a slot: it never blocks another draft.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MaterialDraftV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub target: MaterialTargetV1,
    pub submitter: PalwBondKeyV2,
    pub draft_id: Hash64,
    pub total_bytes: u64,
    pub body_hash: Hash64,
    pub begun_block: BlockHash,
    pub begun_daa: u64,
    pub last_daa: u64,
    pub accepted_bytes: u64,
    pub head: Hash64,
    pub expires_daa: u64,
    pub material_until_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct MaterialPartV1 {
    pub version: u16,
    pub claim: Hash64,
    pub draft_id: Hash64,
    pub offset: u64,
    pub previous: Hash64,
    pub bytes: Vec<u8>,
    pub daa: u64,
    pub expires_daa: u64,
}
impl BorshDeserialize for MaterialPartV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        Ok(Self {
            version: u16::deserialize_reader(r)?,
            claim: Hash64::deserialize_reader(r)?,
            draft_id: Hash64::deserialize_reader(r)?,
            offset: u64::deserialize_reader(r)?,
            previous: Hash64::deserialize_reader(r)?,
            bytes: crate::palw_public_binding_v1::read_bounded_bytes_v1(r, PART)?,
            daa: u64::deserialize_reader(r)?,
            expires_daa: u64::deserialize_reader(r)?,
        })
    }
}
/// The canonical (first complete authenticated) body of one target. Raw bytes
/// are kept and re-authenticated on every load; they are never a sealed flag.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MaterialRowV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub target: MaterialTargetV1,
    pub target_id: Hash64,
    pub bytes: Vec<u8>,
    pub digest: Hash64,
    pub block: BlockHash,
    pub accepted_daa: u64,
    pub completed_by: PalwBondKeyV2,
    pub material_until_daa: u64,
}
/// Per (claim, submitter) storage reservation. The budget is derived from the
/// original once, never declared; each submitter has the whole requirement so a
/// colluding submitter cannot exhaust an honest one's room.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MaterialLedgerV1 {
    pub version: u16,
    pub claim: Hash64,
    pub submitter: PalwBondKeyV2,
    pub budget_bytes: u64,
    pub reserved_bytes: u64,
    /// Accounting of the schedule price; the payer is part of the unadopted schedule.
    pub charged_sompi: u64,
    pub material_until_daa: u64,
}

pub fn material_part_digest_v1(p: &MaterialPartV1) -> Result<Hash64, String> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/material-part/v1", p)
}
impl MaterialDraftV1 {
    pub fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1
            || *key != Key::CourtMaterialDraft(self.claim, self.draft_id)
            || material_draft_id_v1(self.claim, self.target, self.submitter, self.total_bytes, self.body_hash)? != self.draft_id
            || self.total_bytes == 0
            || self.accepted_bytes > self.total_bytes
            || self.last_daa < self.begun_daa
            || self.expires_daa <= self.begun_daa
            || self.expires_daa > self.material_until_daa
            || (self.accepted_bytes == 0) != (self.head == Hash64::default())
            || self.accepted_bytes != self.total_bytes && self.accepted_bytes % PART as u64 != 0
        {
            return Err("material draft version/key/declaration/cursor/clock".into());
        }
        Ok(())
    }
    pub fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.check(&Key::CourtMaterialDraft(self.claim, self.draft_id))?;
        let c = shared::context(state, self.claim)?;
        if self.scope_id != c.scope_id || self.material_until_daa != c.material_until_daa
            || self.begun_daa < c.original_accepted_daa || self.expires_daa > c.hard_daa {
            return Err("material draft substitutes original scope/window".into());
        }
        Ok(())
    }
    /// Reassemble the retained prefix; every part must chain from the previous.
    pub fn body(&self, state: &PalwChainStateV2) -> Result<Vec<u8>, String> {
        self.check(&Key::CourtMaterialDraft(self.claim, self.draft_id))?;
        let n = usize::try_from(self.accepted_bytes).map_err(|_| "material prefix exceeds address space")?;
        let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
        q.exact_v1::<u8>(self.accepted_bytes).map_err(|e| e.to_string())?;
        let _lease = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(n).map_err(|_| "material prefix allocation refused")?;
        let (mut at, mut head) = (0u64, Hash64::default());
        while at < self.accepted_bytes {
            let Some(Row::CourtMaterialPart(part)) = state.staged_court_row_v1(&Key::CourtMaterialPart(self.draft_id, at)) else {
                return Err("material prefix part missing".into());
            };
            part.check(&Key::CourtMaterialPart(self.draft_id, at))?;
            if part.previous != head || part.claim != self.claim {
                return Err("material prefix chain mismatch".into());
            }
            head = material_part_digest_v1(part)?;
            at = at.checked_add(part.bytes.len() as u64).ok_or("material prefix overflow")?;
            bytes.extend_from_slice(&part.bytes);
        }
        if at != self.accepted_bytes || head != self.head {
            return Err("material prefix head mismatch".into());
        }
        Ok(bytes)
    }
}
impl MaterialPartV1 {
    pub fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1 || *key != Key::CourtMaterialPart(self.draft_id, self.offset)
            || self.bytes.is_empty() || self.bytes.len() > PART || self.offset % PART as u64 != 0 {
            return Err("material part version/key/bound".into());
        }
        Ok(())
    }
}
impl MaterialRowV1 {
    pub fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1 || *key != Key::CourtMaterial(self.claim, self.target_id)
            || self.target.id_v1(self.claim)? != self.target_id
            || matches!(self.target, MaterialTargetV1::FullIds)
            || self.bytes.is_empty()
            || self.digest != crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&self.bytes) {
            return Err("material row version/key/target/digest".into());
        }
        Ok(())
    }
    /// Reload re-authenticates the raw bytes against the retained original.
    pub fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.check(&Key::CourtMaterial(self.claim, self.target_id))?;
        let c = shared::context(state, self.claim)?;
        if self.scope_id != c.scope_id || self.material_until_daa != c.material_until_daa
            || self.accepted_daa < c.original_accepted_daa || self.accepted_daa >= c.hard_daa {
            return Err("material row substitutes original scope/clock".into());
        }
        let source = c.source(state)?;
        decode_material_v1(state, c, &source, self.claim, self.target, &self.bytes, None).map(|_| ())
    }
}
impl MaterialLedgerV1 {
    pub fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1 || *key != Key::CourtMaterialLedger(self.claim, self.submitter)
            || self.reserved_bytes > self.budget_bytes {
            return Err("material ledger version/key/budget".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Native requirements (never caller-declared).
// ---------------------------------------------------------------------------

fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "material budget overflow".into())
}
fn frame_of(state: &PalwChainStateV2, claim: Hash64) -> Result<&crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1, String> {
    crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim).ok_or_else(|| "material original frame missing".into())
}
fn prompt_form(state: &PalwChainStateV2, claim: Hash64) -> Result<PromptForm, String> {
    Ok(frame_of(state, claim)?.plan_v1().owner_work.rules.prompt_form)
}
/// Upper bound of the Borsh width of tile `tile` of a `count`-id Merkle prompt.
fn prompt_tile_bound(count: u32, tile: u32) -> Result<u64, String> {
    let tiles = crate::palw_prompt_ids_v1::prompt_ids_tile_count_v1(u64::from(count)).ok_or("material prompt tile count")?;
    if u64::from(tile) >= tiles {
        return Err("material prompt tile outside original".into());
    }
    let len = crate::palw_prompt_ids_v1::prompt_ids_tile_len_v1(u64::from(count), u64::from(tile)).ok_or("material prompt tile len")? as u64;
    let depth = if tiles <= 1 { 0 } else { 64 - u64::from((tiles - 1).leading_zeros()) };
    // count + tile_index + Vec len + ids + (leaf_index + leaf_hash + Vec len + siblings)
    Ok(4 + 4 + 4 + 4 * len + 8 + 64 + 4 + 64 * depth)
}
fn prompt_bound(state: &PalwChainStateV2, claim: Hash64, source: &Source) -> Result<u64, String> {
    let count = source.header_v1().job_context.declared_prefill_tokens;
    match prompt_form(state, claim)? {
        PromptForm::Flat => Ok(4 + 4 * u64::from(count)),
        PromptForm::MerkleV1 => {
            let tiles = crate::palw_prompt_ids_v1::prompt_ids_tile_count_v1(u64::from(count)).ok_or("material prompt tile count")?;
            let mut total = 0;
            for tile in 0..tiles {
                total = add(total, prompt_tile_bound(count, u32::try_from(tile).map_err(|_| "material prompt tile index")?)?)?;
            }
            Ok(total)
        }
    }
}
/// The original's registered program, decoded under native admission.
pub(crate) fn class_program_v1(state: &PalwChainStateV2, claim: Hash64)
    -> Result<crate::palw_native_resource_v1::NativeRetainedV1<misaka_palw_tir::program::TirProgramV1>, String> {
    let frame = frame_of(state, claim)?;
    let record = state.tir_class_v1(&frame.original_v1().job.class_id).ok_or("material registered class absent")?;
    crate::palw_native_resource_v1::decode_native_tir_program_v1(record.program.as_slice()).map_err(|e| e.to_string())
}
/// Ascending union of the parameter coordinates any numerical root of the
/// original may read — the same input cover the preclaim requirement
/// derivation (`palw_tir_prepared_original_requirements_v1`) retains.
pub(crate) fn required_parameter_leaves_v1(state: &PalwChainStateV2, claim: Hash64,
    budget: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1) -> Result<Vec<u32>, String> {
    let frame = frame_of(state, claim)?;
    let class = frame.original_v1().job.class_id;
    let record = state.tir_class_v1(&class).ok_or("material registered class absent")?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, class).ok_or("material registered layout absent")?;
    let source = Source::new_native_v1(record, layout, frame.header_v1(), frame.original_v1().execution_root,
        frame.original_v1().trace_root, frame.descriptor_v1().maximum_leaves).map_err(|e| e.to_string())?;
    let cover = Source::root_input_cover_native_v1(&source, record, layout, budget).map_err(|e| e.to_string())?;
    Ok(cover.parameters_v1().union.clone())
}
/// Exact native byte bound of one target. Declared sizes above it are refused.
pub(crate) fn target_bound_v1(state: &PalwChainStateV2, c: &shared::SharedTirContextV1, source: &Source,
    target: MaterialTargetV1, program: &misaka_palw_tir::program::TirProgramV1, leaves: &[u32]) -> Result<u64, String> {
    use crate::palw_tir_court_v1::{palw_tir_param_carriage_bytes_v1, PalwTirParamCarriageV1};
    match target {
        MaterialTargetV1::Parameter { leaf } => {
            if leaves.binary_search(&leaf).is_err() {
                return Err("material parameter is not an input of any numerical root of the original".into());
            }
            palw_tir_param_carriage_bytes_v1(program, PalwTirParamCarriageV1::PerLeafOpenings, &[leaf])
                .and_then(|n| n.checked_sub(4)).ok_or_else(|| "material parameter width not derivable".into())
        }
        MaterialTargetV1::PromptFlat => match prompt_form(state, c.claim)? {
            PromptForm::Flat => Ok(4 + 4 * u64::from(source.header_v1().job_context.declared_prefill_tokens)),
            _ => Err("material prompt form is not the original's".into()),
        },
        MaterialTargetV1::PromptTile { tile } => match prompt_form(state, c.claim)? {
            PromptForm::MerkleV1 => prompt_tile_bound(source.header_v1().job_context.declared_prefill_tokens, tile),
            _ => Err("material prompt form is not the original's".into()),
        },
        MaterialTargetV1::Unit { ordinal } => {
            let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame_of(state, c.claim)?, source, c.profile_id)?;
            let q = set.request_at(ordinal)?;
            if matches!(q, Query::Ids { .. }) {
                return Err("material IDs require the FullIds target".into());
            }
            Ok(crate::palw_tir_public_owned_v1::request_body_bound_v1(source, q)?.0)
        }
        MaterialTargetV1::FullIds => Ok(crate::palw_tir_public_owned_v1::request_body_bound_v1(source, Query::Ids { offset: 0 })?.0),
    }
}
/// The per-claim requirement: every unit, the complete IDs, every required
/// parameter at its exact opening width and the whole prompt in native form.
pub(crate) fn claim_budget_v1(state: &PalwChainStateV2, c: &shared::SharedTirContextV1, source: &Source,
    program: &misaka_palw_tir::program::TirProgramV1, leaves: &[u32]) -> Result<u64, String> {
    use crate::palw_tir_court_v1::{palw_tir_param_carriage_bytes_v1, PalwTirParamCarriageV1};
    let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame_of(state, c.claim)?, source, c.profile_id)?;
    let mut total = 0u64;
    for ordinal in 0..set.len() {
        let q = set.request_at(ordinal)?;
        if matches!(q, Query::Ids { .. }) {
            continue;
        }
        total = add(total, crate::palw_tir_public_owned_v1::request_body_bound_v1(source, q)?.0)?;
    }
    if let Ok((ids, _)) = crate::palw_tir_public_owned_v1::request_body_bound_v1(source, Query::Ids { offset: 0 }) {
        total = add(total, ids)?;
    }
    if !leaves.is_empty() {
        total = add(total, palw_tir_param_carriage_bytes_v1(program, PalwTirParamCarriageV1::PerLeafOpenings, leaves)
            .ok_or("material parameter union width not derivable")?)?;
    }
    add(total, prompt_bound(state, c.claim, source)?)
}

// ---------------------------------------------------------------------------
// Authentication (decode against the retained original).
// ---------------------------------------------------------------------------

pub(crate) enum DecodedMaterialV1 {
    Parameter(crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_artifact::PalwArtifactOpeningV1>),
    PromptFlat(Vec<u32>),
    PromptTile(PalwPromptIdsOpeningV1),
    Unit(crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedUnitV1),
}
/// `leaves`: when given, a parameter must be in the original's input union
/// (enforced at ingress; reload re-authenticates identity only).
pub(crate) fn decode_material_v1(state: &PalwChainStateV2, c: &shared::SharedTirContextV1, source: &Source, claim: Hash64,
    target: MaterialTargetV1, bytes: &[u8], leaves: Option<&[u32]>) -> Result<DecodedMaterialV1, String> {
    if c.claim != claim {
        return Err("material context of another original".into());
    }
    let ctx = &source.header_v1().job_context;
    match target {
        MaterialTargetV1::Parameter { leaf } => {
            if leaves.is_some_and(|l| l.binary_search(&leaf).is_err()) {
                return Err("material parameter is not an input of any numerical root of the original".into());
            }
            c.decode_parameter_opening(state, c.scope_id, leaf, bytes).map(DecodedMaterialV1::Parameter)
        }
        MaterialTargetV1::PromptFlat => {
            let form = prompt_form(state, claim)?;
            if !matches!(form, PromptForm::Flat) || bytes.len() as u64 != 4 + 4 * u64::from(ctx.declared_prefill_tokens) {
                return Err("material flat prompt form/width".into());
            }
            let ids: Vec<u32> = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
            if ids.len() != ctx.declared_prefill_tokens as usize
                || !crate::palw_prompt_ids_v1::prompt_token_ids_match_v1(form, &ids, &ctx.prompt_token_ids_hash) {
                return Err("material flat prompt does not match original count/commitment".into());
            }
            Ok(DecodedMaterialV1::PromptFlat(ids))
        }
        MaterialTargetV1::PromptTile { tile } => {
            if !matches!(prompt_form(state, claim)?, PromptForm::MerkleV1)
                || bytes.len() as u64 > prompt_tile_bound(ctx.declared_prefill_tokens, tile)? {
                return Err("material prompt tile form/width".into());
            }
            let opening: PalwPromptIdsOpeningV1 = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
            if opening.tile_index != tile {
                return Err("material prompt tile substitutes coordinate".into());
            }
            crate::palw_prompt_ids_v1::verify_prompt_ids_opening_v1(&ctx.prompt_token_ids_hash, ctx.declared_prefill_tokens, &opening)
                .map_err(|e| e.to_string())?;
            Ok(DecodedMaterialV1::PromptTile(opening))
        }
        MaterialTargetV1::Unit { ordinal } => {
            let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame_of(state, claim)?, source, c.profile_id)?;
            let q = set.request_at(ordinal)?;
            if matches!(q, Query::Ids { .. }) {
                return Err("material IDs require the FullIds target".into());
            }
            decode_unit(state, c, source, q, bytes).map(DecodedMaterialV1::Unit)
        }
        MaterialTargetV1::FullIds => Err("material FullIds are retained as the shared ID row".into()),
    }
}
fn decode_unit(state: &PalwChainStateV2, c: &shared::SharedTirContextV1, source: &Source, q: Query, bytes: &[u8])
    -> Result<crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedUnitV1, String> {
    let bound = source.unit_bound_v1(&q)?;
    let _scratch = crate::palw_native_resource_v1::reserve_native_requests_v1(bound.resident_ram_bytes).map_err(|e| e.to_string())?;
    let response = source.decode_response_v1(&q, bytes)?;
    let rows = match q {
        Query::RowsNode { .. } | Query::TileNode { .. } | Query::TraceTile { .. } => {
            match state.staged_court_row_v1(&Key::CourtSharedIds(c.claim)) {
                Some(Row::CourtSharedIds(ids)) => {
                    if ids.scope_id != c.scope_id {
                        return Err("material unit substitutes ID context".into());
                    }
                    Some(source.rows_anchor_from_complete_ids_v1(ids.rows_root, &ids.ids)?)
                }
                _ => None,
            }
        }
        _ => None,
    };
    source.authenticate_with_rows_v1(&q, &response, None, rows.as_ref())
}
pub(crate) fn target_complete_v1(state: &PalwChainStateV2, claim: Hash64, target: MaterialTargetV1) -> Result<bool, String> {
    Ok(match target {
        MaterialTargetV1::FullIds => matches!(state.staged_court_row_v1(&Key::CourtSharedIds(claim)), Some(Row::CourtSharedIds(_))),
        _ => matches!(state.staged_court_row_v1(&Key::CourtMaterial(claim, target.id_v1(claim)?)), Some(Row::CourtMaterial(_))),
    })
}

// ---------------------------------------------------------------------------
// Adapter: the existing builder over rooted material (same borrowed views).
// ---------------------------------------------------------------------------

enum RootedPromptV1 {
    Missing,
    Flat(Vec<u32>),
    Merkle(Vec<PalwPromptIdsOpeningV1>),
}
/// Owned, freshly re-authenticated copy of a claim's rooted material. No
/// local cache, disk, network or fixture fallback: absent material is absent.
#[allow(dead_code)]
pub(crate) struct RootedMaterialV1 {
    units: Vec<crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedUnitV1>,
    params: Vec<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_artifact::PalwArtifactOpeningV1>>,
    ids: Option<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedIdsV1>>,
    prompt: RootedPromptV1,
}
#[allow(dead_code)]
impl RootedMaterialV1 {
    /// Reads canonical intake rows and the existing shared query bodies / ID row.
    pub(crate) fn load_v1(state: &PalwChainStateV2, claim: Hash64) -> Result<Self, String> {
        let c = shared::context(state, claim)?;
        let source = c.source(state)?;
        let ids = match state.staged_court_row_v1(&Key::CourtSharedIds(claim)) {
            Some(Row::CourtSharedIds(row)) => {
                if row.scope_id != c.scope_id {
                    return Err("material ID row substitutes context".into());
                }
                Some(source.retain_complete_ids_v1(row.rows_root, &row.ids)?)
            }
            _ => None,
        };
        let (mut units, mut params, mut flat, mut tiles) = (Vec::new(), Vec::new(), None, Vec::new());
        let (lo, hi) = (Hash64::from_bytes([0u8; 64]), Hash64::from_bytes([0xffu8; 64]));
        for (key, row) in state.staged_court_range_v1(&Key::CourtMaterial(claim, lo), &Key::CourtMaterial(claim, hi)) {
            let Row::CourtMaterial(row) = row else { return Err("material key holds a foreign row".into()) };
            row.check(key)?;
            match decode_material_v1(state, c, &source, claim, row.target, &row.bytes, None)? {
                DecodedMaterialV1::Parameter(p) => params.push(p),
                DecodedMaterialV1::PromptFlat(v) => flat = Some(v),
                DecodedMaterialV1::PromptTile(t) => tiles.push(t),
                DecodedMaterialV1::Unit(u) => units.push(u),
            }
        }
        // Existing rooted shared query bodies are the same authenticated units.
        let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame_of(state, claim)?, &source, c.profile_id)?;
        for ordinal in 0..set.len() {
            let q = set.request_at(ordinal)?;
            if matches!(q, Query::Ids { .. }) || units.iter().any(|u: &crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedUnitV1| u.request_v1() == q) {
                continue;
            }
            if let Some(Row::CourtSharedQuery(row)) = state.staged_court_row_v1(&Key::CourtSharedQuery(set.request_id(ordinal)?)) {
                if let Some(body) = &row.body {
                    units.push(decode_unit(state, c, &source, q, &body.bytes)?);
                }
            }
        }
        params.sort_by_key(|p| p.leaf_index);
        tiles.sort_by_key(|t| t.tile_index);
        let prompt = match (flat, tiles.is_empty()) {
            (Some(v), true) => RootedPromptV1::Flat(v),
            (None, false) => RootedPromptV1::Merkle(tiles),
            (None, true) => RootedPromptV1::Missing,
            (Some(_), false) => return Err("material holds two prompt forms".into()),
        };
        Ok(Self { units, params, ids, prompt })
    }
    pub(crate) fn units_len_v1(&self) -> usize {
        self.units.len()
    }
    pub(crate) fn params_len_v1(&self) -> usize {
        self.params.len()
    }
    /// The existing builder store over this rooted material (retained-parameter path).
    pub(crate) fn store_v1<'a>(&'a self, state: &PalwChainStateV2, claim: Hash64, scope: Hash64)
        -> Result<crate::palw_court_shared_tir_builder_store_v1::SharedTirBuilderStoreV1<'a>, String> {
        use crate::palw_court_shared_tir_builder_store_v1::{PromptMaterialV1, SharedTirBuilderStoreV1};
        let prompt = match &self.prompt {
            RootedPromptV1::Missing => PromptMaterialV1::Missing,
            RootedPromptV1::Flat(v) => PromptMaterialV1::Flat(v),
            RootedPromptV1::Merkle(t) => PromptMaterialV1::Merkle(t),
        };
        SharedTirBuilderStoreV1::new_with_retained_parameters(state, claim, scope, &self.units, &self.params, self.ids.as_deref(), prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn material_schedule_is_the_approved_estimate_unless_injected() {
        set_material_intake_test_schedule_v1(None);
        assert_eq!(material_intake_schedule_v1(), Ok(crate::palw_g14_values_v1::PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1));
    }
    #[test]
    fn material_ids_bind_claim_target_and_literal_submitter() {
        let h = |x: u8| Hash64::from_bytes([x; 64]);
        let t = MaterialTargetV1::Parameter { leaf: 3 };
        assert_ne!(t.id_v1(h(1)).unwrap(), t.id_v1(h(2)).unwrap());
        assert_ne!(t.id_v1(h(1)).unwrap(), MaterialTargetV1::Parameter { leaf: 4 }.id_v1(h(1)).unwrap());
        let a = material_draft_id_v1(h(1), t, PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(7), 0)), 10, h(9)).unwrap();
        let b = material_draft_id_v1(h(1), t, PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(8), 0)), 10, h(9)).unwrap();
        assert_ne!(a, b, "two submitters' partial bodies never share a key");
    }
    #[test]
    fn material_part_wire_is_bounded() {
        let p = MaterialPartV1 { version: 1, claim: Hash64::default(), draft_id: Hash64::default(), offset: 0,
            previous: Hash64::default(), bytes: vec![1; PART], daa: 1, expires_daa: 2 };
        let wire = borsh::to_vec(&p).unwrap();
        assert_eq!(borsh::from_slice::<MaterialPartV1>(&wire).unwrap(), p);
        let mut big = p.clone();
        big.bytes.push(0);
        assert!(borsh::from_slice::<MaterialPartV1>(&borsh::to_vec(&big).unwrap()).is_err());
        assert!(p.check(&Key::CourtMaterialPart(p.draft_id, 0)).is_ok());
        assert!(p.check(&Key::CourtMaterialPart(p.draft_id, 1)).is_err());
    }
    #[test]
    fn prompt_tile_bound_covers_ragged_tile() {
        // 33 ids: two tiles, depth 1; tile 1 holds one id.
        assert_eq!(prompt_tile_bound(33, 1).unwrap(), 4 + 4 + 4 + 4 + 8 + 64 + 4 + 64);
        assert!(prompt_tile_bound(33, 2).is_err());
    }
}
