//! A1 implementation of the A0 r1 NodeChecker port. Private interchange only.
//! A4 must resolve the rooted admission inputs and authorize each carrier before
//! calling this adapter. Signatures here authenticate authors, not native
//! admission, drawn membership, funding, deadlines or computation truth.

pub(crate) mod protected_batch_case;
pub mod record;
mod tfp1_hf_artifact;
mod tfp1_material;
pub use tfp1_hf_artifact::PublicTfp1HfArtifactV1;
pub(crate) mod tfp1_original;
pub use tfp1_original::native_frame::PublicTfp1FrameV1;

pub use tfp1_material::{PublicTfp1ColdMaterialV1, public_ordinary_tfp1_rules_material_v1, public_tfp1_cold_material_v1};

use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeHeapV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, native_struct, native_tuple_struct,
    native_unit, wire::wire_struct,
};
use crate::palw_positive_check_v2::{CheckError, CheckLimits, CheckOutcome, CheckProgress, CoveragePlan, ExactChecker};
use crate::palw_tfp1_original_history_v1::{PUBLIC_TFP1_HISTORY_TABLE_V1, Tfp1OriginalHistoryBoundaryV1};
use crate::{
    Hash64, palw_attempt_rules_v1::palw_attempt_output_root_v1, palw_decode_select_v2::PalwDecodeSamplingV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1, palw_tir_class_v1::PalwTirClassV1, palw_tir_step_v1::PalwTirStepBindingV1,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::{ChainScope, Digest, PUBKEY_BYTES, PrincipalId, ResultId, SIGNATURE_BYTES, WitnessId},
    messages::{
        AdmissionContext, AggregateBudget, CheckProfile, ClaimForm, CoverageCursor, CoverageDescriptor, MinerResultCommit,
        PanelWitnessCommit, coverage_plan_id,
    },
    ports::{NodeChecker, require_next},
    public_one_honest::{
        CaseId, Claimant, FirstNoticeDemand, FullReplayApproval, ManifestEntryV1, ManifestRequest, ManifestRequestContext,
        ManifestResponseRecordV1, ManifestScopeRules, ManifestV1, MaterialCaseDemand, MaterialId, MaterialScope,
        MissingInspectionOutcome, OriginalClock, OriginalCommit, OriginalId, PanelBond, PendingManifestCase,
        PendingManifestCaseRecordV1, ProtectedRescueDemand, PublicDrawMemberV1, ReplayApprovalContext, SignedPublicOriginalV1,
        VerificationSchedule, VerifiedManifestDeliveryV1, manifest_limits_id_v1, public_draw_id_v1, public_draw_members_digest_v1,
        public_original_service_contract_id_v1, require_accepted_after_v1, tir_wide_rules_material_entry_v1,
    },
    reservations::RemainingCapacity,
    rows::{AcceptedMaterial, AcceptedOrigin, AggregateUsage},
    tir::{EXACT_PART_HEADER_BYTES, decode_frame, exact_derivation_id, greedy_selector_id, provenance_id, raw_part_budget},
    wire::{AuthenticatedEvidenceFrame, EVIDENCE_HEADER_BYTES, EvidenceFrame},
};

pub const PART_OVERHEAD: u64 = (EVIDENCE_HEADER_BYTES + EXACT_PART_HEADER_BYTES) as u64;

pub(crate) const PUBLIC_ORIGINAL_ADMISSION_TABLE_V1: u8 = 198;
pub(crate) const PUBLIC_PENDING_MANIFEST_TABLE_V1: u8 = 199;
pub(crate) const PUBLIC_MANIFEST_RESPONSE_TABLE_V1: u8 = 200;
pub(crate) const PUBLIC_DRAW_TABLE_V1: u8 = 201;
pub(crate) const PUBLIC_REPLAY_APPROVAL_TABLE_V1: u8 = 202;
pub(crate) const PUBLIC_COURT_RECORD_TABLE_V1: u8 = 203;
pub(crate) const PUBLIC_TFP1_LINK_TABLE_V1: u8 = 204;
pub(crate) const PUBLIC_MANIFEST_PAID_TABLE_V1: u8 = 205;

pub(crate) fn is_public_verification_table_v1(table: u8) -> bool {
    matches!(
        table,
        PUBLIC_ORIGINAL_ADMISSION_TABLE_V1
            | PUBLIC_PENDING_MANIFEST_TABLE_V1
            | PUBLIC_MANIFEST_RESPONSE_TABLE_V1
            | PUBLIC_DRAW_TABLE_V1
            | PUBLIC_REPLAY_APPROVAL_TABLE_V1
            | PUBLIC_COURT_RECORD_TABLE_V1
            | PUBLIC_TFP1_LINK_TABLE_V1
            | PUBLIC_MANIFEST_PAID_TABLE_V1
            | PUBLIC_TFP1_HISTORY_TABLE_V1
    )
}

pub(crate) fn check_public_verification_row_v1(table: u8, key: &[u8], bytes: &[u8]) -> Result<(), ContractError> {
    match table {
        PUBLIC_ORIGINAL_ADMISSION_TABLE_V1 => PublicOriginalAdmissionRecordV1::decode_row(key, bytes).map(|_| ()),
        PUBLIC_PENDING_MANIFEST_TABLE_V1 => PendingManifestCaseRecordV1::decode_row(key, bytes).map(|_| ()),
        PUBLIC_MANIFEST_RESPONSE_TABLE_V1 => ManifestResponseRecordV1::decode_row(key, bytes).map(|_| ()),
        PUBLIC_DRAW_TABLE_V1 => PublicDrawRecordV1::decode_row(key, bytes).map(|_| ()),
        PUBLIC_REPLAY_APPROVAL_TABLE_V1 => PublicReplayApprovalRecordV1::decode_row(key, bytes).map(|_| ()),
        PUBLIC_COURT_RECORD_TABLE_V1 => PublicOriginalCourtRecordV1::decode_row(key, bytes).map(|_| ()),
        PUBLIC_TFP1_LINK_TABLE_V1 => tfp1_original::native_link::Tfp1PublicOriginalLinkV1::decode_row(key, bytes).map(|_| ()),
        PUBLIC_MANIFEST_PAID_TABLE_V1 => tfp1_original::manifest_paid::PaidManifestRequestRowV1::decode_row(key, bytes).map(|_| ()),
        PUBLIC_TFP1_HISTORY_TABLE_V1 => Tfp1OriginalHistoryBoundaryV1::decode_row(key, bytes).map(|_| ()),
        _ => Ok(()),
    }
}

/// Transport quote only; A4 reserves native wrapper/transaction overhead too.
/// Existing frame limits remain unsigned and are never silently reinterpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignedTransportQuote {
    pub max_packet_bytes: u64,
    pub unsigned_frame_bytes: u64,
    pub signature_bytes: u64,
    pub total_packet_bytes: u64,
}
pub(crate) fn signed_quote(count: u64, frame_cap: u64) -> Result<SignedTransportQuote, AdapterError> {
    let max_packet_bytes = frame_cap.checked_add(SIGNATURE_BYTES as u64).ok_or(CheckError::Budget)?;
    let unsigned_frame_bytes = count.checked_mul(frame_cap).ok_or(CheckError::Budget)?;
    let signature_bytes = count.checked_mul(SIGNATURE_BYTES as u64).ok_or(CheckError::Budget)?;
    let total_packet_bytes = unsigned_frame_bytes.checked_add(signature_bytes).ok_or(CheckError::Budget)?;
    Ok(SignedTransportQuote { max_packet_bytes, unsigned_frame_bytes, signature_bytes, total_packet_bytes })
}
fn digest(hash: Hash64) -> Digest {
    Digest(hash.as_bytes())
}

/// Describe bounded tokenizer bytes by the existing native raw identity.
/// This only identifies the data; the execution profile owns its parser.
pub fn public_tokenizer_material_v1(
    bytes: &[u8],
    rules: misaka_palw_positive_contract::public_one_honest::ManifestScopeRules,
) -> Result<misaka_palw_positive_contract::public_one_honest::ManifestEntryV1, ContractError> {
    use misaka_palw_positive_contract::public_one_honest::{ManifestEntryV1, tokenizer_json_material_codec_v1};
    if rules.codec != tokenizer_json_material_codec_v1() {
        return Err(ContractError::UnsupportedProfile);
    }
    let total_bytes = bytes.len() as u64;
    if total_bytes == 0 || total_bytes > rules.max_bytes || total_bytes > u64::from(rules.chunk_bytes) {
        return Err(ContractError::Budget);
    }
    let subject = digest(crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(bytes));
    Ok(ManifestEntryV1 { subject, content_root: subject, codec: rules.codec, total_bytes, chunk_bytes: rules.chunk_bytes })
}

/// Match the original against execution metadata resolved by admission or court.
pub fn check_public_original_execution_v1(
    original: &OriginalCommit,
    job: &PalwJobContextV2,
    class: &PalwTirClassV1,
    artifact: Hash64,
) -> Result<(), ContractError> {
    if original.job != digest(job.job_id)
        || original.artifact != digest(artifact)
        || original.registered_class != digest(job.shape_profile_id)
        || original.input_tokens != digest(job.prompt_token_ids_hash)
        || original.tokenizer != digest(job.tokenizer_id)
        || original.execution_rules != digest(job.cu_ruleset_id)
        || original.generation_seed != job.execution_seed
        || original.prefill != job.declared_prefill_tokens
        || original.decode != job.exact_decode_tokens
        || original.max_context != job.max_context_tokens
        || original.execution != digest(job.context_hash())
        || original.registered_class != digest(class.class_id(&artifact))
        || original.program != digest(class.graph_ir_root())
    {
        return Err(ContractError::Scope);
    }
    Ok(())
}

/// Public preimage for the native wide TIR execution rules selected by the job.
pub fn public_wide_tir_rules_material_v1(
    job: &PalwJobContextV2,
    program: &misaka_palw_tir::TirProgramV1,
    rules: ManifestScopeRules,
) -> Result<(ManifestEntryV1, [u8; 256]), ContractError> {
    if !program.wide_logits_q24()
        || program.prim_set_id != crate::palw_tir_v1::palw_tir_prim_set_id_v1().as_bytes()
        || job.trace_scheme_id != crate::palw_logits_q24_v2::scheme_id()
    {
        return Err(ContractError::UnsupportedProfile);
    }
    let mut bytes = [0; 256];
    for (part, field) in bytes.chunks_exact_mut(64).zip([
        program.prim_set_id,
        program.logits_scheme_id,
        job.trace_scheme_id.as_bytes(),
        job.cu_ruleset_id.as_bytes(),
    ]) {
        part.copy_from_slice(&field);
    }
    let entry = tir_wide_rules_material_entry_v1(digest(job.cu_ruleset_id), &bytes, rules)?;
    Ok((entry, bytes))
}

/// Facts resolved from the canonical job, class and funded execution owner.
/// The native admission path supplies these independently of the candidate.
pub struct PublicOriginalAdmissionV1<'a> {
    pub scope: ChainScope,
    pub job: &'a PalwJobContextV2,
    pub class: &'a PalwTirClassV1,
    pub artifact: Hash64,
    pub miner: PrincipalId,
    pub prompt: Digest,
    pub initial_state: Digest,
    pub dependencies: Digest,
    pub service_contract: Digest,
    pub max_manifest_bytes: u32,
    pub manifest_scope_limits: &'a [ManifestScopeRules; MaterialScope::COUNT],
    pub manifest_total_limit: u64,
}

/// Native reservation inputs from the accepted profile and current ledger.
pub struct PublicOriginalReservationV1<'a> {
    pub origin: AcceptedOrigin,
    pub service_obligation: Digest,
    pub notice: FirstNoticeDemand,
    pub case: MaterialCaseDemand,
    /// Includes the manifest body and the profile's maximum referenced payload.
    pub initial_supply: RemainingCapacity,
    pub rescue: ProtectedRescueDemand,
    pub usage: &'a AggregateUsage,
    pub budget: &'a AggregateBudget,
}

fn public_original_capacity_v1(
    max_manifest_bytes: u32,
    manifest_payload_bytes: u64,
    max_unique_source_bytes: u64,
    initial_supply: RemainingCapacity,
    rescue: ProtectedRescueDemand,
) -> Result<RemainingCapacity, ContractError> {
    let body_bytes = u64::from(ManifestV1::fetch_size(max_manifest_bytes)?);
    let material_bytes = body_bytes.checked_add(manifest_payload_bytes).ok_or(ContractError::Budget)?;
    if manifest_payload_bytes < MaterialScope::COUNT as u64
        || material_bytes > max_unique_source_bytes
        || initial_supply.transmitted_bytes < material_bytes
        || initial_supply.retained_bytes < material_bytes
        || rescue.per_drawn_bond.transmitted_bytes < material_bytes
        || (rescue.max_outsiders != 0 && rescue.per_outsider.transmitted_bytes < material_bytes)
    {
        return Err(ContractError::Budget);
    }
    initial_supply.checked_add(rescue.total()?)
}

/// Checked inputs for the native fold to book and persist atomically.
#[derive(Debug, PartialEq, Eq)]
pub struct PreparedPublicOriginalV1 {
    pub original: OriginalId,
    pub schedule: VerificationSchedule,
    pub capacity: RemainingCapacity,
    carrier: AcceptedMaterial,
    manifest_material: MaterialId,
    draw_ready_by: u64,
    /// Native readiness floor retained for signed preclaim booking. The
    /// deadline alone cannot distinguish late detection from a wider window.
    material_request_ready_by: u64,
    initial_supply: RemainingCapacity,
    rescue: ProtectedRescueDemand,
    budget: AggregateBudget,
    max_manifest_bytes: u32,
    manifest_payload_bytes: u64,
    manifest_limits: Digest,
}

/// An original qualified at an accepted carrier with its native reservation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedPublicOriginalRecordV1(PublicOriginalAdmissionRecordV1);

impl PreparedPublicOriginalRecordV1 {
    pub(crate) fn storage_row(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<Option<([u8; 64], &PublicOriginalAdmissionRecordV1)>, ContractError> {
        let row = &self.0;
        require_current_public_origin_v1(row.carrier.origin, point)?;
        if row.reservation == Digest([0; 64]) || state.kernel_route_state_v1().is_none() {
            return Err(ContractError::Scope);
        }
        // Admission can be installed inside the ordinary object checkpoint,
        // before LastPoint advances. Validate at the explicit accepting point
        // rather than using the reader for a completed state snapshot.
        let key = row.original.0.0;
        let previous =
            state.kernel_route_state_v1().and_then(|route| route.aux.get(&(PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, key.to_vec())));
        if let Some(bytes) = previous {
            let existing = PublicOriginalAdmissionRecordV1::decode_row(&key, bytes)?;
            existing.check_state_point(point)?;
            return if existing == *row { Ok(None) } else { Err(ContractError::Scope) };
        }
        Ok(Some((row.original.0.0, row)))
    }
}

/// Fixed-size record to store in the same state update as the reservation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PublicOriginalAdmissionRecordV1 {
    original: OriginalId,
    carrier: AcceptedMaterial,
    manifest_material: MaterialId,
    reservation: Digest,
    initial_supply: RemainingCapacity,
    rescue: ProtectedRescueDemand,
    budget: AggregateBudget,
    max_manifest_bytes: u32,
    manifest_payload_bytes: u64,
    manifest_limits: Digest,
    deadlines: [u64; 8],
}

/// Identity of a verdict included under one restored admission.
/// Revalidation uses the case evidence and the canonical inclusion block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PublicOriginalCourtRecordV1 {
    original: OriginalId,
    admission_carrier: AcceptedMaterial,
    reservation: Digest,
    evidence_id: Hash64,
    origin: AcceptedOrigin,
}

/// A local refutation linked to its admitted original and inclusion point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedPublicCourtRecordV1(PublicOriginalCourtRecordV1);

impl PreparedPublicCourtRecordV1 {
    pub(crate) fn storage_row(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<Option<([u8; PublicOriginalCourtRecordV1::KEY_BYTES], &PublicOriginalCourtRecordV1)>, ContractError> {
        let row = &self.0;
        require_current_public_origin_v1(row.origin, point)?;
        state.check_public_court_record_v1(row)?;
        if let Some(existing) = state.public_original_court_record_v1(row.original, row.evidence_id)? {
            return if existing == *row { Ok(None) } else { Err(ContractError::Scope) };
        }
        Ok(Some((PublicOriginalCourtRecordV1::key(row.original, row.evidence_id), row)))
    }
}

impl PublicOriginalCourtRecordV1 {
    pub(crate) const KEY_BYTES: usize = 2 * 64;

    fn key(original: OriginalId, evidence_id: Hash64) -> [u8; Self::KEY_BYTES] {
        let mut key = [0; Self::KEY_BYTES];
        key[..64].copy_from_slice(&original.0.0);
        key[64..].copy_from_slice(&evidence_id.as_bytes());
        key
    }

    fn decode_row(key: &[u8], bytes: &[u8]) -> Result<Self, ContractError> {
        let row: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        if key != Self::key(row.original, row.evidence_id).as_slice() {
            return Err(ContractError::Identity);
        }
        Ok(row)
    }

    pub fn check_restore(
        &self,
        admission: &PublicOriginalAdmissionRecordV1,
        prepared: &PreparedPublicOriginalV1,
        reservation: Digest,
        origin: AcceptedOrigin,
        verdict: &crate::palw_tir_court_v1::PublicOriginalCourtVerdictV1,
    ) -> Result<(), ContractError> {
        let expected = admission.court_record(prepared, reservation, origin, verdict)?;
        if *self != expected {
            return Err(ContractError::Scope);
        }
        Ok(())
    }
}

/// Membership and per-bond rescue capacity validated for one original.
pub struct PreparedPublicDrawV1 {
    original: OriginalId,
    admission_carrier: AcceptedMaterial,
    draw: Digest,
    beacon: Digest,
    draw_point: crate::palw_state_v2::PalwBlockContextV2,
    hard: u64,
    full_scope: Digest,
    members: std::collections::BTreeMap<(Digest, u32), PublicDrawMemberV1>,
    ordered_members: Digest,
    per_bond: RemainingCapacity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PublicDrawRecordV1 {
    original: OriginalId,
    admission_carrier: AcceptedMaterial,
    draw: Digest,
    beacon: Digest,
    draw_point: crate::palw_state_v2::PalwBlockContextV2,
    hard: u64,
    full_scope: Digest,
    ordered_members: Digest,
    per_bond: RemainingCapacity,
}

/// A draw prepared from the accepted original and its selected members.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedPublicDrawRecordV1(PublicDrawRecordV1);

impl PreparedPublicDrawRecordV1 {
    pub(crate) fn storage_row(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<Option<([u8; 64], &PublicDrawRecordV1)>, ContractError> {
        let row = &self.0;
        if row.draw_point != *point {
            return Err(ContractError::Scope);
        }
        state.check_public_draw_record_v1(row)?;
        if let Some(existing) = state.public_draw_record_v1(row.original)? {
            return if existing == *row { Ok(None) } else { Err(ContractError::Scope) };
        }
        Ok(Some((row.original.0.0, row)))
    }
}

impl PublicDrawRecordV1 {
    fn decode_row(key: &[u8], bytes: &[u8]) -> Result<Self, ContractError> {
        let row: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        if key != row.original.0.0.as_slice() {
            return Err(ContractError::Identity);
        }
        Ok(row)
    }

    /// Restore only against a draw re-derived from the native history.
    pub fn check_restore(&self, draw: &PreparedPublicDrawV1) -> Result<(), ContractError> {
        if *self != draw.record() {
            return Err(ContractError::Scope);
        }
        Ok(())
    }
}

/// Canonical inclusion identity of an authenticated replay approval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PublicReplayApprovalRecordV1 {
    original: OriginalId,
    draw: Digest,
    full_scope: Digest,
    ordered_members: Digest,
    bond: PanelBond,
    origin: AcceptedOrigin,
    approval_digest: Digest,
}

// The native restore preflight visits the same fixed fields as the Borsh
// records. The owning fold still has to revalidate each record's provenance.
native_tuple_struct!(OriginalId; id);
wire_struct!(OriginalId; Digest);
native_struct!(RemainingCapacity; transmitted_bytes, additional_work, retained_bytes);
wire_struct!(RemainingCapacity; u64, u64, u64);
native_struct!(ProtectedRescueDemand; max_drawn_bonds, per_drawn_bond, max_outsiders, per_outsider, defensive);
wire_struct!(ProtectedRescueDemand; u64, RemainingCapacity, u64, RemainingCapacity, RemainingCapacity);
native_struct!(PanelBond; transaction, index);
wire_struct!(PanelBond; Digest, u32);

native_tuple_struct!(MaterialId; id);
wire_struct!(MaterialId; Digest);
native_tuple_struct!(PreparedPublicOriginalRecordV1; admission);
native_struct!(PublicOriginalAdmissionRecordV1; original, carrier, manifest_material, reservation, initial_supply, rescue, budget, max_manifest_bytes, manifest_payload_bytes, manifest_limits, deadlines);
wire_struct!(PublicOriginalAdmissionRecordV1; OriginalId, AcceptedMaterial, MaterialId, Digest, RemainingCapacity, ProtectedRescueDemand, AggregateBudget, u32, u64, Digest, [u64; 8]);
native_tuple_struct!(PreparedPublicCourtRecordV1; court);
native_struct!(PublicOriginalCourtRecordV1; original, admission_carrier, reservation, evidence_id, origin);
wire_struct!(PublicOriginalCourtRecordV1; OriginalId, AcceptedMaterial, Digest, Hash64, AcceptedOrigin);
native_tuple_struct!(PreparedPublicDrawRecordV1; draw);
native_struct!(PublicDrawRecordV1; original, admission_carrier, draw, beacon, draw_point, hard, full_scope, ordered_members, per_bond);
wire_struct!(PublicDrawRecordV1; OriginalId, AcceptedMaterial, Digest, Digest, crate::palw_state_v2::PalwBlockContextV2, u64, Digest, Digest, RemainingCapacity);
native_struct!(PublicReplayApprovalRecordV1; original, draw, full_scope, ordered_members, bond, origin, approval_digest);
native_struct!(PreparedPublicReplayApprovalV1; row, draw);
wire_struct!(PublicReplayApprovalRecordV1; OriginalId, Digest, Digest, Digest, PanelBond, AcceptedOrigin, Digest);

// Shared contract records keep their fields private. Read exactly the fixed
// frame and let Borsh validate it, including the delivery classification tag.
native_unit!(PendingManifestCaseRecordV1);
native_unit!(ManifestResponseRecordV1);

fn scan_fixed_manifest_record<T: Copy + borsh::BorshDeserialize, const N: usize>(
    reader: &mut NativeWireReaderV1<impl std::io::Read>,
) -> std::io::Result<NativeWireShapeV1> {
    use std::io::Read;
    reader.step()?;
    let mut bytes = [0; N];
    reader.read_exact(&mut bytes)?;
    borsh::from_slice::<T>(&bytes)?;
    Ok(NativeWireShapeV1::default())
}

impl NativeWireScanV1 for PendingManifestCaseRecordV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        scan_fixed_manifest_record::<Self, { Self::WIRE_BYTES }>(reader)
    }
}

impl NativeWireScanV1 for ManifestResponseRecordV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        scan_fixed_manifest_record::<Self, { Self::WIRE_BYTES }>(reader)
    }
}

impl PublicReplayApprovalRecordV1 {
    pub(crate) const KEY_BYTES: usize = 2 * 64 + 4;

    fn key(original: OriginalId, bond: PanelBond) -> [u8; Self::KEY_BYTES] {
        let mut key = [0; Self::KEY_BYTES];
        key[..64].copy_from_slice(&original.0.0);
        key[64..128].copy_from_slice(&bond.transaction.0);
        key[128..].copy_from_slice(&bond.index.to_le_bytes());
        key
    }

    fn decode_row(key: &[u8], bytes: &[u8]) -> Result<Self, ContractError> {
        let row: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        if key != Self::key(row.original, row.bond).as_slice() {
            return Err(ContractError::Identity);
        }
        Ok(row)
    }

    pub fn check_restore(
        &self,
        draw: &PreparedPublicDrawV1,
        approval_until: u64,
        origin: AcceptedOrigin,
        approval: &FullReplayApproval,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), ContractError> {
        if *self != draw.verify_replay_approval(approval_until, origin, approval, key, signature)? {
            return Err(ContractError::Scope);
        }
        Ok(())
    }
}

/// An approval verified against the selected member and the accepted carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedPublicReplayApprovalV1 {
    row: PublicReplayApprovalRecordV1,
    draw: PublicDrawRecordV1,
}

impl PreparedPublicReplayApprovalV1 {
    pub(crate) fn accepted_order(&self) -> (u32, u32) {
        (self.row.origin.accepted_tx_ordinal, self.row.origin.event_ordinal)
    }

    pub(crate) fn storage_row(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<Option<([u8; PublicReplayApprovalRecordV1::KEY_BYTES], &PublicReplayApprovalRecordV1)>, ContractError> {
        let row = &self.row;
        require_current_public_origin_v1(row.origin, point)?;
        if state.public_draw_record_v1(row.original)? != Some(self.draw) {
            return Err(ContractError::Scope);
        }
        if let Some(existing) = state.public_replay_approval_record_v1(row.original, row.bond)? {
            if existing.approval_digest != row.approval_digest {
                return Err(ContractError::Scope);
            }
            return Ok(None);
        }
        Ok(Some((PublicReplayApprovalRecordV1::key(row.original, row.bond), row)))
    }
}

impl PreparedPublicDrawV1 {
    /// Verify the literal carrier at an inclusion point resolved from native history.
    pub fn verify_replay_approval_bytes(
        &self,
        approval_until: u64,
        origin: AcceptedOrigin,
        bytes: &[u8],
    ) -> Result<PublicReplayApprovalRecordV1, ContractError> {
        let signed = misaka_palw_positive_contract::public_one_honest::SignedFullReplayApprovalV1::decode(bytes)?;
        self.verify_replay_approval(approval_until, origin, &signed.approval, &signed.key, &signed.signature)
    }

    /// Prepare a verified carrier for the accepting transition. The fold
    /// checks its inclusion point and the draw rooted in the executing state.
    pub fn prepare_replay_approval_bytes(
        &self,
        approval_until: u64,
        origin: AcceptedOrigin,
        bytes: &[u8],
    ) -> Result<PreparedPublicReplayApprovalV1, ContractError> {
        let row = self.verify_replay_approval_bytes(approval_until, origin, bytes)?;
        Ok(PreparedPublicReplayApprovalV1 { row, draw: self.record() })
    }

    /// Seal the checked draw for the fold. This does not select members.
    pub fn prepare_storage_record(&self) -> PreparedPublicDrawRecordV1 {
        PreparedPublicDrawRecordV1(self.record())
    }

    pub fn record(&self) -> PublicDrawRecordV1 {
        PublicDrawRecordV1 {
            original: self.original,
            admission_carrier: self.admission_carrier,
            draw: self.draw,
            beacon: self.beacon,
            draw_point: self.draw_point,
            hard: self.hard,
            full_scope: self.full_scope,
            ordered_members: self.ordered_members,
            per_bond: self.per_bond,
        }
    }

    pub fn approval_context(&self, bond: PanelBond) -> Result<ReplayApprovalContext, ContractError> {
        let member = self.members.get(&(bond.transaction, bond.index)).ok_or(ContractError::Identity)?;
        Ok(ReplayApprovalContext {
            original: self.original,
            draw: self.draw,
            bond,
            member: member.principal,
            full_scope: self.full_scope,
        })
    }

    pub fn rescue_capacity(&self, bond: PanelBond) -> Result<RemainingCapacity, ContractError> {
        self.approval_context(bond)?;
        Ok(self.per_bond)
    }

    /// Validate a single included vote before it enters the native tally.
    pub fn verify_replay_approval(
        &self,
        approval_until: u64,
        origin: AcceptedOrigin,
        approval: &FullReplayApproval,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<PublicReplayApprovalRecordV1, ContractError> {
        if approval_until > self.hard || !(self.draw_point.daa_score..=approval_until).contains(&origin.accepted_daa) {
            return Err(ContractError::Range);
        }
        check_public_origin_after_draw_v1(origin, &self.draw_point)?;
        let bond = approval.bond;
        approval.authenticate(&self.approval_context(bond)?, key, signature)?;
        Ok(PublicReplayApprovalRecordV1 {
            original: self.original,
            draw: self.draw,
            full_scope: self.full_scope,
            ordered_members: self.ordered_members,
            bond,
            origin,
            approval_digest: approval.signing_digest(),
        })
    }

    /// Check a quorum of included replay assertions. The caller resolves the
    /// canonical accepted origin of each assertion and the profile's deadline.
    /// Local checks, inspection opportunity and Final are separate gates.
    pub fn verify_replay_quorum(
        &self,
        required: usize,
        approval_until: u64,
        approvals: &[(AcceptedOrigin, &FullReplayApproval, &[u8; PUBKEY_BYTES], &[u8; SIGNATURE_BYTES])],
    ) -> Result<Vec<PublicReplayApprovalRecordV1>, ContractError> {
        if approval_until > self.hard || required == 0 || approvals.len() < required || approvals.len() > self.members.len() {
            return Err(ContractError::Range);
        }
        let mut checked = std::collections::BTreeMap::new();
        let mut included_events = std::collections::BTreeSet::new();
        for &(origin, approval, key, signature) in approvals {
            let bond = approval.bond;
            let key_order = (bond.transaction, bond.index);
            let event = (origin.accepting_block.0, origin.accepted_tx_ordinal, origin.event_ordinal);
            if checked.contains_key(&key_order) || !included_events.insert(event) {
                return Err(ContractError::Identity);
            }
            let record = self.verify_replay_approval(approval_until, origin, approval, key, signature)?;
            checked.insert(key_order, record);
        }
        Ok(checked.into_values().collect())
    }
}

/// A signed first manifest request verified against one selected bond's
/// reserved inspection opportunity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedDrawnPublicManifestCaseV1 {
    case: CaseId,
    row: PendingManifestCaseRecordV1,
    admission: PublicOriginalAdmissionRecordV1,
    draw: PublicDrawRecordV1,
}

native_tuple_struct!(CaseId; id);
native_struct!(PreparedDrawnPublicManifestCaseV1; case, row, admission, draw);

impl PreparedDrawnPublicManifestCaseV1 {
    pub(crate) fn storage_row(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<Option<([u8; PendingManifestCaseRecordV1::KEY_BYTES], &PendingManifestCaseRecordV1)>, ContractError> {
        self.storage_row_at_points_v1(state, point, state.last_point().ok_or(ContractError::Scope)?)
    }

    /// Validate the request in the active native fold before its last-point
    /// marker advances. Original admission and draw must already be rooted.
    /// The caller must separately verify the paid carrier and booked slot.
    pub(crate) fn storage_row_at_v1(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<Option<([u8; PendingManifestCaseRecordV1::KEY_BYTES], &PendingManifestCaseRecordV1)>, ContractError> {
        let last = state.last_point().ok_or(ContractError::Scope)?;
        if point != last && (point.blue_score <= last.blue_score || point.daa_score < last.daa_score) {
            return Err(ContractError::Scope);
        }
        self.storage_row_at_points_v1(state, point, point)
    }

    fn storage_row_at_points_v1(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        accepting_point: &crate::palw_state_v2::PalwBlockContextV2,
        observation_point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<Option<([u8; PendingManifestCaseRecordV1::KEY_BYTES], &PendingManifestCaseRecordV1)>, ContractError> {
        let row = &self.row;
        let original = row.original();
        require_current_public_origin_v1(row.origin(), accepting_point)?;
        if state.public_original_admission_record_v1(original)? != Some(self.admission)
            || state.public_draw_record_v1(original)? != Some(self.draw)
        {
            return Err(ContractError::Scope);
        }
        state.check_public_pending_manifest_record_at_v1(row, observation_point)?;
        let key = PendingManifestCaseRecordV1::key(original, self.case);
        let route = state.kernel_route_state_v1().ok_or(ContractError::Scope)?;
        if let Some(bytes) = route.aux.get(&(PUBLIC_PENDING_MANIFEST_TABLE_V1, key.to_vec())) {
            let existing = PendingManifestCaseRecordV1::decode_row(&key, bytes)?;
            state.check_public_pending_manifest_record_at_v1(&existing, observation_point)?;
            return if existing == *row { Ok(None) } else { Err(ContractError::Scope) };
        }
        Ok(Some((key, row)))
    }
}

/// A provider delivery authenticated against the obligation for an open case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedPublicManifestResponseV1 {
    pending: PendingManifestCaseRecordV1,
    row: ManifestResponseRecordV1,
}

native_struct!(PreparedPublicManifestResponseV1; pending, row);

impl PreparedPublicManifestResponseV1 {
    pub fn new(pending: &PendingManifestCase, delivery: &VerifiedManifestDeliveryV1) -> Result<Self, ContractError> {
        let pending = pending.record();
        let row = delivery.record();
        row.check_case(&pending)?;
        Ok(Self { pending, row })
    }

    pub(crate) fn storage_row(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<Option<([u8; ManifestResponseRecordV1::KEY_BYTES], &ManifestResponseRecordV1)>, ContractError> {
        let row = &self.row;
        require_current_public_origin_v1(row.origin(), point)?;
        if state.public_pending_manifest_record_v1(row.original(), row.case())? != Some(self.pending) {
            return Err(ContractError::Scope);
        }
        let key = row.key();
        if let Some(existing) = state.public_manifest_response_record_v1(&key)? {
            return if existing == *row { Ok(None) } else { Err(ContractError::Scope) };
        }
        Ok(Some((key, row)))
    }
}

pub enum PublicManifestClaimantAuthorityV1<'a> {
    Drawn(&'a PreparedPublicDrawV1),
    Outsider(u64),
}

impl PreparedPublicOriginalV1 {
    /// Construct the packet a drawn member signs before inclusion. Draw
    /// membership supplies the signer and reservation, and the native supply
    /// ledger supplies the obligation. Inclusion verifies them again.
    pub fn drawn_manifest_request(
        &self,
        draw: &PreparedPublicDrawV1,
        original: &OriginalCommit,
        bond: PanelBond,
        inspection_scope: Digest,
        supply_obligation: Digest,
    ) -> Result<ManifestRequest, ContractError> {
        if original.id() != self.original
            || draw.original != self.original
            || draw.admission_carrier != self.carrier
            || self.original.full_replay_scope(inspection_scope) != draw.full_scope
        {
            return Err(ContractError::Scope);
        }
        let member = draw.members.get(&(bond.transaction, bond.index)).ok_or(ContractError::Identity)?;
        Ok(ManifestRequest {
            original: self.original,
            manifest_root: original.manifest_root,
            claimant: Claimant::DrawnBond(bond),
            signer: member.principal,
            inspection_scope,
            supply_obligation,
            collect_until: self.schedule.filed_by(),
            reservation: member.rescue_reservation,
        })
    }

    /// Decode an included request and apply the authority already resolved by
    /// the native fold. The packet cannot choose its own draw or slot.
    pub fn verify_manifest_request_bytes(
        &self,
        authority: PublicManifestClaimantAuthorityV1<'_>,
        bytes: &[u8],
        context: &ManifestRequestContext<'_>,
    ) -> Result<PendingManifestCase, ContractError> {
        let signed = misaka_palw_positive_contract::public_one_honest::SignedManifestRequestV1::decode(bytes)?;
        match authority {
            PublicManifestClaimantAuthorityV1::Drawn(draw) => {
                self.verify_drawn_manifest_request(draw, &signed.request, context, &signed.key, &signed.signature)
            }
            PublicManifestClaimantAuthorityV1::Outsider(slot) => {
                self.verify_outsider_manifest_request(slot, &signed.request, context, &signed.key, &signed.signature)
            }
        }
    }

    /// Qualify a drawn member's first request for the native fold. The
    /// admission reservation and the per-bond rescue reservation are distinct.
    pub fn prepare_drawn_manifest_storage_record(
        &self,
        admission_reservation: Digest,
        draw: &PreparedPublicDrawV1,
        bytes: &[u8],
        context: &ManifestRequestContext<'_>,
    ) -> Result<PreparedDrawnPublicManifestCaseV1, ContractError> {
        let pending = self.verify_manifest_request_bytes(PublicManifestClaimantAuthorityV1::Drawn(draw), bytes, context)?;
        Ok(PreparedDrawnPublicManifestCaseV1 {
            case: pending.id(),
            row: pending.record(),
            admission: self.record(admission_reservation),
            draw: draw.record(),
        })
    }

    /// Restore against the literal packet and the authority resolved at its
    /// original inclusion point.
    pub fn restore_manifest_request_bytes(
        &self,
        record: &PendingManifestCaseRecordV1,
        authority: PublicManifestClaimantAuthorityV1<'_>,
        bytes: &[u8],
        context: &ManifestRequestContext<'_>,
    ) -> Result<PendingManifestCase, ContractError> {
        let pending = self.verify_manifest_request_bytes(authority, bytes, context)?;
        record.check_restore(&pending)?;
        Ok(pending)
    }

    /// Restore the draw using the members and inclusion DAA read from native
    /// history. An archived draw record cannot supply its own inputs.
    pub fn restore_draw(
        &self,
        record: &PublicDrawRecordV1,
        beacon: Digest,
        draw_point: crate::palw_state_v2::PalwBlockContextV2,
        members: &[PublicDrawMemberV1],
        relation_plan: Digest,
    ) -> Result<PreparedPublicDrawV1, ContractError> {
        let prepared = self.prepare_draw(beacon, draw_point, members, relation_plan)?;
        record.check_restore(&prepared)?;
        Ok(prepared)
    }

    /// Authenticate a request against an outsider slot assigned by the
    /// native eligibility and reservation ledger.
    pub fn verify_outsider_manifest_request(
        &self,
        slot: u64,
        request: &ManifestRequest,
        context: &ManifestRequestContext<'_>,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<PendingManifestCase, ContractError> {
        if !matches!(context.claimant, Claimant::Outsider(principal) if principal == context.signer) {
            return Err(ContractError::Identity);
        }
        if slot >= self.rescue.max_outsiders {
            return Err(ContractError::Budget);
        }
        self.verify_manifest_request_with_capacity(self.rescue.per_outsider, request, context, key, signature)
    }

    /// Check the protected draw slot before authenticating a manifest request.
    /// The ledger supplies the reservation identity and remaining body bytes.
    pub fn verify_drawn_manifest_request(
        &self,
        draw: &PreparedPublicDrawV1,
        request: &ManifestRequest,
        context: &ManifestRequestContext<'_>,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<PendingManifestCase, ContractError> {
        if draw.original != self.original
            || draw.admission_carrier != self.carrier
            || self.original.full_replay_scope(context.inspection_scope) != draw.full_scope
        {
            return Err(ContractError::Scope);
        }
        if context.origin.accepted_daa < draw.draw_point.daa_score {
            return Err(ContractError::Range);
        }
        check_public_origin_after_draw_v1(context.origin, &draw.draw_point)?;
        let Claimant::DrawnBond(bond) = context.claimant else {
            return Err(ContractError::Identity);
        };
        let member = draw.members.get(&(bond.transaction, bond.index)).ok_or(ContractError::Identity)?;
        if context.signer != member.principal {
            return Err(ContractError::Identity);
        }
        if context.reservation != member.rescue_reservation {
            return Err(ContractError::Budget);
        }
        self.verify_manifest_request_with_capacity(draw.per_bond, request, context, key, signature)
    }

    fn verify_manifest_request_with_capacity(
        &self,
        capacity: RemainingCapacity,
        request: &ManifestRequest,
        context: &ManifestRequestContext<'_>,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<PendingManifestCase, ContractError> {
        if context.original.id() != self.original
            || context.schedule != self.schedule
            || context.max_manifest_bytes != self.max_manifest_bytes
            || context.manifest_limits != self.manifest_limits
        {
            return Err(ContractError::Scope);
        }
        if capacity
            .transmitted_bytes
            .checked_sub(context.manifest_body_bytes_reserved)
            .is_none_or(|remaining| remaining < self.manifest_payload_bytes)
        {
            return Err(ContractError::Budget);
        }
        require_accepted_after_v1(self.carrier.origin, context.origin)?;
        request.verify_for_inclusion(context, key, signature)
    }

    /// Validate membership against the original's reservation. The caller
    /// supplies the beacon and ordered member list from canonical chain history.
    pub fn prepare_draw(
        &self,
        beacon: Digest,
        draw_point: crate::palw_state_v2::PalwBlockContextV2,
        members: &[PublicDrawMemberV1],
        relation_plan: Digest,
    ) -> Result<PreparedPublicDrawV1, ContractError> {
        if draw_point.daa_score <= self.carrier.origin.accepted_daa
            || draw_point.daa_score > self.draw_ready_by
            || draw_point.blue_score <= self.carrier.origin.accepted_blue_score
        {
            return Err(ContractError::Range);
        }
        if members.is_empty() || members.len() as u64 > self.rescue.max_drawn_bonds {
            return Err(ContractError::Budget);
        }
        let mut checked = std::collections::BTreeMap::new();
        let mut reservations = std::collections::BTreeSet::new();
        for &member in members {
            if !reservations.insert(member.rescue_reservation)
                || checked.insert((member.bond.transaction, member.bond.index), member).is_some()
            {
                return Err(ContractError::Identity);
            }
        }
        let full_scope = self.original.full_replay_scope(relation_plan);
        let ordered_members = public_draw_members_digest_v1(members);
        let draw = public_draw_id_v1(
            self.original,
            self.carrier.digest,
            Digest(draw_point.block.as_bytes()),
            beacon,
            full_scope,
            ordered_members,
        );
        Ok(PreparedPublicDrawV1 {
            original: self.original,
            admission_carrier: self.carrier,
            draw,
            beacon,
            draw_point,
            hard: self.schedule.hard(),
            full_scope,
            members: checked,
            ordered_members,
            per_bond: self.rescue.per_drawn_bond,
        })
    }

    /// Qualify this checked original for storage with the reservation resolved
    /// by the native acceptance owner. The identifier does not book capacity.
    pub fn prepare_storage_record(&self, reservation: Digest) -> PreparedPublicOriginalRecordV1 {
        PreparedPublicOriginalRecordV1(self.record(reservation))
    }

    pub fn record(&self, reservation: Digest) -> PublicOriginalAdmissionRecordV1 {
        PublicOriginalAdmissionRecordV1 {
            original: self.original,
            carrier: self.carrier,
            manifest_material: self.manifest_material,
            reservation,
            initial_supply: self.initial_supply,
            rescue: self.rescue,
            budget: self.budget,
            max_manifest_bytes: self.max_manifest_bytes,
            manifest_payload_bytes: self.manifest_payload_bytes,
            manifest_limits: self.manifest_limits,
            deadlines: [
                self.draw_ready_by,
                self.schedule.first_notice_by(),
                self.schedule.first_material_request_by(),
                self.schedule.material_ready_by(),
                self.schedule.inspection_by(),
                self.schedule.localized_by(),
                self.schedule.filed_by(),
                self.schedule.terminal_by(),
            ],
        }
    }
}

impl PublicOriginalAdmissionRecordV1 {
    #[cfg(test)]
    pub(crate) fn storage_fixture(original: OriginalId, origin: AcceptedOrigin) -> Self {
        let supply = RemainingCapacity { transmitted_bytes: 4096, additional_work: 16, retained_bytes: 4096 };
        let draw = origin.accepted_daa + 1;
        let notice = OriginalClock::new(origin.accepted_daa).unwrap().notice_until();
        Self {
            original,
            carrier: AcceptedMaterial { origin, digest: Digest([1; 64]) },
            manifest_material: MaterialId(Digest([4; 64])),
            reservation: Digest([2; 64]),
            initial_supply: supply,
            rescue: ProtectedRescueDemand {
                max_drawn_bonds: 1,
                per_drawn_bond: supply,
                max_outsiders: 1,
                per_outsider: supply,
                defensive: supply,
            },
            budget: AggregateBudget {
                rounds: 1,
                unique_source_bytes: supply.transmitted_bytes,
                transmitted_bytes: supply.transmitted_bytes * 4,
                additional_work: supply.additional_work * 4,
                retained_bytes: supply.retained_bytes * 4,
                retention_until: OriginalClock::new(origin.accepted_daa).unwrap().hard(),
                fee: 1,
                witness_attempts: 1,
            },
            max_manifest_bytes: ManifestV1::WIRE_BYTES,
            manifest_payload_bytes: MaterialScope::COUNT as u64,
            manifest_limits: Digest([3; 64]),
            deadlines: [draw, draw + 1, draw + 1, notice + 1, notice + 2, notice + 3, notice + 4, notice + 5],
        }
    }

    #[cfg(test)]
    pub(crate) fn storage_fixture_with_manifest(
        original: &OriginalCommit,
        origin: AcceptedOrigin,
        manifest_limits: Digest,
    ) -> (Self, PreparedPublicOriginalV1) {
        let mut row = Self::storage_fixture(original.id(), origin);
        row.manifest_limits = manifest_limits;
        row.manifest_material = original.manifest_material_id();
        // This fixture reserves two independent protected opportunities.
        row.rescue.max_drawn_bonds = 2;
        let capacity = row.initial_supply.checked_add(row.rescue.total().unwrap()).unwrap();
        row.budget.transmitted_bytes = capacity.transmitted_bytes;
        row.budget.additional_work = capacity.additional_work;
        row.budget.retained_bytes = capacity.retained_bytes;
        let draw_by = row.deadlines[0];
        let schedule = VerificationSchedule::new(
            OriginalClock::new(origin.accepted_daa).unwrap(),
            FirstNoticeDemand {
                draw_ready_by: draw_by,
                material_ready_by: draw_by,
                cold_acquisition_daa: 1,
                full_inspection_daa: 1,
                notice_inclusion_daa: 1,
                missing_material_detected_by: draw_by,
                material_request_inclusion_daa: 1,
            },
            MaterialCaseDemand {
                material_delivery_daa: 1,
                full_inspection_daa: 1,
                localization_daa: 1,
                filing_daa: 1,
                adjudication_and_recovery_daa: 1,
            },
        )
        .unwrap();
        let prepared = PreparedPublicOriginalV1 {
            original: row.original,
            schedule,
            capacity,
            carrier: row.carrier,
            manifest_material: row.manifest_material,
            draw_ready_by: draw_by,
            material_request_ready_by: draw_by,
            initial_supply: row.initial_supply,
            rescue: row.rescue,
            budget: row.budget,
            max_manifest_bytes: row.max_manifest_bytes,
            manifest_payload_bytes: row.manifest_payload_bytes,
            manifest_limits: row.manifest_limits,
        };
        (prepared.record(row.reservation), prepared)
    }

    #[cfg(test)]
    pub(crate) fn storage_admission_fixture(&self) -> PreparedPublicOriginalRecordV1 {
        PreparedPublicOriginalRecordV1(*self)
    }

    pub(crate) fn decode_row(key: &[u8], bytes: &[u8]) -> Result<Self, ContractError> {
        let row: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        if key != row.original.0.0.as_slice() {
            return Err(ContractError::Identity);
        }
        let capacity = public_original_capacity_v1(
            row.max_manifest_bytes,
            row.manifest_payload_bytes,
            row.budget.unique_source_bytes,
            row.initial_supply,
            row.rescue,
        )?;
        let clock = OriginalClock::new(row.carrier.origin.accepted_daa)?;
        let [draw, first_notice, first_request, ready, inspection, localized, filed, terminal] = row.deadlines;
        if draw <= row.carrier.origin.accepted_daa
            || draw > first_notice.min(first_request)
            || first_notice.max(first_request) > clock.notice_until()
            || ready < clock.notice_until()
            || [ready, inspection, localized, filed, terminal].windows(2).any(|pair| pair[0] > pair[1])
            || terminal > clock.hard()
        {
            return Err(ContractError::Range);
        }
        if row.budget.retention_until < clock.hard() {
            return Err(ContractError::Budget);
        }
        capacity.check(&AggregateUsage::default(), &row.budget)?;
        Ok(row)
    }

    pub(crate) fn check_state_point(&self, point: &crate::palw_state_v2::PalwBlockContextV2) -> Result<(), ContractError> {
        check_public_origin_point_v1(self.carrier.origin, point)
    }

    /// Resolve this reference from canonical accepted history on restore.
    pub fn carrier(&self) -> AcceptedMaterial {
        self.carrier
    }

    /// Match against admission re-derived at the original DAA and against the
    /// reservation from native state. A stored record alone grants no rights.
    pub fn check_restore(&self, prepared: &PreparedPublicOriginalV1, reservation: Digest) -> Result<(), ContractError> {
        if *self != prepared.record(reservation) {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    /// Join a computed verdict to a restored admission before the native fold
    /// applies its case and economic rules. The original schedule bounds
    /// inclusion even after the reservation is restored.
    pub fn court_verdict_key(
        &self,
        prepared: &PreparedPublicOriginalV1,
        reservation: Digest,
        inclusion_daa: u64,
        verdict: &crate::palw_tir_court_v1::PublicOriginalCourtVerdictV1,
    ) -> Result<(OriginalId, Hash64), ContractError> {
        self.check_restore(prepared, reservation)?;
        let key = verdict.key();
        if key.0 != self.original {
            return Err(ContractError::Scope);
        }
        if inclusion_daa < self.carrier.origin.accepted_daa || inclusion_daa > prepared.schedule.terminal_by() {
            return Err(ContractError::Range);
        }
        Ok(key)
    }

    /// Seal a computed verdict for the native fold. Inclusion and the
    /// original's reservation are checked again against the executing state.
    pub fn prepare_court_storage_record(
        &self,
        prepared: &PreparedPublicOriginalV1,
        reservation: Digest,
        origin: AcceptedOrigin,
        verdict: &crate::palw_tir_court_v1::PublicOriginalCourtVerdictV1,
    ) -> Result<PreparedPublicCourtRecordV1, ContractError> {
        self.court_record(prepared, reservation, origin, verdict).map(PreparedPublicCourtRecordV1)
    }

    #[cfg(test)]
    pub(crate) fn storage_court_fixture(&self, evidence_id: Hash64, origin: AcceptedOrigin) -> PreparedPublicCourtRecordV1 {
        PreparedPublicCourtRecordV1(PublicOriginalCourtRecordV1 {
            original: self.original,
            admission_carrier: self.carrier,
            reservation: self.reservation,
            evidence_id,
            origin,
        })
    }

    pub fn court_record(
        &self,
        prepared: &PreparedPublicOriginalV1,
        reservation: Digest,
        origin: AcceptedOrigin,
        verdict: &crate::palw_tir_court_v1::PublicOriginalCourtVerdictV1,
    ) -> Result<PublicOriginalCourtRecordV1, ContractError> {
        let (original, evidence_id) = self.court_verdict_key(prepared, reservation, origin.accepted_daa, verdict)?;
        require_accepted_after_v1(self.carrier.origin, origin)?;
        Ok(PublicOriginalCourtRecordV1 { original, admission_carrier: self.carrier, reservation, evidence_id, origin })
    }
}

fn check_public_origin_after_draw_v1(
    origin: AcceptedOrigin,
    draw: &crate::palw_state_v2::PalwBlockContextV2,
) -> Result<(), ContractError> {
    if origin.accepted_blue_score < draw.blue_score
        || (origin.accepted_blue_score == draw.blue_score
            && (origin.accepting_block.0 != draw.block.as_bytes() || origin.accepted_daa != draw.daa_score))
    {
        return Err(ContractError::Scope);
    }
    Ok(())
}

pub(crate) fn require_current_public_origin_v1(
    origin: AcceptedOrigin,
    point: &crate::palw_state_v2::PalwBlockContextV2,
) -> Result<(), ContractError> {
    if origin.accepting_block.0 != point.block.as_bytes()
        || origin.accepted_daa != point.daa_score
        || origin.accepted_blue_score != point.blue_score
    {
        return Err(ContractError::Scope);
    }
    Ok(())
}

pub(crate) fn check_public_origin_point_v1(
    origin: AcceptedOrigin,
    point: &crate::palw_state_v2::PalwBlockContextV2,
) -> Result<(), ContractError> {
    if origin.accepted_daa > point.daa_score
        || origin.accepted_blue_score > point.blue_score
        || (origin.accepted_blue_score == point.blue_score
            && (origin.accepting_block.0 != point.block.as_bytes() || origin.accepted_daa != point.daa_score))
    {
        return Err(ContractError::Scope);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicReplayApprovalLookupErrorV1 {
    Contract(ContractError),
    CarrierUnavailable(AcceptedOrigin),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicManifestResponseLookupErrorV1 {
    Contract(ContractError),
    CarrierUnavailable(AcceptedOrigin),
}

/// Outcome of the persisted public-verification checks before Final.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicFinalReplayPrerequisiteV1 {
    RecordedRefutation,
    NoticeOpen,
    PastHardDeadline,
    ManifestCasePending,
    MissingReplayApproval,
    ReadyForRemainingChecks,
}

impl From<ContractError> for PublicReplayApprovalLookupErrorV1 {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

/// Cross-row identity check for the dedicated writer's indivisible journal.
/// This authenticates neither an arbitrary delta nor a reservation; normal
/// native delta/root validation and admission remain mandatory.
fn check_tfp1_admission_journal_v1(
    entries: &[crate::palw_state_v2::PalwDeltaEntryV2],
    first: usize,
    preceding: Option<&crate::palw_state_v2::PalwDeltaEntryV2>,
) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
    use crate::palw_state_v2::{PalwDeltaEntryV2, PalwStateV2Error};
    let mismatch = || PalwStateV2Error::DeltaMismatch("TFP1 admission/link/history are not one insertion unit");
    let read = |index: usize, expected: u8| match entries.get(index) {
        Some(PalwDeltaEntryV2::KernelRouteRow { table, key, old: None, new: Some(bytes) }) if *table == expected => {
            Ok((key.as_slice(), bytes.as_slice()))
        }
        _ => Err(mismatch()),
    };
    let (key, bytes) = read(0, PUBLIC_ORIGINAL_ADMISSION_TABLE_V1)?;
    let admission = PublicOriginalAdmissionRecordV1::decode_row(key, bytes).map_err(|_| mismatch())?;
    let (key, bytes) = read(1, PUBLIC_TFP1_LINK_TABLE_V1)?;
    let link = tfp1_original::native_link::Tfp1PublicOriginalLinkV1::decode_row(key, bytes).map_err(|_| mismatch())?;
    let (key, bytes) = read(2, PUBLIC_TFP1_HISTORY_TABLE_V1)?;
    let history = Tfp1OriginalHistoryBoundaryV1::decode_row(key, bytes).map_err(|_| mismatch())?;
    if link.uses_public_frame_v1() {
        use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
        let Some(PalwDeltaEntryV2::StagedCourtSeed {
            key: Key::PublicTfp1FrameV1(claim),
            old: None,
            new: Some(Row::PublicTfp1FrameV1(frame)),
        }) = preceding
        else {
            return Err(mismatch());
        };
        if *claim != link.claim_v1() || frame.material_until_daa_v1() != admission.budget.retention_until {
            return Err(mismatch());
        }
        link.check_public_frame_record_v1(frame).map_err(|_| mismatch())?;
    } else if matches!(
        preceding,
        Some(PalwDeltaEntryV2::StagedCourtSeed {
            key: crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::PublicTfp1FrameV1(_),
            ..
        })
    ) {
        return Err(mismatch());
    }
    let before_first_write = if link.uses_public_frame_v1() { first.checked_sub(1).ok_or_else(mismatch)? } else { first };
    if admission.original != link.original_v1()
        || admission.original != history.original
        || admission.carrier != link.carrier_v1()
        || admission.carrier.origin != history.origin
        || admission.reservation != link.reservation_v1()
        || link.claim_v1() != history.claim
        || link.source_v1() != history.source
        || usize::try_from(history.before_delta_end).ok().is_none_or(|end| end > before_first_write)
    {
        return Err(mismatch());
    }
    Ok(())
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// Read an archived court identity. Restore still reruns the local adjudicator.
    pub fn public_original_court_record_v1(
        &self,
        original: OriginalId,
        evidence: Hash64,
    ) -> Result<Option<PublicOriginalCourtRecordV1>, ContractError> {
        let Some(route) = self.kernel_route_state_v1() else { return Ok(None) };
        let key = (PUBLIC_COURT_RECORD_TABLE_V1, PublicOriginalCourtRecordV1::key(original, evidence).to_vec());
        route.aux.get(&key).map(|bytes| self.decode_public_court_row_v1(&key.1, bytes)).transpose()
    }

    fn decode_public_court_row_v1(&self, key: &[u8], bytes: &[u8]) -> Result<PublicOriginalCourtRecordV1, ContractError> {
        let row = PublicOriginalCourtRecordV1::decode_row(key, bytes)?;
        self.check_public_court_record_v1(&row)?;
        Ok(row)
    }

    fn check_public_court_record_v1(&self, row: &PublicOriginalCourtRecordV1) -> Result<(), ContractError> {
        check_public_origin_point_v1(row.origin, self.last_point().ok_or(ContractError::Scope)?)?;
        let admission = self.public_original_admission_record_v1(row.original)?.ok_or(ContractError::Scope)?;
        require_accepted_after_v1(admission.carrier.origin, row.origin)?;
        if row.admission_carrier != admission.carrier
            || row.reservation != admission.reservation
            || row.origin.accepted_daa > admission.deadlines[7]
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    fn validate_public_verification_row_v1(
        &self,
        table: u8,
        key: &[u8],
        bytes: &[u8],
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error;
        let result = match table {
            PUBLIC_ORIGINAL_ADMISSION_TABLE_V1 => PublicOriginalAdmissionRecordV1::decode_row(key, bytes)
                .and_then(|row| row.check_state_point(self.last_point().ok_or(ContractError::Scope)?)),
            PUBLIC_PENDING_MANIFEST_TABLE_V1 => self.decode_public_pending_manifest_row_v1(key, bytes).map(|_| ()),
            PUBLIC_MANIFEST_RESPONSE_TABLE_V1 => self.decode_public_manifest_response_row_v1(key, bytes).map(|_| ()),
            PUBLIC_DRAW_TABLE_V1 => self.decode_public_draw_row_v1(key, bytes).map(|_| ()),
            PUBLIC_REPLAY_APPROVAL_TABLE_V1 => self.decode_public_replay_approval_row_v1(key, bytes).map(|_| ()),
            PUBLIC_COURT_RECORD_TABLE_V1 => self.decode_public_court_row_v1(key, bytes).map(|_| ()),
            PUBLIC_TFP1_LINK_TABLE_V1 => {
                tfp1_original::native_link::Tfp1PublicOriginalLinkV1::decode_row(key, bytes).and_then(|row| {
                    row.check_state_at_v1(self, self.last_point().ok_or(ContractError::Scope)?)?;
                    // Final-state validation (not the intermediate two-row
                    // writer): a TFO3 link must have its immutable replay row.
                    crate::palw_tfp1_original_history_v1::rooted_tfp1_original_history_v1(self, row.claim_v1())?
                        .ok_or(ContractError::Scope)?;
                    Ok(())
                })
            }
            PUBLIC_MANIFEST_PAID_TABLE_V1 => tfp1_original::manifest_paid::PaidManifestRequestRowV1::decode_row(key, bytes)
                .and_then(|row| row.check_state_at_v1(self, self.last_point().ok_or(ContractError::Scope)?)),
            PUBLIC_TFP1_HISTORY_TABLE_V1 => Tfp1OriginalHistoryBoundaryV1::decode_row(key, bytes)
                .and_then(|row| row.check_state_at_v1(self, self.last_point().ok_or(ContractError::Scope)?)),
            _ => Err(ContractError::NonCanonical),
        };
        result.map_err(|error| {
            PalwStateV2Error::CarriageInconsistent(format!("inconsistent public verification row in table {table}: {error:?}"))
        })
    }

    fn validate_public_tfp1_frame_link_v1(&self, claim: Hash64) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error;
        if self
            .public_tfp1_frame_v1(claim)
            .map_err(|_| PalwStateV2Error::CarriageInconsistent("invalid public TFP1 frame".into()))?
            .is_none()
        {
            return Ok(());
        }
        let link = self
            .public_tfp1_original_link_v1(claim)
            .map_err(|_| PalwStateV2Error::CarriageInconsistent("invalid public TFP1 frame link".into()))?;
        if !link.is_some_and(|link| link.uses_public_frame_v1()) {
            return Err(PalwStateV2Error::CarriageInconsistent("orphan public TFP1 frame".into()));
        }
        Ok(())
    }

    fn validate_public_tfp1_frames_v1(&self) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        // A frame without the paired link/history is not a valid final
        // state, including snapshots that omit the entire kernel route.
        for (claim, _) in self.public_tfp1_frame_rows_v1() {
            self.validate_public_tfp1_frame_link_v1(claim)?;
        }
        Ok(())
    }

    pub(crate) fn validate_public_verification_rows_v1(&self) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        self.validate_public_tfp1_frames_v1()?;
        let Some(route) = self.kernel_route_state_v1() else { return Ok(()) };
        for ((table, key), bytes) in
            route.aux.range((PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, Vec::new())..(PUBLIC_TFP1_HISTORY_TABLE_V1 + 1, Vec::new()))
        {
            self.validate_public_verification_row_v1(*table, key, bytes)?;
        }
        Ok(())
    }

    pub(crate) fn validate_public_verification_delta_v1(
        &self,
        entries: &[crate::palw_state_v2::PalwDeltaEntryV2],
        revert: bool,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::{PalwDeltaEntryV2, PalwStateV2Error};
        use tfp1_original::manifest_paid::PaidManifestRequestRowV1;
        let route = self.kernel_route_state_v1();
        let mut check_all = false;
        let mut check_frames = false;
        for (index, entry) in entries.iter().enumerate() {
            use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
            match entry {
                PalwDeltaEntryV2::Claim { key, .. } => self.validate_public_tfp1_frame_link_v1(*key)?,
                // Class changes can affect many frames. Validate the dedicated
                // frame range once per delta, not once per class or per claim.
                PalwDeltaEntryV2::TirClass { old, new, .. } if old != new => check_frames = true,
                _ => {}
            }
            if let PalwDeltaEntryV2::StagedCourtSeed { key, old, new } = entry {
                if let Key::TirClaim(claim) = key {
                    self.validate_public_tfp1_frame_link_v1(*claim)?;
                }
                if matches!(key, Key::PublicTfp1FrameV1(_))
                    || matches!(old, Some(Row::PublicTfp1FrameV1(_)))
                    || matches!(new, Some(Row::PublicTfp1FrameV1(_)))
                {
                    // New immutable frames have no standalone update/delete.
                    // The forward journal is one frame + 198/204/206 unit;
                    // rollback validates that same unit before removing it.
                    let first = index.checked_add(1).ok_or(PalwStateV2Error::DeltaMismatch("TFP1 frame journal index"))?;
                    check_tfp1_admission_journal_v1(
                        entries
                            .get(first..)
                            .and_then(|tail| tail.get(..3))
                            .ok_or(PalwStateV2Error::DeltaMismatch("incomplete TFP1 frame journal"))?,
                        first,
                        Some(entry),
                    )?;
                    let (Key::PublicTfp1FrameV1(claim), None, Some(Row::PublicTfp1FrameV1(frame))) = (key, old, new) else {
                        return Err(PalwStateV2Error::DeltaMismatch("public TFP1 frame journal is insert-only"));
                    };
                    frame.check_structure_v1(*claim).map_err(PalwStateV2Error::CarriageInconsistent)?;
                    if !revert {
                        let link = self
                            .public_tfp1_original_link_v1(*claim)
                            .map_err(|_| PalwStateV2Error::DeltaMismatch("public TFP1 frame/link mismatch"))?
                            .ok_or(PalwStateV2Error::DeltaMismatch("public TFP1 frame has no link"))?;
                        if !link.uses_public_frame_v1() {
                            return Err(PalwStateV2Error::DeltaMismatch("public TFP1 frame has a legacy link"));
                        }
                    } else {
                        check_all = true;
                    }
                }
                continue;
            }
            let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry else { continue };
            if !is_public_verification_table_v1(*table) {
                continue;
            }
            if matches!(*table, PUBLIC_TFP1_LINK_TABLE_V1 | PUBLIC_TFP1_HISTORY_TABLE_V1) {
                // The private writer emits one adjacent 198/204/206 unit.
                // Check the journal in both directions: after undo, an orphan
                // 198 alone is otherwise indistinguishable from a legacy row.
                let offset = if *table == PUBLIC_TFP1_LINK_TABLE_V1 { 1 } else { 2 };
                let first = index.checked_sub(offset).ok_or(PalwStateV2Error::DeltaMismatch("incomplete TFP1 admission journal"))?;
                check_tfp1_admission_journal_v1(
                    entries
                        .get(first..)
                        .and_then(|tail| tail.get(..3))
                        .ok_or(PalwStateV2Error::DeltaMismatch("incomplete TFP1 admission journal"))?,
                    first,
                    first.checked_sub(1).and_then(|index| entries.get(index)),
                )?;
            }
            if *table == PUBLIC_MANIFEST_PAID_TABLE_V1 && !revert {
                let bytes = new.as_deref().ok_or(PalwStateV2Error::DeltaMismatch("paid ManifestRequest journal is insert-only"))?;
                let point =
                    self.last_point().ok_or(PalwStateV2Error::DeltaMismatch("paid ManifestRequest insert has no accepting point"))?;
                PaidManifestRequestRowV1::decode_row(key, bytes)
                    .and_then(|row| row.check_insert_at_v1(point))
                    .map_err(|_| PalwStateV2Error::DeltaMismatch("paid ManifestRequest insert has another accepting point"))?;
            }
            if *table == PUBLIC_TFP1_HISTORY_TABLE_V1 && !revert {
                let bytes = new.as_deref().ok_or(PalwStateV2Error::DeltaMismatch("TFP1 history journal is insert-only"))?;
                let point = self.last_point().ok_or(PalwStateV2Error::DeltaMismatch("TFP1 history insert has no point"))?;
                Tfp1OriginalHistoryBoundaryV1::decode_row(key, bytes)
                    .and_then(|row| row.check_insert_at_v1(point))
                    .map_err(|_| PalwStateV2Error::DeltaMismatch("TFP1 history insert has another accepting point"))?;
            }
            if (if revert { old } else { new }).is_none() {
                // A removed row could leave surviving records without a parent.
                check_all = true;
            }
            // Reverting the creation of the route can remove the header too.
            // The insertion journal must still pass the structural checks above.
            if !check_all && let Some(bytes) = route.and_then(|route| route.aux.get(&(*table, key.clone()))) {
                self.validate_public_verification_row_v1(*table, key, bytes)?;
            }
        }
        if check_all {
            self.validate_public_verification_rows_v1()
        } else if check_frames {
            self.validate_public_tfp1_frames_v1()
        } else {
            Ok(())
        }
    }

    /// Check the public verification rows at this exact native state point.
    /// Local positive checks, inspection opportunity, dependencies, funding,
    /// and settlement are separate and still required for Final.
    pub fn public_final_replay_prerequisite_v1(
        &self,
        draw: &PreparedPublicDrawV1,
        approval_until: u64,
        read_carrier: impl FnMut(
            AcceptedOrigin,
        )
            -> Option<[u8; misaka_palw_positive_contract::public_one_honest::SignedFullReplayApprovalV1::WIRE_BYTES]>,
    ) -> Result<PublicFinalReplayPrerequisiteV1, PublicReplayApprovalLookupErrorV1> {
        use PublicFinalReplayPrerequisiteV1::*;
        let original = draw.original;
        let admission = self.public_original_admission_record_v1(original)?.ok_or(ContractError::Scope)?;
        if self.public_draw_record_v1(original)? != Some(draw.record()) {
            return Err(ContractError::Scope.into());
        }
        let route = self.kernel_route_state_v1().ok_or(ContractError::Scope)?;
        let key = (PUBLIC_COURT_RECORD_TABLE_V1, original.0.0.to_vec());
        if let Some(((table, key), bytes)) = route.aux.range(key..).next()
            && *table == PUBLIC_COURT_RECORD_TABLE_V1
            && key.starts_with(&original.0.0)
        {
            self.decode_public_court_row_v1(key, bytes)?;
            return Ok(RecordedRefutation);
        }
        let clock = OriginalClock::new(admission.carrier.origin.accepted_daa)?;
        let now = self.last_point().ok_or(ContractError::Scope)?.daa_score;
        if clock.is_past_hard(now) {
            return Ok(PastHardDeadline);
        }
        if !clock.permits_final_time(now) {
            return Ok(NoticeOpen);
        }
        if self.has_public_pending_manifest_v1(original)? {
            return Ok(ManifestCasePending);
        }
        if self.has_reauthenticated_public_replay_approval_v1(draw, approval_until, read_carrier)? {
            Ok(ReadyForRemainingChecks)
        } else {
            Ok(MissingReplayApproval)
        }
    }

    /// Resolve the profile's one-approval condition from rooted state. The
    /// reader supplies the literal accepted carrier at the stored origin.
    /// Missing history does not count as an absent vote.
    pub fn has_reauthenticated_public_replay_approval_v1(
        &self,
        draw: &PreparedPublicDrawV1,
        approval_until: u64,
        mut read_carrier: impl FnMut(
            AcceptedOrigin,
        ) -> Option<
            [u8; misaka_palw_positive_contract::public_one_honest::SignedFullReplayApprovalV1::WIRE_BYTES],
        >,
    ) -> Result<bool, PublicReplayApprovalLookupErrorV1> {
        if self.public_draw_record_v1(draw.original)? != Some(draw.record()) {
            return Err(ContractError::Scope.into());
        }
        if approval_until > draw.hard || approval_until < draw.draw_point.daa_score {
            return Err(ContractError::Range.into());
        }
        let mut unavailable = None;
        for member in draw.members.values() {
            let Some(record) = self.public_replay_approval_record_v1(draw.original, member.bond)? else {
                continue;
            };
            let Some(bytes) = read_carrier(record.origin) else {
                unavailable.get_or_insert(record.origin);
                continue;
            };
            if record != draw.verify_replay_approval_bytes(approval_until, record.origin, &bytes)? {
                return Err(ContractError::Scope.into());
            }
            return Ok(true);
        }
        match unavailable {
            Some(origin) => Err(PublicReplayApprovalLookupErrorV1::CarrierUnavailable(origin)),
            None => Ok(false),
        }
    }

    /// Read the stored approval and check its draw link. Before tallying,
    /// reauthenticate the accepted carrier and its membership with `check_restore`.
    pub fn public_replay_approval_record_v1(
        &self,
        original: OriginalId,
        bond: PanelBond,
    ) -> Result<Option<PublicReplayApprovalRecordV1>, ContractError> {
        let Some(route) = self.kernel_route_state_v1() else { return Ok(None) };
        let key = (PUBLIC_REPLAY_APPROVAL_TABLE_V1, PublicReplayApprovalRecordV1::key(original, bond).to_vec());
        route.aux.get(&key).map(|bytes| self.decode_public_replay_approval_row_v1(&key.1, bytes)).transpose()
    }

    pub(crate) fn decode_public_replay_approval_row_v1(
        &self,
        key: &[u8],
        bytes: &[u8],
    ) -> Result<PublicReplayApprovalRecordV1, ContractError> {
        let row = PublicReplayApprovalRecordV1::decode_row(key, bytes)?;
        check_public_origin_point_v1(row.origin, self.last_point().ok_or(ContractError::Scope)?)?;
        let draw = self.public_draw_record_v1(row.original)?.ok_or(ContractError::Scope)?;
        check_public_origin_after_draw_v1(row.origin, &draw.draw_point)?;
        if row.draw != draw.draw
            || row.full_scope != draw.full_scope
            || row.ordered_members != draw.ordered_members
            || !(draw.draw_point.daa_score..=draw.hard).contains(&row.origin.accepted_daa)
        {
            return Err(ContractError::Scope);
        }
        Ok(row)
    }

    /// Read the recorded draw. Membership is still re-derived from native history.
    pub fn public_draw_record_v1(&self, original: OriginalId) -> Result<Option<PublicDrawRecordV1>, ContractError> {
        let Some(route) = self.kernel_route_state_v1() else { return Ok(None) };
        let key = (PUBLIC_DRAW_TABLE_V1, original.0.0.to_vec());
        route.aux.get(&key).map(|bytes| self.decode_public_draw_row_v1(&key.1, bytes)).transpose()
    }

    pub(crate) fn decode_public_draw_row_v1(&self, key: &[u8], bytes: &[u8]) -> Result<PublicDrawRecordV1, ContractError> {
        let row = PublicDrawRecordV1::decode_row(key, bytes)?;
        self.check_public_draw_record_v1(&row)?;
        Ok(row)
    }

    fn check_public_draw_record_v1(&self, row: &PublicDrawRecordV1) -> Result<(), ContractError> {
        let admission = self.public_original_admission_record_v1(row.original)?.ok_or(ContractError::Scope)?;
        let point = self.last_point().ok_or(ContractError::Scope)?;
        let clock = OriginalClock::new(admission.carrier.origin.accepted_daa)?;
        if row.admission_carrier != admission.carrier
            || row.draw_point.daa_score <= admission.carrier.origin.accepted_daa
            || row.draw_point.daa_score > admission.deadlines[0].min(point.daa_score)
            || row.draw_point.blue_score <= admission.carrier.origin.accepted_blue_score
            || row.draw_point.blue_score > point.blue_score
            || (row.draw_point.blue_score == point.blue_score && row.draw_point != *point)
            || row.hard != clock.hard()
            || row.per_bond != admission.rescue.per_drawn_bond
            || row.draw
                != public_draw_id_v1(
                    row.original,
                    row.admission_carrier.digest,
                    Digest(row.draw_point.block.as_bytes()),
                    row.beacon,
                    row.full_scope,
                    row.ordered_members,
                )
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    pub fn public_manifest_response_record_v1(
        &self,
        key: &[u8; ManifestResponseRecordV1::KEY_BYTES],
    ) -> Result<Option<ManifestResponseRecordV1>, ContractError> {
        let Some(route) = self.kernel_route_state_v1() else { return Ok(None) };
        let key = (PUBLIC_MANIFEST_RESPONSE_TABLE_V1, key.to_vec());
        route.aux.get(&key).map(|bytes| self.decode_public_manifest_response_row_v1(&key.1, bytes)).transpose()
    }

    /// Recover a delivery at its exact accepted carrier. The caller resolves
    /// the provider and limits from the native supply obligation, and restores
    /// the pending request before calling this method.
    #[allow(clippy::too_many_arguments)]
    pub fn reauthenticate_public_manifest_response_v1(
        &self,
        key: &[u8; ManifestResponseRecordV1::KEY_BYTES],
        pending: &PendingManifestCase,
        original: &OriginalCommit,
        provider: PrincipalId,
        scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
        total_limit: u64,
        read_carrier: impl FnOnce(AcceptedOrigin) -> Option<Vec<u8>>,
    ) -> Result<Option<VerifiedManifestDeliveryV1>, PublicManifestResponseLookupErrorV1> {
        use PublicManifestResponseLookupErrorV1::{CarrierUnavailable, Contract};
        let Some(row) = self.public_manifest_response_record_v1(key).map_err(Contract)? else { return Ok(None) };
        if self.public_pending_manifest_record_v1(row.original(), row.case()).map_err(Contract)? != Some(pending.record()) {
            return Err(Contract(ContractError::Scope));
        }
        let origin = row.origin();
        let packet = read_carrier(origin).ok_or(CarrierUnavailable(origin))?;
        pending
            .restore_provider_response_bytes(&row, &packet, provider, original, origin, scope_limits, total_limit)
            .map(Some)
            .map_err(Contract)
    }

    pub(crate) fn decode_public_manifest_response_row_v1(
        &self,
        key: &[u8],
        bytes: &[u8],
    ) -> Result<ManifestResponseRecordV1, ContractError> {
        let row = ManifestResponseRecordV1::decode_row(key, bytes)?;
        check_public_origin_point_v1(row.origin(), self.last_point().ok_or(ContractError::Scope)?)?;
        let pending = self.public_pending_manifest_record_v1(row.original(), row.case())?.ok_or(ContractError::Scope)?;
        row.check_case(&pending)?;
        Ok(row)
    }

    pub fn public_pending_manifest_record_v1(
        &self,
        original: OriginalId,
        case: CaseId,
    ) -> Result<Option<PendingManifestCaseRecordV1>, ContractError> {
        let Some(route) = self.kernel_route_state_v1() else { return Ok(None) };
        let key = (PUBLIC_PENDING_MANIFEST_TABLE_V1, PendingManifestCaseRecordV1::key(original, case).to_vec());
        route.aux.get(&key).map(|bytes| self.decode_public_pending_manifest_row_v1(&key.1, bytes)).transpose()
    }

    /// Classify a still-open case at this accepted state point. Receiving a
    /// response leaves this status unchanged until the case owner resolves it.
    pub fn public_pending_manifest_outcome_v1(
        &self,
        original: OriginalId,
        case: CaseId,
    ) -> Result<Option<MissingInspectionOutcome>, ContractError> {
        let Some(row) = self.public_pending_manifest_record_v1(original, case)? else { return Ok(None) };
        row.unresolved_at(self.last_point().ok_or(ContractError::Scope)?.daa_score).map(Some)
    }

    /// Read the unresolved manifest cases for one admitted original.
    pub fn has_public_pending_manifest_v1(&self, original: OriginalId) -> Result<bool, ContractError> {
        self.public_original_admission_record_v1(original)?.ok_or(ContractError::Scope)?;
        let route = self.kernel_route_state_v1().ok_or(ContractError::Scope)?;
        let first_key = (PUBLIC_PENDING_MANIFEST_TABLE_V1, original.0.0.to_vec());
        if let Some(((table, key), value)) = route.aux.range(first_key..).next()
            && *table == PUBLIC_PENDING_MANIFEST_TABLE_V1
            && key.starts_with(&original.0.0)
        {
            self.decode_public_pending_manifest_row_v1(key, value)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn decode_public_pending_manifest_row_v1(
        &self,
        key: &[u8],
        bytes: &[u8],
    ) -> Result<PendingManifestCaseRecordV1, ContractError> {
        let row = PendingManifestCaseRecordV1::decode_row(key, bytes)?;
        self.check_public_pending_manifest_record_at_v1(&row, self.last_point().ok_or(ContractError::Scope)?)?;
        Ok(row)
    }

    fn check_public_pending_manifest_record_at_v1(
        &self,
        row: &PendingManifestCaseRecordV1,
        point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<(), ContractError> {
        let admission = self.public_original_admission_record_v1(row.original())?.ok_or(ContractError::Scope)?;
        if row.material() != admission.manifest_material {
            return Err(ContractError::Scope);
        }
        check_public_origin_point_v1(row.origin(), point)?;
        require_accepted_after_v1(admission.carrier.origin, row.origin())?;
        match row.claimant()? {
            Claimant::DrawnBond(_) => {
                let draw = self.public_draw_record_v1(row.original())?.ok_or(ContractError::Scope)?;
                if row.original().full_replay_scope(row.inspection_scope()) != draw.full_scope {
                    return Err(ContractError::Scope);
                }
                if row.origin().accepted_daa < draw.draw_point.daa_score {
                    return Err(ContractError::Range);
                }
                check_public_origin_after_draw_v1(row.origin(), &draw.draw_point)?;
            }
            Claimant::Outsider(_) if admission.rescue.max_outsiders == 0 => return Err(ContractError::Budget),
            Claimant::Outsider(_) => {}
        }
        let [_, _, _, ready, _, _, filed, _] = admission.deadlines;
        row.check_admission(
            admission.carrier.origin.accepted_daa,
            point.daa_score,
            admission.max_manifest_bytes,
            admission.manifest_limits,
            ready,
            filed,
        )?;
        Ok(())
    }

    /// Read the record from this state point. Callers must still resolve its
    /// carrier in accepted history and revalidate it with the native reservation.
    pub fn public_original_admission_record_v1(
        &self,
        original: OriginalId,
    ) -> Result<Option<PublicOriginalAdmissionRecordV1>, ContractError> {
        let Some(route) = self.kernel_route_state_v1() else { return Ok(None) };
        let key = (PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, original.0.0.to_vec());
        let Some(bytes) = route.aux.get(&key) else { return Ok(None) };
        let record = PublicOriginalAdmissionRecordV1::decode_row(&key.1, bytes)?;
        record.check_state_point(self.last_point().ok_or(ContractError::Scope)?)?;
        Ok(Some(record))
    }

    /// Structurally checked immutable context. This getter deliberately does
    /// not read aux 204: paired installation writes the frame before its link.
    /// Use the link/history restore for accepted authority.
    pub(crate) fn public_tfp1_frame_v1(&self, claim: Hash64) -> Result<Option<&PublicTfp1FrameV1>, ContractError> {
        use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
        match self.staged_court_row_v1(&Key::PublicTfp1FrameV1(claim)) {
            None => Ok(None),
            Some(Row::PublicTfp1FrameV1(frame)) => {
                frame.check_structure_v1(claim).map_err(|_| ContractError::Scope)?;
                Ok(Some(frame))
            }
            _ => Err(ContractError::Scope),
        }
    }

    /// A rooted link never substitutes for restoring the literal companion,
    /// draw and request. It pins which OriginalId may be used for this claim.
    pub(crate) fn public_tfp1_original_link_v1(
        &self,
        claim: Hash64,
    ) -> Result<Option<tfp1_original::native_link::Tfp1PublicOriginalLinkV1>, ContractError> {
        let Some(route) = self.kernel_route_state_v1() else { return Ok(None) };
        let key = claim.as_bytes();
        let Some(bytes) = route.aux.get(&(PUBLIC_TFP1_LINK_TABLE_V1, key.to_vec())) else { return Ok(None) };
        let row = tfp1_original::native_link::Tfp1PublicOriginalLinkV1::decode_row(&key, bytes)?;
        row.check_state_at_v1(self, self.last_point().ok_or(ContractError::Scope)?)?;
        Ok(Some(row))
    }

    pub(crate) fn public_tfp1_link_holds_claim_v1(&self, claim: &Hash64) -> bool {
        // Until the new profile's bounded release/archive path is complete,
        // never strand a link by retiring the native identity it authenticates.
        self.kernel_route_state_v1()
            .is_some_and(|route| route.aux.contains_key(&(PUBLIC_TFP1_LINK_TABLE_V1, claim.as_bytes().to_vec())))
    }
}

impl PublicOriginalAdmissionV1<'_> {
    /// Decode the fixed-size carrier before validating admission or quoting
    /// its reservation. Native callers resolve the expected facts separately.
    pub fn prepare_signed_bytes(
        &self,
        bytes: &[u8],
        reservation: &PublicOriginalReservationV1<'_>,
    ) -> Result<PreparedPublicOriginalV1, ContractError> {
        let signed = SignedPublicOriginalV1::decode(bytes)?;
        self.prepare(&signed.original, &signed.key, &signed.signature, reservation)
    }

    /// Restore an admission only after replaying the signed carrier under the
    /// original inclusion inputs and matching the stored reservation record.
    pub fn restore_signed_bytes(
        &self,
        bytes: &[u8],
        inputs: &PublicOriginalReservationV1<'_>,
        record: &PublicOriginalAdmissionRecordV1,
        reservation: Digest,
    ) -> Result<PreparedPublicOriginalV1, ContractError> {
        let prepared = self.prepare_signed_bytes(bytes, inputs)?;
        record.check_restore(&prepared, reservation)?;
        Ok(prepared)
    }

    /// Shared unsigned/signed reservation validation; no signature or state write.
    fn check_reservation_v1(
        &self,
        original: &OriginalCommit,
        reservation: &PublicOriginalReservationV1<'_>,
    ) -> Result<(u64, RemainingCapacity, VerificationSchedule), ContractError> {
        let manifest_payload_bytes = self
            .manifest_scope_limits
            .iter()
            .fold(0u64, |total, scope| total.saturating_add(scope.max_bytes))
            .min(self.manifest_total_limit);
        let capacity = public_original_capacity_v1(
            self.max_manifest_bytes,
            manifest_payload_bytes,
            reservation.budget.unique_source_bytes,
            reservation.initial_supply,
            reservation.rescue,
        )?;
        let clock = OriginalClock::new(reservation.origin.accepted_daa)?;
        if reservation.notice.draw_ready_by <= reservation.origin.accepted_daa {
            return Err(ContractError::Range);
        }
        if reservation.budget.retention_until < clock.hard() {
            return Err(ContractError::Budget);
        }
        let schedule = VerificationSchedule::new(clock, reservation.notice, reservation.case)?;
        capacity.check(reservation.usage, reservation.budget)?;
        if original.service_contract != public_original_service_contract_id_v1(reservation.service_obligation, reservation.budget) {
            return Err(ContractError::Scope);
        }
        Ok((manifest_payload_bytes, capacity, schedule))
    }

    pub fn prepare(
        &self,
        original: &OriginalCommit,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
        reservation: &PublicOriginalReservationV1<'_>,
    ) -> Result<PreparedPublicOriginalV1, ContractError> {
        let (manifest_payload_bytes, capacity, schedule) = self.check_reservation_v1(original, reservation)?;
        let original_id = self.authenticate(original, key, signature)?;
        Ok(PreparedPublicOriginalV1 {
            original: original_id,
            schedule,
            capacity,
            carrier: AcceptedMaterial {
                origin: reservation.origin,
                digest: SignedPublicOriginalV1::carrier_digest(original, key, signature),
            },
            manifest_material: original.manifest_material_id(),
            draw_ready_by: reservation.notice.draw_ready_by,
            material_request_ready_by: reservation.notice.draw_ready_by.max(reservation.notice.missing_material_detected_by),
            initial_supply: reservation.initial_supply,
            rescue: reservation.rescue,
            budget: *reservation.budget,
            max_manifest_bytes: self.max_manifest_bytes,
            manifest_payload_bytes,
            manifest_limits: manifest_limits_id_v1(self.manifest_scope_limits, self.manifest_total_limit, self.max_manifest_bytes),
        })
    }

    /// Verify the signed original against native facts. The caller must still
    /// reserve resources and persist admission before drawing any Panel bonds.
    pub fn authenticate(
        &self,
        original: &OriginalCommit,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<OriginalId, ContractError> {
        self.check_original_metadata_v1(original)?;
        original.authenticate(key, signature)?;
        Ok(original.id())
    }

    fn check_original_metadata_v1(&self, original: &OriginalCommit) -> Result<(), ContractError> {
        ManifestV1::fetch_size(self.max_manifest_bytes)?;
        if self.manifest_total_limit < MaterialScope::COUNT as u64
            || self.manifest_scope_limits.iter().any(|scope| scope.max_bytes == 0 || scope.chunk_bytes == 0)
        {
            return Err(ContractError::Budget);
        }
        if original.scope != self.scope
            || original.miner != self.miner
            || original.prompt != self.prompt
            || original.initial_state != self.initial_state
            || original.dependencies != self.dependencies
            || original.service_contract != self.service_contract
            || original.manifest_root == Digest([0; 64])
        {
            return Err(ContractError::Scope);
        }
        check_public_original_execution_v1(original, self.job, self.class, self.artifact)
    }
}

#[test]
fn stored_public_admission_requires_manifest_capacity_for_initial_supply_and_rescue() {
    use misaka_palw_positive_contract::rows::NativeHash64;

    let original = OriginalId(Digest([1; 64]));
    let origin = AcceptedOrigin {
        accepting_block: NativeHash64([1; 64]),
        carrier_block: NativeHash64([2; 64]),
        carrier_transaction: NativeHash64([3; 64]),
        accepted_tx_ordinal: 0,
        event_ordinal: 0,
        accepted_daa: 1,
        accepted_blue_score: 1,
    };
    let mut row = PublicOriginalAdmissionRecordV1::storage_fixture(original, origin);
    let decode = |row: &PublicOriginalAdmissionRecordV1| {
        PublicOriginalAdmissionRecordV1::decode_row(&original.0.0, &borsh::to_vec(row).unwrap())
    };
    assert!(decode(&row).is_ok());

    let mut short_source_budget = row;
    short_source_budget.budget.unique_source_bytes = u64::from(ManifestV1::WIRE_BYTES);
    assert_eq!(decode(&short_source_budget), Err(ContractError::Budget));

    let mut short_retention = row;
    short_retention.initial_supply.retained_bytes = u64::from(ManifestV1::WIRE_BYTES);
    assert_eq!(decode(&short_retention), Err(ContractError::Budget));

    let mut short_supply = row;
    short_supply.initial_supply.transmitted_bytes = u64::from(ManifestV1::WIRE_BYTES);
    assert_eq!(decode(&short_supply), Err(ContractError::Budget));

    // Each protected allocation must cover the referenced materials even
    // when the smaller allocation still fits the aggregate budget.
    let mut short_drawn = row;
    short_drawn.rescue.per_drawn_bond.transmitted_bytes = u64::from(ManifestV1::WIRE_BYTES);
    assert_eq!(decode(&short_drawn), Err(ContractError::Budget));
    row.rescue.per_outsider.transmitted_bytes = u64::from(ManifestV1::WIRE_BYTES);
    assert_eq!(decode(&row), Err(ContractError::Budget));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterError {
    Contract(ContractError),
    Check(CheckError),
    Binding,
    Closed,
}
impl From<ContractError> for AdapterError {
    fn from(e: ContractError) -> Self {
        Self::Contract(e)
    }
}
impl From<CheckError> for AdapterError {
    fn from(e: CheckError) -> Self {
        Self::Check(e)
    }
}

/// Verified Miner commitment plus independently derived plan. No native credit
/// capability. Limits include the complete outer frame, not only G14 payloads.
pub struct PreparedCheck {
    result: MinerResultCommit,
    plan: CoveragePlan,
    descriptor: CoverageDescriptor,
    form: PalwPromptIdsFormV1,
    limits: CheckLimits,
    // Exact independently resolved metadata, retained for primitive record
    // reconstruction. These are not decoded from an evidence carrier.
    native_class: PalwTirClassV1,
    native_job: PalwJobContextV2,
}
impl PreparedCheck {
    /// Native inputs must be resolved independently of the candidate. The
    /// private TIR adapter uses job_id as `job`, context_hash as `execution`, and
    /// the existing graph root/input root/CU rules digest in their named fields.
    /// A4's rooted selector must designate the greedy rule this derivation pins.
    #[allow(clippy::too_many_arguments)]
    pub fn authenticate(
        rooted: &AdmissionContext,
        registered: &PalwTirClassV1,
        artifact: Hash64,
        job: &PalwJobContextV2,
        form: PalwPromptIdsFormV1,
        limits: CheckLimits,
        result: MinerResultCommit,
        miner_key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<Self, AdapterError> {
        rooted.check(ClaimForm::PositiveResultV2, &result)?;
        result.authenticate(miner_key, signature)?;
        if result.job != digest(job.job_id)
            || result.execution != digest(job.context_hash())
            || result.registered_class != digest(registered.class_id(&artifact))
            || result.artifact != digest(artifact)
            || result.program != digest(registered.graph_ir_root())
            || result.input != digest(job.prompt_token_ids_hash)
            || result.tokenizer != digest(job.tokenizer_id)
            || result.execution_rules != digest(job.cu_ruleset_id)
            || result.generation_seed != job.execution_seed
            || result.prefill != job.declared_prefill_tokens
            || result.decode != job.exact_decode_tokens
            || result.max_context != job.max_context_tokens
        {
            return Err(AdapterError::Binding);
        }
        if result.temperature_q24 != 0 || result.selector != greedy_selector_id() {
            return Err(CheckError::UnsupportedProfile.into());
        }
        let mut inner = limits;
        inner.max_part_bytes = raw_part_budget(limits.max_part_bytes)?;
        // Determine the canonical count, then reserve ALL outer/prefix bytes.
        let provisional = CoveragePlan::derive(registered, artifact, job, form, inner)?;
        provisional
            .leaf_count()
            .checked_mul(limits.max_part_bytes)
            .filter(|n| *n <= limits.max_total_bytes)
            .ok_or(CheckError::Budget)?;
        let overhead = provisional.leaf_count().checked_mul(PART_OVERHEAD).ok_or(CheckError::Budget)?;
        inner.max_total_bytes = limits.max_total_bytes.checked_sub(overhead).ok_or(CheckError::Budget)?;
        let plan = CoveragePlan::derive(registered, artifact, job, form, inner)?;
        let cost = plan.cost();
        let descriptor = CoverageDescriptor {
            scope: result.scope,
            result: result.id(),
            plan: coverage_plan_id(&result, CheckProfile::ExactStreamV1, exact_derivation_id(), plan.leaf_count())?,
            profile: CheckProfile::ExactStreamV1,
            mandatory_relations: plan.leaf_count(),
            reserved_elements: cost.evaluation_elements,
            reserved_terms: cost.evaluation_terms,
            max_part_bytes: limits.max_part_bytes,
        };
        Ok(Self { result, plan, descriptor, form, limits, native_class: registered.clone(), native_job: job.clone() })
    }
    pub fn descriptor(&self) -> CoverageDescriptor {
        self.descriptor
    }
    pub fn reserved_wire_bytes(&self) -> u64 {
        // Both checked during construction and bounded by max_total_bytes.
        self.plan.leaf_count() * self.limits.max_part_bytes
    }
    pub fn signed_transport_quote(&self) -> Result<SignedTransportQuote, AdapterError> {
        signed_quote(self.plan.leaf_count(), self.limits.max_part_bytes)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn authenticate_witness(
        self,
        witness: PanelWitnessCommit,
        binding: PalwTirStepBindingV1,
        generated: Vec<u32>,
        panel_key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<CheckContext, AdapterError> {
        witness.authenticate(&self.result, panel_key, signature)?;
        self.plan.validate_witness(&binding, &generated)?;
        if witness.plan != self.descriptor.plan
            || witness.provenance != provenance_id(&self.result)
            || witness.execution_record != digest(binding.committed_execution_root)
            || witness.candidate_output != digest(palw_attempt_output_root_v1(&binding.job_context, &generated))
        {
            return Err(AdapterError::Binding);
        }
        let identity = ContextIdentity {
            descriptor: self.descriptor,
            witness: witness.id(),
            binding,
            generated,
            expected_output: self.result.output,
            limits: self.limits,
            form: self.form,
            seed: self.result.generation_seed,
        };
        Ok(CheckContext { plan: self.plan, witness, identity })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ContextIdentity {
    descriptor: CoverageDescriptor,
    witness: WitnessId,
    binding: PalwTirStepBindingV1,
    generated: Vec<u32>,
    expected_output: Digest,
    limits: CheckLimits,
    form: PalwPromptIdsFormV1,
    seed: [u8; 32],
}
pub struct CheckContext {
    plan: CoveragePlan,
    witness: PanelWitnessCommit,
    identity: ContextIdentity,
}
impl CheckContext {
    /// Compare the complete opaque bindings and limits for an idempotent join.
    /// Native ingress must reserve this bounded comparison's work.
    pub fn same_identity(&self, other: &Self) -> bool {
        self.identity == other.identity
    }

    /// Identity of the already authenticated candidate, for the Court adapter.
    /// This accessor grants no native admission or computation authority.
    pub fn witness_id(&self) -> WitnessId {
        self.witness.id()
    }
    pub fn signed_transport_quote(&self) -> Result<SignedTransportQuote, AdapterError> {
        signed_quote(self.identity.descriptor.mandatory_relations, self.identity.limits.max_part_bytes)
    }
    pub fn start(&self) -> Result<ContractProgress, AdapterError> {
        Ok(ContractProgress {
            identity: self.identity.clone(),
            wire_bytes: 0,
            signature_bytes: 0,
            failure: None,
            checker: ExactChecker::begin_greedy(
                self.plan.clone(),
                self.identity.binding.clone(),
                self.identity.generated.clone(),
                Hash64::from_bytes(self.identity.expected_output.0),
                PalwDecodeSamplingV2 { seed: self.identity.seed, temperature_q: 0 },
            )?,
        })
    }
}
/// No Deserialize or public cursor setter. Recovery replays the same frames.
pub struct ContractProgress {
    identity: ContextIdentity,
    checker: ExactChecker,
    wire_bytes: u64,
    signature_bytes: u64,
    failure: Option<CheckOutcome>,
}
impl ContractProgress {
    pub fn snapshot(&self) -> CheckProgress {
        let mut snapshot = self.checker.progress();
        if let Some(outcome) = self.failure {
            snapshot.outcome = outcome;
        }
        snapshot
    }
    pub fn charged_wire_bytes(&self) -> u64 {
        self.wire_bytes
    }
    pub fn charged_signature_bytes(&self) -> u64 {
        self.signature_bytes
    }
    pub fn charged_transport_bytes(&self) -> Result<u64, AdapterError> {
        self.wire_bytes.checked_add(self.signature_bytes).ok_or(CheckError::Budget.into())
    }
}
/// A checked local fact, not a Gate C/payment/native verdict authority.
pub struct CheckedTerminal {
    result: ResultId,
    witness: WitnessId,
    descriptor: CoverageDescriptor,
    outcome: CheckOutcome,
}
impl CheckedTerminal {
    pub fn result(&self) -> ResultId {
        self.result
    }
    pub fn witness(&self) -> WitnessId {
        self.witness
    }
    pub fn descriptor(&self) -> CoverageDescriptor {
        self.descriptor
    }
    pub fn outcome(&self) -> CheckOutcome {
        self.outcome
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TirNodeChecker;
impl TirNodeChecker {
    /// Failed signature/foreign scope leaves the candidate and counters intact.
    /// A4 still owns current drawn membership, admission and duty authorization.
    pub fn check_signed_part(
        &self,
        context: &CheckContext,
        progress: &mut ContractProgress,
        frame: EvidenceFrame<'_>,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), AdapterError> {
        let quote = context.signed_transport_quote()?;
        let authenticated = frame.authenticate(&context.witness, key, signature, quote.max_packet_bytes)?;
        self.check_authenticated_part(context, progress, authenticated)
    }
    pub fn check_authenticated_part(
        &self,
        context: &CheckContext,
        progress: &mut ContractProgress,
        part: AuthenticatedEvidenceFrame<'_>,
    ) -> Result<(), AdapterError> {
        if context.identity != progress.identity || part.author() != context.witness.author {
            return Err(ContractError::Identity.into());
        }
        if progress.failure.is_some() {
            return Err(AdapterError::Closed);
        }
        let frame = part.frame();
        require_next(
            &context.identity.descriptor,
            CoverageCursor { plan: context.identity.descriptor.plan, next_relation: progress.checker.progress().next_leaf },
            &context.witness,
            &frame,
        )?;
        if frame.header().relation_count != 1 {
            return Err(ContractError::NonCanonical.into());
        }
        let quote = context.signed_transport_quote()?;
        // Author-authenticated same-scope attempts pay the signature even if
        // inner material fails. A4 owns aggregate retries and ingress refusals.
        progress.signature_bytes = progress
            .signature_bytes
            .checked_add(SIGNATURE_BYTES as u64)
            .filter(|n| *n <= quote.signature_bytes)
            .ok_or(CheckError::Budget)?;
        self.check_part(context, progress, frame)
    }
}
impl NodeChecker for TirNodeChecker {
    type Context = CheckContext;
    type Progress = ContractProgress;
    type CheckedTerminal = CheckedTerminal;
    type Error = AdapterError;
    fn derive_plan(&self, context: &CheckContext) -> Result<CoverageDescriptor, AdapterError> {
        Ok(context.identity.descriptor)
    }
    fn check_part(
        &self,
        context: &CheckContext,
        progress: &mut ContractProgress,
        part: EvidenceFrame<'_>,
    ) -> Result<(), AdapterError> {
        // Trusted A4/court port only. Untrusted packet ingress must use the
        // author-authenticating helpers above before candidate attribution.
        if context.identity != progress.identity {
            return Err(ContractError::Identity.into());
        }
        if progress.failure.is_some() {
            return Err(AdapterError::Closed);
        }
        // Foreign identity cannot poison another candidate's progress. A4 must
        // authorize the carrier BEFORE attributing malformed bytes to this Panel.
        let end = require_next(
            &context.identity.descriptor,
            CoverageCursor { plan: context.identity.descriptor.plan, next_relation: progress.checker.progress().next_leaf },
            &context.witness,
            &part,
        )?;
        if part.header().relation_count != 1 {
            return Err(ContractError::NonCanonical.into());
        }
        let result = (|| {
            progress.wire_bytes = progress
                .wire_bytes
                .checked_add(part.wire_bytes())
                .filter(|n| *n <= context.identity.limits.max_total_bytes)
                .ok_or(CheckError::Budget)?;
            let payload = decode_frame(&part, context.identity.descriptor.max_part_bytes)?;
            progress.checker.submit_encoded(payload.cone(), payload.trace())?;
            if progress.checker.progress().next_leaf != end {
                return Err(AdapterError::Binding);
            }
            Ok(())
        })();
        if let Err(error) = &result {
            progress.failure = Some(match error {
                AdapterError::Check(CheckError::Unverified | CheckError::Budget) => CheckOutcome::EndedUnverified,
                _ => CheckOutcome::WitnessRejected,
            });
        }
        result
    }
    fn finish(&self, context: &CheckContext, mut progress: ContractProgress) -> Result<CheckedTerminal, AdapterError> {
        if context.identity != progress.identity {
            return Err(ContractError::Identity.into());
        }
        if progress.failure.is_some() {
            return Err(AdapterError::Closed);
        }
        let outcome = progress.checker.finish()?;
        Ok(CheckedTerminal {
            result: context.identity.descriptor.result,
            witness: context.witness.id(),
            descriptor: context.identity.descriptor,
            outcome,
        })
    }
}
