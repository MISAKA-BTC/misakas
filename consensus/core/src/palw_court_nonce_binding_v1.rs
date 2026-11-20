//! **RFC-0057 (`CPN1`) & RFC-0058 §12 — Court Nonce Binding, Per-Panel Challenge Selection, and Split Part Assembly.**
//!
//! This module implements:
//! 1. **`ObligationKey`, `ContextHash`, `SWC1PositiveContext`, and `SWC1ActionContext` (`RFC-0057 §5, §16`)**:
//!    Binds every court/panel obligation to network, policy, claim/Original, case, round, role, actor,
//!    and commitment roots.
//! 2. **`PositiveNonce` vs `ActionNonce` (`RFC-0057 §5.4–§5.5`)**:
//!    - `PositiveNonce` binds `R_panel` (preserving full 64-byte native beacon digests) for positive
//!      Panel verification and per-Panel challenge extraction.
//!    - `ActionNonce` derives deterministically from current state (`round`, `action_kind`, `step`)
//!      with zero future-beacon wait for negative refutations, material requests, responses, and terminals.
//! 3. **Sparse Partial Fisher–Yates Selector (`RFC-0057 §8`)**:
//!    Selects `k` distinct challenge indices from `0..n` in `O(k)` memory with exact 256-bit rejection
//!    sampling and bounded draw cap `R` (`SelectionUnavailable` on exhaustion).
//! 4. **Fixed-Width Binary Merkle `ChunkTree` & Split Assembly (`RFC-0057 §7`)**:
//!    Verifies `CourtSubmissionId` signature and `ChunkRoot` openings before staging split parts,
//!    promoting to canonical state only after complete assembly matches `BodyDigest`.
//! 5. **`CountablePositiveRecord` & Shared Evidence Independence (`RFC-0057 §9–§10`)**:
//!    Separates actor-specific `PositiveNonce` challenge records from nonce-independent shared
//!    `EvidenceMaterialId` / `ServiceReceipt`.

use crate::palw_an1_input_binding_and_panel_court_v1::{An1CourtVerdictV1, An1PanelReviewOutcomeV1};
use crate::palw_staged_work_commitments_v1::{
    SWC1_MAX_FIELD_BYTES_V1, Swc1ErrorV1, Swc1TypedDigestV1, lp_append_checked_v1, nr_append_checked_v1, swc1_h256_v1,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Upper bounds for sparse Fisher–Yates challenge draws (`RFC-0057 §8.1`).
pub const CPN1_K_MAX_V1: u32 = 4_096;
pub const CPN1_R_MAX_V1: u32 = 65_536;
pub const CPN1_MAX_SPLIT_PARTS_V1: u32 = 4_096;

/// Domain separation tags (`RFC-0057 §5.1`, `RFC-0058 §12.1`).
pub const TAG_CPN1_OBLIGATION_V1: &[u8] = b"MISAKA/PALW/CPN1/OBLIGATION";
pub const TAG_CPN1_CONTEXT_V1: &[u8] = b"MISAKA/PALW/CPN1/CONTEXT";
pub const TAG_CPN1_POSITIVE_NONCE_V1: &[u8] = b"MISAKA/PALW/CPN1/POSITIVE-NONCE";
pub const TAG_CPN1_ACTION_NONCE_V1: &[u8] = b"MISAKA/PALW/CPN1/ACTION-NONCE";
pub const TAG_CPN1_TARGETS_V1: &[u8] = b"MISAKA/PALW/CPN1/TARGETS";
pub const TAG_CPN1_BODY_V1: &[u8] = b"MISAKA/PALW/CPN1/BODY";
pub const TAG_CPN1_CHUNK_V1: &[u8] = b"MISAKA/PALW/CPN1/CHUNK";
pub const TAG_CPN1_CHUNK_PAD_V1: &[u8] = b"MISAKA/PALW/CPN1/CHUNK-PAD";
pub const TAG_CPN1_CHUNK_NODE_V1: &[u8] = b"MISAKA/PALW/CPN1/CHUNK-NODE";
pub const TAG_CPN1_SUBMISSION_V1: &[u8] = b"MISAKA/PALW/CPN1/SUBMISSION";
pub const TAG_CPN1_DRAW_V1: &[u8] = b"MISAKA/PALW/CPN1/DRAW";
pub const TAG_CPN1_SHARED_EVIDENCE_V1: &[u8] = b"MISAKA/PALW/CPN1/SHARED-EVIDENCE";
pub const TAG_SWC1_CPN_POSITIVE_CONTEXT_V1: &[u8] = b"MISAKA/PALW/SWC1/CPN-POSITIVE-CONTEXT";
pub const TAG_SWC1_CPN_ACTION_CONTEXT_V1: &[u8] = b"MISAKA/PALW/SWC1/CPN-ACTION-CONTEXT";
pub const TAG_CPN1_AN1_CONTEXT_V1: &[u8] = b"MISAKA/PALW/CPN1/AN1-CONTEXT";

/// Actor roles (`RFC-0057 §5.2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Cpn1ActorRoleV1 {
    Producer = 1,
    PanelSeat = 2,
    Challenger = 3,
    Responder = 4,
    Provider = 5,
}

/// Obligation kinds (`RFC-0057 §5.2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Cpn1ObligationKindV1 {
    PositivePanelDuty = 1,
    NegativeRefutation = 2,
    MaterialDemand = 3,
    MaterialDisclosure = 4,
    CaseTerminal = 5,
}

/// Action kinds for `ActionNonce` (`RFC-0057 §5.5`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Cpn1ActionKindV1 {
    PositiveSubmission = 1,
    NegativeAccusation = 2,
    MaterialRequest = 3,
    MaterialResponse = 4,
    TerminalSettlement = 5,
}

/// Errors produced by `CPN1` nonce derivation, Fisher–Yates selection, chunk verification, and positive counting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cpn1ErrorV1 {
    Swc1(Swc1ErrorV1),
    InvalidVersion(u16),
    InvalidTargetOrdering,
    InvalidSelectionBounds { n: u32, k: u32, r: u32 },
    SelectionUnavailable { unit_kind: u8, draws_attempted: u32 },
    InvalidPartCount { part_index: u32, total_parts: u32 },
    InvalidChunkMerkleProof { part_index: u32 },
    InvalidSubmissionSignature([u8; 32]),
    DuplicateSubmissionOrPart { submission_id: [u8; 32], part_index: u32 },
    ConflictingSubmissionOnNonce { nonce: [u8; 32], existing_id: [u8; 32], new_id: [u8; 32] },
    ReassembledBodyDigestMismatch { expected: [u8; 32], got: [u8; 32] },
    TargetSetMismatch { expected: [u8; 32], got: [u8; 32] },
    MissingCommonCoverage,
    LocalChallengeVerificationFailed { unit_kind: u8, unit_index: u32 },
    PanelFreezeOrSeedMissing,
    PanelNonceMixedIntoMinerInputOrTicket,
}

impl From<Swc1ErrorV1> for Cpn1ErrorV1 {
    fn from(e: Swc1ErrorV1) -> Self {
        Self::Swc1(e)
    }
}

/// Compute `ObligationKey` (`MISAKA/PALW/CPN1/OBLIGATION`, `RFC-0057 §5.2`).
#[allow(clippy::too_many_arguments)]
pub fn cpn1_obligation_key_v1(
    network_id: &[u8; 32],
    policy_id: &[u8; 32],
    claim_id: &Swc1TypedDigestV1,
    case_id: &Swc1TypedDigestV1,
    round: u32,
    actor_role: Cpn1ActorRoleV1,
    actor_id: &Swc1TypedDigestV1,
    obligation_kind: Cpn1ObligationKindV1,
) -> Result<[u8; 32], Cpn1ErrorV1> {
    claim_id.validate()?;
    case_id.validate()?;
    actor_id.validate()?;
    let mut buf = Vec::with_capacity(384);
    lp_append_checked_v1(&mut buf, TAG_CPN1_OBLIGATION_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    nr_append_checked_v1(&mut buf, "network", 1, network_id)?;
    nr_append_checked_v1(&mut buf, "policy", 1, policy_id)?;
    nr_append_checked_v1(&mut buf, "claim", claim_id.version, &claim_id.bytes)?;
    nr_append_checked_v1(&mut buf, "case", case_id.version, &case_id.bytes)?;
    nr_append_checked_v1(&mut buf, "round", 1, &round.to_le_bytes())?;
    nr_append_checked_v1(&mut buf, "role", 1, &[actor_role as u8])?;
    nr_append_checked_v1(&mut buf, "actor", actor_id.version, &actor_id.bytes)?;
    nr_append_checked_v1(&mut buf, "obligation-kind", 1, &[obligation_kind as u8])?;
    Ok(swc1_h256_v1(&buf))
}

/// Canonical 18-field court context (`ContextHash`, `RFC-0057 §5.3`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cpn1ContextFieldsV1 {
    pub network_id: [u8; 32],
    pub policy_id: [u8; 32],
    pub claim_id: Swc1TypedDigestV1,
    pub case_id: Swc1TypedDigestV1,
    pub parent_anchor: Swc1TypedDigestV1,
    pub service_cut: Swc1TypedDigestV1,
    pub model_id: Swc1TypedDigestV1,
    pub runtime_profile_id: Swc1TypedDigestV1,
    pub job_descriptor: Swc1TypedDigestV1,
    pub trace_root: Swc1TypedDigestV1,
    pub output_root: Swc1TypedDigestV1,
    pub transcript_root: Swc1TypedDigestV1,
    pub readiness_root: Swc1TypedDigestV1,
    pub panel_roster_root: Swc1TypedDigestV1,
    pub assignment_anchor: Swc1TypedDigestV1,
    pub open_ms: u64,
    pub due_ms: u64,
    pub hard_expiry_ms: u64,
    pub round: u32,
    pub action_kind: Cpn1ActionKindV1,
    pub step: u32,
}

impl Cpn1ContextFieldsV1 {
    pub fn context_hash(&self) -> Result<[u8; 32], Cpn1ErrorV1> {
        for d in [
            &self.claim_id,
            &self.case_id,
            &self.parent_anchor,
            &self.service_cut,
            &self.model_id,
            &self.runtime_profile_id,
            &self.job_descriptor,
            &self.trace_root,
            &self.output_root,
            &self.transcript_root,
            &self.readiness_root,
            &self.panel_roster_root,
            &self.assignment_anchor,
        ] {
            d.validate()?;
        }
        let mut buf = Vec::with_capacity(1200);
        lp_append_checked_v1(&mut buf, TAG_CPN1_CONTEXT_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        nr_append_checked_v1(&mut buf, "network", 1, &self.network_id)?;
        nr_append_checked_v1(&mut buf, "policy", 1, &self.policy_id)?;
        nr_append_checked_v1(&mut buf, "claim", self.claim_id.version, &self.claim_id.bytes)?;
        nr_append_checked_v1(&mut buf, "case", self.case_id.version, &self.case_id.bytes)?;
        nr_append_checked_v1(&mut buf, "parent-anchor", self.parent_anchor.version, &self.parent_anchor.bytes)?;
        nr_append_checked_v1(&mut buf, "service-cut", self.service_cut.version, &self.service_cut.bytes)?;
        nr_append_checked_v1(&mut buf, "model", self.model_id.version, &self.model_id.bytes)?;
        nr_append_checked_v1(&mut buf, "runtime-profile", self.runtime_profile_id.version, &self.runtime_profile_id.bytes)?;
        nr_append_checked_v1(&mut buf, "job", self.job_descriptor.version, &self.job_descriptor.bytes)?;
        nr_append_checked_v1(&mut buf, "trace-root", self.trace_root.version, &self.trace_root.bytes)?;
        nr_append_checked_v1(&mut buf, "output-root", self.output_root.version, &self.output_root.bytes)?;
        nr_append_checked_v1(&mut buf, "transcript-root", self.transcript_root.version, &self.transcript_root.bytes)?;
        nr_append_checked_v1(&mut buf, "readiness-root", self.readiness_root.version, &self.readiness_root.bytes)?;
        nr_append_checked_v1(&mut buf, "panel-roster", self.panel_roster_root.version, &self.panel_roster_root.bytes)?;
        nr_append_checked_v1(&mut buf, "assignment-anchor", self.assignment_anchor.version, &self.assignment_anchor.bytes)?;
        nr_append_checked_v1(&mut buf, "open-ms", 1, &self.open_ms.to_le_bytes())?;
        nr_append_checked_v1(&mut buf, "due-ms", 1, &self.due_ms.to_le_bytes())?;
        nr_append_checked_v1(&mut buf, "hard-expiry-ms", 1, &self.hard_expiry_ms.to_le_bytes())?;
        buf.extend_from_slice(&self.round.to_le_bytes());
        buf.push(self.action_kind as u8);
        buf.extend_from_slice(&self.step.to_le_bytes());
        Ok(swc1_h256_v1(&buf))
    }
}

/// `SWC1` Positive Context binding all 4 staged identifiers and `PanelFreezeId` (`RFC-0057 §16.1`, `RFC-0058 §12.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1PositiveContextV1 {
    pub base_context_hash: [u8; 32],
    pub opportunity_id: [u8; 32],
    pub job_freeze_id: [u8; 32],
    pub job_seed: [u8; 32],
    pub work_key: [u8; 32],
    pub original_id: [u8; 32],
    pub original_freeze_id: [u8; 32],
    pub work_ticket_be: [u8; 32],
    pub panel_freeze_id: [u8; 32],
}

impl Swc1PositiveContextV1 {
    pub fn context_hash(&self) -> Result<[u8; 32], Cpn1ErrorV1> {
        let mut buf = Vec::with_capacity(512);
        lp_append_checked_v1(&mut buf, TAG_SWC1_CPN_POSITIVE_CONTEXT_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        nr_append_checked_v1(&mut buf, "base-context", 1, &self.base_context_hash)?;
        nr_append_checked_v1(&mut buf, "opportunity", 1, &self.opportunity_id)?;
        nr_append_checked_v1(&mut buf, "job-freeze", 1, &self.job_freeze_id)?;
        nr_append_checked_v1(&mut buf, "job-seed", 1, &self.job_seed)?;
        nr_append_checked_v1(&mut buf, "work-key", 1, &self.work_key)?;
        nr_append_checked_v1(&mut buf, "original-id", 1, &self.original_id)?;
        nr_append_checked_v1(&mut buf, "original-freeze", 1, &self.original_freeze_id)?;
        nr_append_checked_v1(&mut buf, "work-ticket", 1, &self.work_ticket_be)?;
        nr_append_checked_v1(&mut buf, "panel-freeze", 1, &self.panel_freeze_id)?;
        Ok(swc1_h256_v1(&buf))
    }
}

/// `SWC1` Action Context for immediate refutations, material requests/responses, and terminals (`RFC-0057 §16.2`, `RFC-0058 §12.2`).
///
/// Does NOT wait for `PanelFreezeId` or `R_panel`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1ActionContextV1 {
    pub base_context_hash: [u8; 32],
    pub opportunity_id: [u8; 32],
    pub work_key: [u8; 32],
    pub original_id: [u8; 32],
}

impl Swc1ActionContextV1 {
    pub fn context_hash(&self) -> Result<[u8; 32], Cpn1ErrorV1> {
        let mut buf = Vec::with_capacity(256);
        lp_append_checked_v1(&mut buf, TAG_SWC1_CPN_ACTION_CONTEXT_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        nr_append_checked_v1(&mut buf, "base-context", 1, &self.base_context_hash)?;
        nr_append_checked_v1(&mut buf, "opportunity", 1, &self.opportunity_id)?;
        nr_append_checked_v1(&mut buf, "work-key", 1, &self.work_key)?;
        nr_append_checked_v1(&mut buf, "original-id", 1, &self.original_id)?;
        Ok(swc1_h256_v1(&buf))
    }
}

/// Compute `PositiveNonce` (`MISAKA/PALW/CPN1/POSITIVE-NONCE`, `RFC-0057 §5.4`).
///
/// Preserves the full native width (e.g., 64-byte `Hash64` / `R_panel`) inside `NR("beacon-seed", ...)`.
pub fn cpn1_positive_nonce_v1(
    obligation_key: &[u8; 32],
    context_hash: &[u8; 32],
    beacon_round: u64,
    beacon_seed: &Swc1TypedDigestV1,
) -> Result<[u8; 32], Cpn1ErrorV1> {
    beacon_seed.validate()?;
    let mut buf = Vec::with_capacity(200);
    lp_append_checked_v1(&mut buf, TAG_CPN1_POSITIVE_NONCE_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    buf.extend_from_slice(obligation_key);
    buf.extend_from_slice(context_hash);
    nr_append_checked_v1(&mut buf, "beacon-round", 1, &beacon_round.to_le_bytes())?;
    nr_append_checked_v1(&mut buf, "beacon-seed", beacon_seed.version, &beacon_seed.bytes)?;
    Ok(swc1_h256_v1(&buf))
}

/// Compute `ActionNonce` (`MISAKA/PALW/CPN1/ACTION-NONCE`, `RFC-0057 §5.5`).
pub fn cpn1_action_nonce_v1(
    obligation_key: &[u8; 32],
    context_hash: &[u8; 32],
    round: u32,
    action_kind: Cpn1ActionKindV1,
    step: u32,
) -> Result<[u8; 32], Cpn1ErrorV1> {
    let mut buf = Vec::with_capacity(120);
    lp_append_checked_v1(&mut buf, TAG_CPN1_ACTION_NONCE_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    buf.extend_from_slice(obligation_key);
    buf.extend_from_slice(context_hash);
    buf.extend_from_slice(&round.to_le_bytes());
    buf.push(action_kind as u8);
    buf.extend_from_slice(&step.to_le_bytes());
    Ok(swc1_h256_v1(&buf))
}

/// Target entry in a challenge target set (`RFC-0057 §6.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cpn1TargetEntryV1 {
    pub unit_kind: u8,
    pub unit_index: u32,
    pub aux_index: u32,
    pub expected_commitment: Swc1TypedDigestV1,
}

impl Cpn1TargetEntryV1 {
    pub const fn sort_key(&self) -> (u8, u32, u32) {
        (self.unit_kind, self.unit_index, self.aux_index)
    }

    pub fn encode_into(&self, out: &mut Vec<u8>) -> Result<(), Cpn1ErrorV1> {
        self.expected_commitment.validate()?;
        out.push(self.unit_kind);
        out.extend_from_slice(&self.unit_index.to_le_bytes());
        out.extend_from_slice(&self.aux_index.to_le_bytes());
        nr_append_checked_v1(out, "expect", self.expected_commitment.version, &self.expected_commitment.bytes)?;
        Ok(())
    }
}

/// Compute `TargetSetDigest` over strictly sorted, duplicate-free `Cpn1TargetEntryV1` items (`RFC-0057 §6.1`).
pub fn cpn1_target_set_digest_v1(entries: &[Cpn1TargetEntryV1]) -> Result<[u8; 32], Cpn1ErrorV1> {
    let mut buf = Vec::with_capacity(64 + entries.len() * 64);
    lp_append_checked_v1(&mut buf, TAG_CPN1_TARGETS_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    buf.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    let mut prev_key: Option<(u8, u32, u32)> = None;
    for entry in entries {
        let key = entry.sort_key();
        if let Some(prev) = prev_key
            && key <= prev
        {
            return Err(Cpn1ErrorV1::InvalidTargetOrdering);
        }
        prev_key = Some(key);
        entry.encode_into(&mut buf)?;
    }
    Ok(swc1_h256_v1(&buf))
}

/// Compute `BodyDigest` (`MISAKA/PALW/CPN1/BODY`, `RFC-0057 §6.2`).
pub fn cpn1_body_digest_v1(body_kind: u8, schema_version: u16, canonical_body: &[u8]) -> Result<[u8; 32], Cpn1ErrorV1> {
    if schema_version == 0 {
        return Err(Cpn1ErrorV1::InvalidVersion(schema_version));
    }
    let mut buf = Vec::with_capacity(4 + TAG_CPN1_BODY_V1.len() + 3 + 4 + canonical_body.len().min(SWC1_MAX_FIELD_BYTES_V1));
    lp_append_checked_v1(&mut buf, TAG_CPN1_BODY_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    buf.push(body_kind);
    buf.extend_from_slice(&schema_version.to_le_bytes());
    lp_append_checked_v1(&mut buf, canonical_body, SWC1_MAX_FIELD_BYTES_V1)?;
    Ok(swc1_h256_v1(&buf))
}

/// Fixed-width binary Merkle chunk tree (`RFC-0057 §7.2`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cpn1ChunkTreeV1 {
    pub total_parts: u32,
    pub padded_width: usize,
    pub levels: Vec<Vec<[u8; 32]>>,
}

impl Cpn1ChunkTreeV1 {
    pub fn build(chunks: &[Vec<u8>]) -> Result<Self, Cpn1ErrorV1> {
        let total_parts =
            u32::try_from(chunks.len()).map_err(|_| Cpn1ErrorV1::InvalidPartCount { part_index: 0, total_parts: u32::MAX })?;
        if total_parts == 0 || total_parts > CPN1_MAX_SPLIT_PARTS_V1 {
            return Err(Cpn1ErrorV1::InvalidPartCount { part_index: 0, total_parts });
        }
        let m = (total_parts as usize).next_power_of_two().max(1);
        let mut leaves = Vec::with_capacity(m);
        for (i, chunk) in chunks.iter().enumerate() {
            leaves.push(cpn1_chunk_leaf_hash_v1(i as u32, total_parts, chunk)?);
        }
        for i in (total_parts as usize)..m {
            leaves.push(cpn1_chunk_pad_hash_v1(i as u32, total_parts)?);
        }
        let mut levels = vec![leaves];
        let mut level = 0u32;
        while levels.last().expect("non-empty").len() > 1 {
            let cur = levels.last().expect("non-empty");
            let mut next = Vec::with_capacity(cur.len() / 2);
            for pair in cur.chunks_exact(2) {
                next.push(cpn1_chunk_node_hash_v1(level, &pair[0], &pair[1])?);
            }
            levels.push(next);
            level += 1;
        }
        Ok(Self { total_parts, padded_width: m, levels })
    }

    pub fn root(&self) -> [u8; 32] {
        self.levels.last().expect("non-empty")[0]
    }

    pub fn opening_for_part(&self, part_index: u32) -> Result<Vec<[u8; 32]>, Cpn1ErrorV1> {
        if part_index >= self.total_parts {
            return Err(Cpn1ErrorV1::InvalidPartCount { part_index, total_parts: self.total_parts });
        }
        let mut idx = part_index as usize;
        let mut siblings = Vec::with_capacity(self.levels.len().saturating_sub(1));
        for level in 0..(self.levels.len() - 1) {
            let sib_idx = idx ^ 1;
            siblings.push(self.levels[level][sib_idx]);
            idx >>= 1;
        }
        Ok(siblings)
    }
}

pub fn cpn1_chunk_leaf_hash_v1(part_index: u32, total_parts: u32, chunk_bytes: &[u8]) -> Result<[u8; 32], Cpn1ErrorV1> {
    let mut buf = Vec::with_capacity(4 + TAG_CPN1_CHUNK_V1.len() + 8 + 4 + chunk_bytes.len().min(SWC1_MAX_FIELD_BYTES_V1));
    lp_append_checked_v1(&mut buf, TAG_CPN1_CHUNK_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    buf.extend_from_slice(&part_index.to_le_bytes());
    buf.extend_from_slice(&total_parts.to_le_bytes());
    lp_append_checked_v1(&mut buf, chunk_bytes, SWC1_MAX_FIELD_BYTES_V1)?;
    Ok(swc1_h256_v1(&buf))
}

pub fn cpn1_chunk_pad_hash_v1(part_index: u32, total_parts: u32) -> Result<[u8; 32], Cpn1ErrorV1> {
    let mut buf = Vec::with_capacity(4 + TAG_CPN1_CHUNK_PAD_V1.len() + 8);
    lp_append_checked_v1(&mut buf, TAG_CPN1_CHUNK_PAD_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    buf.extend_from_slice(&part_index.to_le_bytes());
    buf.extend_from_slice(&total_parts.to_le_bytes());
    Ok(swc1_h256_v1(&buf))
}

pub fn cpn1_chunk_node_hash_v1(level: u32, left: &[u8; 32], right: &[u8; 32]) -> Result<[u8; 32], Cpn1ErrorV1> {
    let mut buf = Vec::with_capacity(4 + TAG_CPN1_CHUNK_NODE_V1.len() + 4 + 64);
    lp_append_checked_v1(&mut buf, TAG_CPN1_CHUNK_NODE_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    buf.extend_from_slice(&level.to_le_bytes());
    buf.extend_from_slice(left);
    buf.extend_from_slice(right);
    Ok(swc1_h256_v1(&buf))
}

/// Verify a Merkle chunk opening against `chunk_root` (`RFC-0057 §7.2`).
pub fn cpn1_verify_chunk_opening_v1(
    chunk_root: &[u8; 32],
    part_index: u32,
    total_parts: u32,
    chunk_bytes: &[u8],
    merkle_siblings: &[[u8; 32]],
) -> Result<bool, Cpn1ErrorV1> {
    if total_parts == 0 || total_parts > CPN1_MAX_SPLIT_PARTS_V1 || part_index >= total_parts {
        return Err(Cpn1ErrorV1::InvalidPartCount { part_index, total_parts });
    }
    let m = (total_parts as usize).next_power_of_two().max(1);
    let expected_depth = m.trailing_zeros() as usize;
    if merkle_siblings.len() != expected_depth {
        return Ok(false);
    }
    let mut cur = cpn1_chunk_leaf_hash_v1(part_index, total_parts, chunk_bytes)?;
    let mut idx = part_index as usize;
    for (level, sib) in merkle_siblings.iter().enumerate() {
        cur = if (idx & 1) == 0 {
            cpn1_chunk_node_hash_v1(level as u32, &cur, sib)?
        } else {
            cpn1_chunk_node_hash_v1(level as u32, sib, &cur)?
        };
        idx >>= 1;
    }
    Ok(&cur == chunk_root)
}

/// Signed submission subject (`SubmissionSubject`, `RFC-0057 §7.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cpn1SubmissionSubjectV1 {
    pub version: u16,
    pub obligation_key: [u8; 32],
    pub context_hash: [u8; 32],
    pub nonce: [u8; 32],
    pub target_set_digest: [u8; 32],
    pub body_digest: [u8; 32],
    pub total_parts: u32,
    pub chunk_root: [u8; 32],
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
}

impl Cpn1SubmissionSubjectV1 {
    pub fn submission_id(&self) -> Result<[u8; 32], Cpn1ErrorV1> {
        if self.version != 1 {
            return Err(Cpn1ErrorV1::InvalidVersion(self.version));
        }
        if self.total_parts == 0 || self.total_parts > CPN1_MAX_SPLIT_PARTS_V1 {
            return Err(Cpn1ErrorV1::InvalidPartCount { part_index: 0, total_parts: self.total_parts });
        }
        let mut buf = Vec::with_capacity(240);
        lp_append_checked_v1(&mut buf, TAG_CPN1_SUBMISSION_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.obligation_key);
        buf.extend_from_slice(&self.context_hash);
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.target_set_digest);
        buf.extend_from_slice(&self.body_digest);
        buf.extend_from_slice(&self.total_parts.to_le_bytes());
        buf.extend_from_slice(&self.chunk_root);
        buf.extend_from_slice(&self.created_at_ms.to_le_bytes());
        buf.extend_from_slice(&self.expires_at_ms.to_le_bytes());
        Ok(swc1_h256_v1(&buf))
    }
}

/// Exact 256-bit `uint256_be(bytes32) mod m` and rejection sampling check `x < 2^256 - (2^256 mod m)` (`RFC-0057 §8.2`).
#[inline]
fn u256_be_rejection_sample_mod_u32(draw_be: &[u8; 32], m: u32) -> Option<u32> {
    if m <= 1 {
        return Some(0);
    }
    let m64 = m as u64;
    // Compute r_256 = 2^256 mod m
    // Since 2^256 = (2^64)^4, we can compute 2^256 mod m64 in 4 steps, and also compare draw_be < 2^256 - r_256.
    // Note: 2^256 - r_256 has the exact same top 248..224 bits as [0xFF; 32] except in the lowest 32 bits
    // because r_256 < m <= 2^32 - 1!
    // Wait: 2^256 - 1 = 0xFFFF...FFFF.
    // Therefore, 2^256 - r_256 = (2^256 - 1) - (r_256 - 1) when r_256 > 0!
    // When r_256 == 0 (i.e. m is a power of two), every 256-bit value x < 2^256 is accepted!
    let mut rem_pow = (1u128 << 64) % (m64 as u128);
    rem_pow = (rem_pow * rem_pow) % (m64 as u128); // 2^128 mod m
    rem_pow = (rem_pow * rem_pow) % (m64 as u128); // 2^256 mod m
    let r_256 = rem_pow as u32;
    if r_256 != 0 {
        // limit = 2^256 - r_256. Since r_256 <= m - 1 < 2^32,
        // limit in big-endian bytes is [0xFF; 28] followed by (0x1_0000_0000u64 - r_256 as u64) in 4 big-endian bytes!
        let mut limit_be = [0xFFu8; 32];
        let tail = (1u64 << 32) - (r_256 as u64);
        limit_be[28..32].copy_from_slice(&(tail as u32).to_be_bytes());
        if draw_be >= &limit_be {
            return None;
        }
    }
    // Compute draw_be mod m
    let mut rem = 0u64;
    for &b in draw_be {
        rem = ((rem << 8) | (b as u64)) % m64;
    }
    Some(rem as u32)
}

/// Sparse partial Fisher–Yates challenge selector (`RFC-0057 §8.2`).
///
/// Selects `k` distinct indices from `0..n` in `O(k)` memory using `PositiveNonce` and `unit_kind`,
/// returning them in strictly ascending order.
pub fn cpn1_select_targets_v1(
    positive_nonce: &[u8; 32],
    unit_kind: u8,
    n: u32,
    k: u32,
    max_draws_r: u32,
) -> Result<Vec<u32>, Cpn1ErrorV1> {
    if k == 0 || k > CPN1_K_MAX_V1 || k > n || max_draws_r < k || max_draws_r > CPN1_R_MAX_V1 {
        return Err(Cpn1ErrorV1::InvalidSelectionBounds { n, k, r: max_draws_r });
    }
    let mut sparse_swaps: BTreeMap<u32, u32> = BTreeMap::new();
    let mut selected = Vec::with_capacity(k as usize);
    let mut total_draws = 0u32;

    for i in 0..k {
        let m = n - i;
        let mut t = 0u32;
        let j = loop {
            total_draws = total_draws.saturating_add(1);
            if total_draws > max_draws_r {
                return Err(Cpn1ErrorV1::SelectionUnavailable { unit_kind, draws_attempted: total_draws });
            }
            let mut buf = Vec::with_capacity(4 + TAG_CPN1_DRAW_V1.len() + 32 + 1 + 8);
            lp_append_checked_v1(&mut buf, TAG_CPN1_DRAW_V1, SWC1_MAX_FIELD_BYTES_V1)?;
            buf.extend_from_slice(positive_nonce);
            buf.push(unit_kind);
            buf.extend_from_slice(&i.to_le_bytes());
            buf.extend_from_slice(&t.to_le_bytes());
            let draw_be = swc1_h256_v1(&buf);
            if let Some(offset) = u256_be_rejection_sample_mod_u32(&draw_be, m) {
                break i + offset;
            }
            t = t.saturating_add(1);
        };
        let val_j = sparse_swaps.get(&j).copied().unwrap_or(j);
        let val_i = sparse_swaps.get(&i).copied().unwrap_or(i);
        sparse_swaps.insert(j, val_i);
        selected.push(val_j);
    }
    selected.sort_unstable();
    Ok(selected)
}

/// Nonce-independent shared evidence material identifier (`EvidenceMaterialId`, `RFC-0057 §10`).
///
/// Shared evidence is keyed by `(claim_id, unit_kind, unit_index, verified_leaf_hash)` so multiple
/// Panel seats or court participants reuse the same verified material without duplicate carriage.
pub fn cpn1_shared_evidence_material_id_v1(
    claim_id: &Swc1TypedDigestV1,
    unit_kind: u8,
    unit_index: u32,
    verified_leaf_hash: &[u8; 32],
) -> Result<[u8; 32], Cpn1ErrorV1> {
    claim_id.validate()?;
    let mut buf = Vec::with_capacity(140);
    lp_append_checked_v1(&mut buf, TAG_CPN1_SHARED_EVIDENCE_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    nr_append_checked_v1(&mut buf, "claim", claim_id.version, &claim_id.bytes)?;
    buf.push(unit_kind);
    buf.extend_from_slice(&unit_index.to_le_bytes());
    buf.extend_from_slice(verified_leaf_hash);
    Ok(swc1_h256_v1(&buf))
}

/// Staging assembly buffer for split court/panel submissions (`RFC-0057 §7.3`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cpn1SplitAssemblyBufferV1 {
    pub subject: Cpn1SubmissionSubjectV1,
    pub submission_id: [u8; 32],
    pub parts: BTreeMap<u32, Vec<u8>>,
}

/// Assembled and verified court/panel submission record (`RFC-0057 §7, §9`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cpn1CompleteSubmissionV1 {
    pub subject: Cpn1SubmissionSubjectV1,
    pub submission_id: [u8; 32],
    pub body_kind: u8,
    pub schema_version: u16,
    pub reassembled_body: Vec<u8>,
}

/// State manager for `CPN1` split part staging, equivocation detection, and positive counting (`RFC-0057 §7–§11`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cpn1SubmissionManagerV1 {
    /// Active staging buffers keyed by `submission_id`.
    pub staging_assemblies: BTreeMap<[u8; 32], Cpn1SplitAssemblyBufferV1>,
    /// Canonical completed submissions keyed by `submission_id`.
    pub completed_submissions: BTreeMap<[u8; 32], Cpn1CompleteSubmissionV1>,
    /// Nonce -> `submission_id` ledger preventing duplicate/conflicting filings under one nonce (`RFC-0057 §7.3`).
    pub nonce_to_submission: BTreeMap<[u8; 32], [u8; 32]>,
    /// Countable positive records keyed by `(claim_id_hash32, actor_id_hash32)`.
    pub countable_positive_records: BTreeMap<([u8; 32], [u8; 32]), [u8; 32]>,
    /// Shared evidence store keyed by `EvidenceMaterialId` (`RFC-0057 §10`).
    pub shared_evidence_store: BTreeSet<[u8; 32]>,
}

impl Cpn1SubmissionManagerV1 {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ingest one signed split part (`RFC-0057 §7.2–§7.3`).
    ///
    /// Verifies `signature_valid` over `CourtSubmissionId` and `ChunkRoot` Merkle opening BEFORE
    /// allocating or staging any part. Promotes to `completed_submissions` only when all `total_parts`
    /// are present and reassembled bytes match `BodyDigest`.
    #[allow(clippy::too_many_arguments)]
    pub fn ingest_submission_part(
        &mut self,
        subject: Cpn1SubmissionSubjectV1,
        signature_valid: bool,
        part_index: u32,
        chunk_bytes: Vec<u8>,
        merkle_siblings: &[[u8; 32]],
        body_kind: u8,
        schema_version: u16,
    ) -> Result<Option<[u8; 32]>, Cpn1ErrorV1> {
        let submission_id = subject.submission_id()?;
        if !signature_valid {
            return Err(Cpn1ErrorV1::InvalidSubmissionSignature(submission_id));
        }
        if let Some(&existing_sub_id) = self.nonce_to_submission.get(&subject.nonce)
            && existing_sub_id != submission_id
        {
            return Err(Cpn1ErrorV1::ConflictingSubmissionOnNonce {
                nonce: subject.nonce,
                existing_id: existing_sub_id,
                new_id: submission_id,
            });
        }
        if self.completed_submissions.contains_key(&submission_id) {
            return Err(Cpn1ErrorV1::DuplicateSubmissionOrPart { submission_id, part_index });
        }
        if !cpn1_verify_chunk_opening_v1(&subject.chunk_root, part_index, subject.total_parts, &chunk_bytes, merkle_siblings)? {
            return Err(Cpn1ErrorV1::InvalidChunkMerkleProof { part_index });
        }
        self.nonce_to_submission.insert(subject.nonce, submission_id);
        let entry = self.staging_assemblies.entry(submission_id).or_insert_with(|| Cpn1SplitAssemblyBufferV1 {
            subject: subject.clone(),
            submission_id,
            parts: BTreeMap::new(),
        });
        if entry.parts.contains_key(&part_index) {
            return Err(Cpn1ErrorV1::DuplicateSubmissionOrPart { submission_id, part_index });
        }
        entry.parts.insert(part_index, chunk_bytes);
        if entry.parts.len() == subject.total_parts as usize {
            let staged = self.staging_assemblies.remove(&submission_id).expect("just checked");
            let mut reassembled = Vec::new();
            for idx in 0..staged.subject.total_parts {
                let part = staged.parts.get(&idx).expect("all parts 0..total_parts present");
                reassembled.extend_from_slice(part);
            }
            let actual_body_digest = cpn1_body_digest_v1(body_kind, schema_version, &reassembled)?;
            if actual_body_digest != staged.subject.body_digest {
                return Err(Cpn1ErrorV1::ReassembledBodyDigestMismatch {
                    expected: staged.subject.body_digest,
                    got: actual_body_digest,
                });
            }
            self.completed_submissions.insert(
                submission_id,
                Cpn1CompleteSubmissionV1 {
                    subject: staged.subject,
                    submission_id,
                    body_kind,
                    schema_version,
                    reassembled_body: reassembled,
                },
            );
            return Ok(Some(submission_id));
        }
        Ok(None)
    }

    /// Verify `AdmissibleNewPositive` and record `CountablePositiveRecord` (`RFC-0057 §9`).
    ///
    /// - Verifies that the submission's `target_set_digest` matches the deterministic Fisher–Yates
    ///   targets derived from `PositiveNonce`.
    /// - Verifies that common coverage is complete and every selected local challenge target is verified.
    #[allow(clippy::too_many_arguments)]
    pub fn verify_and_record_countable_positive(
        &mut self,
        claim_key32: [u8; 32],
        actor_key32: [u8; 32],
        submission_id: [u8; 32],
        expected_obligation_key: [u8; 32],
        expected_context_hash: [u8; 32],
        beacon_round: u64,
        r_panel: &Swc1TypedDigestV1,
        unit_population_and_k: &[(u8, u32, u32, u32)],
        submitted_targets: &[Cpn1TargetEntryV1],
        common_coverage_complete: bool,
        all_local_targets_verified: bool,
    ) -> Result<[u8; 32], Cpn1ErrorV1> {
        let completed =
            self.completed_submissions.get(&submission_id).ok_or(Cpn1ErrorV1::InvalidSubmissionSignature(submission_id))?;
        let expected_pos_nonce = cpn1_positive_nonce_v1(&expected_obligation_key, &expected_context_hash, beacon_round, r_panel)?;
        if completed.subject.obligation_key != expected_obligation_key
            || completed.subject.context_hash != expected_context_hash
            || completed.subject.nonce != expected_pos_nonce
        {
            return Err(Cpn1ErrorV1::InvalidSubmissionSignature(submission_id));
        }
        if !common_coverage_complete {
            return Err(Cpn1ErrorV1::MissingCommonCoverage);
        }
        if !all_local_targets_verified {
            return Err(Cpn1ErrorV1::LocalChallengeVerificationFailed { unit_kind: 0, unit_index: 0 });
        }
        // Check that submitted_targets match the Fisher-Yates draw for every required unit_kind
        for &(unit_kind, n, k, max_draws_r) in unit_population_and_k {
            let expected_indices = cpn1_select_targets_v1(&expected_pos_nonce, unit_kind, n, k, max_draws_r)?;
            let actual_indices: Vec<u32> =
                submitted_targets.iter().filter(|t| t.unit_kind == unit_kind).map(|t| t.unit_index).collect();
            if actual_indices != expected_indices {
                return Err(Cpn1ErrorV1::LocalChallengeVerificationFailed {
                    unit_kind,
                    unit_index: *expected_indices.first().unwrap_or(&0),
                });
            }
        }
        let expected_target_digest = cpn1_target_set_digest_v1(submitted_targets)?;
        if completed.subject.target_set_digest != expected_target_digest {
            return Err(Cpn1ErrorV1::TargetSetMismatch { expected: expected_target_digest, got: completed.subject.target_set_digest });
        }
        self.countable_positive_records.insert((claim_key32, actor_key32), submission_id);
        Ok(expected_pos_nonce)
    }

    /// Check whether `claim_key32` has at least `k_min` countable positive records (e.g., `k_min = 1`
    /// under `PublicOneHonest`, `RFC-0057 §9.2`).
    pub fn has_quorum_of_countable_positives(&self, claim_key32: &[u8; 32], k_min: usize) -> bool {
        let count = self.countable_positive_records.keys().filter(|(cid, _)| cid == claim_key32).count();
        count >= k_min
    }
}

/// Distinct `PALW-AN1` adjudication status (`RFC-0057 §17.4`, `RFC-0059 §7.4`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cpn1An1AdjudicationStatusV1 {
    /// Unauthenticated signature, scope, opening, root, expected index, or codec -> submission rejected without clearing `Q`.
    InvalidEvidence { reason: String },
    /// Local target relation holds -> does NOT imply full trace validity, full Panel replay, or `Final`.
    LocalHolds { step_index: usize },
    /// Authenticated violation of committed value, structure, or output -> blocks `Final` and routes to refutation/slashing.
    ComputationRefuted { reason: String },
    /// Required material or qualified objection not yet resolved -> holds `Final` without timeout-success (`A57-09`, `A57-11`).
    MaterialOrInspectionPending { reason: String },
}

impl Cpn1An1AdjudicationStatusV1 {
    pub fn from_court_verdict(verdict: &An1CourtVerdictV1, step_index: usize) -> Self {
        match verdict {
            An1CourtVerdictV1::InvalidEvidence => {
                Self::InvalidEvidence { reason: format!("invalid court evidence at index {step_index}") }
            }
            An1CourtVerdictV1::NoRefutation => Self::LocalHolds { step_index },
            An1CourtVerdictV1::ComputationRefuted => {
                Self::ComputationRefuted { reason: format!("computation refuted at index {step_index}") }
            }
        }
    }

    pub fn from_panel_review_outcome(outcome: &An1PanelReviewOutcomeV1) -> Option<Self> {
        match outcome {
            An1PanelReviewOutcomeV1::ApprovedFullReplay { .. } => None,
            An1PanelReviewOutcomeV1::Refuted { court, .. } => {
                Some(Self::ComputationRefuted { reason: format!("refuted at index {}", court.index) })
            }
            An1PanelReviewOutcomeV1::MaterialProblem { status, reason, .. } => {
                Some(Self::MaterialOrInspectionPending { reason: format!("{status}: {reason}") })
            }
        }
    }

    /// True when this adjudication status blocks `SuccessFinal` (`RFC-0057 §17.4, §17.6`).
    #[inline]
    pub const fn blocks_success_final(&self) -> bool {
        matches!(self, Self::ComputationRefuted { .. } | Self::MaterialOrInspectionPending { .. })
    }
}

/// Native-authoritative `PALW-AN1` Court & Panel context binding (`RFC-0057 §17.1–§17.2`).
///
/// Separates Miner registration nonce / `s` (input/trace/ticket binding) from Panel `PositiveNonce`
/// (post-claim-seal challenge selection) and immediate `ActionNonce` (negative refutations and
/// material requests/responses without waiting for future beacon seeds).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cpn1An1ContextBindingV1 {
    pub chain_id: String,
    pub slot_id: [u8; 32],
    pub owner: String,
    pub miner_registration_nonce: [u8; 32],
    pub attempt_s: [u8; 32],
    pub original_id: [u8; 32],
    pub input_root: [u8; 32],
    pub output_root: [u8; 32],
    pub trace_root: [u8; 32],
    pub model_root: [u8; 32],
    pub program_root: [u8; 32],
    pub profile_id: [u8; 32],
    pub trace_records: u64,
    pub output_count: u64,
}

impl Cpn1An1ContextBindingV1 {
    /// Compute the domain-separated `AN1` Court context hash (`RFC-0057 §17.2`), distinct from legacy `CPN1` / `SWC1`.
    pub fn an1_context_hash(&self) -> Result<[u8; 32], Cpn1ErrorV1> {
        let mut buf = Vec::with_capacity(560);
        lp_append_checked_v1(&mut buf, TAG_CPN1_AN1_CONTEXT_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        nr_append_checked_v1(&mut buf, "chain", 1, self.chain_id.as_bytes())?;
        nr_append_checked_v1(&mut buf, "slot-id", 1, &self.slot_id)?;
        nr_append_checked_v1(&mut buf, "owner", 1, self.owner.as_bytes())?;
        nr_append_checked_v1(&mut buf, "miner-reg-nonce", 1, &self.miner_registration_nonce)?;
        nr_append_checked_v1(&mut buf, "attempt-s", 1, &self.attempt_s)?;
        nr_append_checked_v1(&mut buf, "original-id", 1, &self.original_id)?;
        nr_append_checked_v1(&mut buf, "input-root", 1, &self.input_root)?;
        nr_append_checked_v1(&mut buf, "output-root", 1, &self.output_root)?;
        nr_append_checked_v1(&mut buf, "trace-root", 1, &self.trace_root)?;
        nr_append_checked_v1(&mut buf, "model-root", 1, &self.model_root)?;
        nr_append_checked_v1(&mut buf, "program-root", 1, &self.program_root)?;
        nr_append_checked_v1(&mut buf, "profile-id", 1, &self.profile_id)?;
        nr_append_checked_v1(&mut buf, "trace-records", 1, &self.trace_records.to_le_bytes())?;
        nr_append_checked_v1(&mut buf, "output-count", 1, &self.output_count.to_le_bytes())?;
        Ok(swc1_h256_v1(&buf))
    }

    /// Derive Panel `PositiveNonce` after Original seal and verify that Panel nonce is never mixed into
    /// Miner registration nonce, `attempt_s`, or roots (`RFC-0057 §17.1`, `A57-02`).
    pub fn derive_panel_positive_nonce(
        &self,
        obligation_key: &[u8; 32],
        panel_epoch: u64,
        r_panel: &Swc1TypedDigestV1,
    ) -> Result<[u8; 32], Cpn1ErrorV1> {
        let ctx_hash = self.an1_context_hash()?;
        let pos_nonce = cpn1_positive_nonce_v1(obligation_key, &ctx_hash, panel_epoch, r_panel)?;
        if pos_nonce == self.miner_registration_nonce
            || pos_nonce == self.attempt_s
            || pos_nonce == self.input_root
            || pos_nonce == self.trace_root
            || pos_nonce == self.output_root
        {
            return Err(Cpn1ErrorV1::PanelNonceMixedIntoMinerInputOrTicket);
        }
        Ok(pos_nonce)
    }

    /// Reject any attempt to use a Panel `PositiveNonce` as Miner registration nonce or `attempt_s` (`A57-02`).
    pub fn verify_miner_nonce_disjoint_from_panel_nonce(&self, panel_positive_nonce: &[u8; 32]) -> Result<(), Cpn1ErrorV1> {
        if &self.miner_registration_nonce == panel_positive_nonce || &self.attempt_s == panel_positive_nonce {
            return Err(Cpn1ErrorV1::PanelNonceMixedIntoMinerInputOrTicket);
        }
        Ok(())
    }

    /// Derive immediate `ActionNonce` for negative refutation, material request/response, or terminal
    /// settlement without waiting for `R_panel` or any new `D_admit` (`RFC-0057 §17.1, §17.6`, `A57-09`).
    pub fn derive_immediate_action_nonce(
        &self,
        obligation_key: &[u8; 32],
        round: u32,
        action_kind: Cpn1ActionKindV1,
        step: u32,
    ) -> Result<[u8; 32], Cpn1ErrorV1> {
        let ctx_hash = self.an1_context_hash()?;
        cpn1_action_nonce_v1(obligation_key, &ctx_hash, round, action_kind, step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpn1_positive_nonce_fisher_yates_split_assembly_and_action_nonce_without_beacon_wait() {
        let net = [0x11; 32];
        let pol = [0x22; 32];
        let claim = Swc1TypedDigestV1::new64([0x33; 64]);
        let case = Swc1TypedDigestV1::new64([0x44; 64]);
        let seat_a = Swc1TypedDigestV1::new64([0x55; 64]);
        let seat_b = Swc1TypedDigestV1::new64([0x66; 64]);
        let r_panel = Swc1TypedDigestV1::new64([0x77; 64]);

        let ob_a = cpn1_obligation_key_v1(
            &net,
            &pol,
            &claim,
            &case,
            1,
            Cpn1ActorRoleV1::PanelSeat,
            &seat_a,
            Cpn1ObligationKindV1::PositivePanelDuty,
        )
        .unwrap();
        let ob_b = cpn1_obligation_key_v1(
            &net,
            &pol,
            &claim,
            &case,
            1,
            Cpn1ActorRoleV1::PanelSeat,
            &seat_b,
            Cpn1ObligationKindV1::PositivePanelDuty,
        )
        .unwrap();
        assert_ne!(ob_a, ob_b);

        let ctx_hash = [0x88; 32];
        let pos_nonce_a = cpn1_positive_nonce_v1(&ob_a, &ctx_hash, 10, &r_panel).unwrap();
        let pos_nonce_b = cpn1_positive_nonce_v1(&ob_b, &ctx_hash, 10, &r_panel).unwrap();
        assert_ne!(pos_nonce_a, pos_nonce_b);

        // Changing the second half (bytes 32..64) of the 64-byte beacon seed must change PositiveNonce!
        let mut r_panel_alt_bytes = [0x77; 64];
        r_panel_alt_bytes[63] ^= 0x01;
        let pos_nonce_a_alt = cpn1_positive_nonce_v1(&ob_a, &ctx_hash, 10, &Swc1TypedDigestV1::new64(r_panel_alt_bytes)).unwrap();
        assert_ne!(pos_nonce_a, pos_nonce_a_alt);

        // ActionNonce for negative refutation requires NO beacon seed (R_panel)
        let action_nonce = cpn1_action_nonce_v1(&ob_a, &ctx_hash, 1, Cpn1ActionKindV1::NegativeAccusation, 0).unwrap();
        assert_ne!(action_nonce, pos_nonce_a);

        // Sparse Fisher-Yates selects k distinct sorted indices from n = 85_938
        let targets_a = cpn1_select_targets_v1(&pos_nonce_a, 1, 85_938, 8, 64).unwrap();
        let targets_b = cpn1_select_targets_v1(&pos_nonce_b, 1, 85_938, 8, 64).unwrap();
        assert_eq!(targets_a.len(), 8);
        assert_ne!(targets_a, targets_b);
        for w in targets_a.windows(2) {
            assert!(w[0] < w[1]);
        }

        // Split part assembly and CountablePositiveRecord verification
        let entries_a: Vec<Cpn1TargetEntryV1> = targets_a
            .iter()
            .map(|&idx| Cpn1TargetEntryV1 {
                unit_kind: 1,
                unit_index: idx,
                aux_index: 0,
                expected_commitment: Swc1TypedDigestV1::new32([0x99; 32]),
            })
            .collect();
        let target_digest_a = cpn1_target_set_digest_v1(&entries_a).unwrap();

        let part0 = b"positive-panel-part-0".to_vec();
        let part1 = b"-and-part-1".to_vec();
        let mut full_body = part0.clone();
        full_body.extend_from_slice(&part1);
        let body_digest = cpn1_body_digest_v1(1, 1, &full_body).unwrap();
        let chunk_tree = Cpn1ChunkTreeV1::build(&[part0.clone(), part1.clone()]).unwrap();

        let subject = Cpn1SubmissionSubjectV1 {
            version: 1,
            obligation_key: ob_a,
            context_hash: ctx_hash,
            nonce: pos_nonce_a,
            target_set_digest: target_digest_a,
            body_digest,
            total_parts: 2,
            chunk_root: chunk_tree.root(),
            created_at_ms: 1_000,
            expires_at_ms: 20_000,
        };
        let sub_id = subject.submission_id().unwrap();

        let mut mgr = Cpn1SubmissionManagerV1::new();
        let open0 = chunk_tree.opening_for_part(0).unwrap();
        let open1 = chunk_tree.opening_for_part(1).unwrap();

        // Part 0 alone stages without promoting to completed
        assert_eq!(mgr.ingest_submission_part(subject.clone(), true, 0, part0, &open0, 1, 1).unwrap(), None);
        // Part 1 completes assembly and verifies BodyDigest
        assert_eq!(mgr.ingest_submission_part(subject, true, 1, part1, &open1, 1, 1).unwrap(), Some(sub_id));

        // Verify CountablePositiveRecord for seat A (under PublicOneHonest, k_min = 1 reaches quorum even if seat B is silent)
        mgr.verify_and_record_countable_positive(
            [0x33; 32],
            [0x55; 32],
            sub_id,
            ob_a,
            ctx_hash,
            10,
            &r_panel,
            &[(1, 85_938, 8, 64)],
            &entries_a,
            true,
            true,
        )
        .unwrap();
        assert!(mgr.has_quorum_of_countable_positives(&[0x33; 32], 1));

        // Shared evidence material ID is identical across seat A and seat B
        let ev_a = cpn1_shared_evidence_material_id_v1(&claim, 1, targets_a[0], &[0xAA; 32]).unwrap();
        let ev_b = cpn1_shared_evidence_material_id_v1(&claim, 1, targets_a[0], &[0xAA; 32]).unwrap();
        assert_eq!(ev_a, ev_b);
    }

    #[test]
    fn cpn1_an1_context_binding_role_separation_and_adjudication_mapping_a57_01_to_a57_12() {
        let mut ctx = Cpn1An1ContextBindingV1 {
            chain_id: "misaka-an1-test".to_owned(),
            slot_id: [0x11; 32],
            owner: "miner-alice".to_owned(),
            miner_registration_nonce: [0x22; 32],
            attempt_s: [0x33; 32],
            original_id: [0x44; 32],
            input_root: [0x55; 32],
            output_root: [0x66; 32],
            trace_root: [0x77; 32],
            model_root: [0x88; 32],
            program_root: [0x99; 32],
            profile_id: [0xAA; 32],
            trace_records: 6153,
            output_count: 2,
        };
        let ob = [0xBB; 32];
        let r_panel = Swc1TypedDigestV1::new64([0xCC; 64]);
        let pos_nonce = ctx.derive_panel_positive_nonce(&ob, 12, &r_panel).unwrap();
        let act_nonce = ctx.derive_immediate_action_nonce(&ob, 1, Cpn1ActionKindV1::NegativeAccusation, 0).unwrap();
        assert_ne!(pos_nonce, act_nonce);
        assert!(ctx.verify_miner_nonce_disjoint_from_panel_nonce(&pos_nonce).is_ok());

        // A57-02: Mixing Panel PositiveNonce into Miner registration nonce or attempt_s is rejected
        ctx.miner_registration_nonce = pos_nonce;
        assert_eq!(
            ctx.verify_miner_nonce_disjoint_from_panel_nonce(&pos_nonce),
            Err(Cpn1ErrorV1::PanelNonceMixedIntoMinerInputOrTicket)
        );

        // A57-04..A57-11: Verdict mapping distinguishes InvalidEvidence, LocalHolds, ComputationRefuted, and MaterialPending
        let inv = Cpn1An1AdjudicationStatusV1::from_court_verdict(&An1CourtVerdictV1::InvalidEvidence, 10);
        assert!(!inv.blocks_success_final());

        let loc = Cpn1An1AdjudicationStatusV1::from_court_verdict(&An1CourtVerdictV1::NoRefutation, 10);
        assert!(!loc.blocks_success_final());

        let refu = Cpn1An1AdjudicationStatusV1::from_court_verdict(&An1CourtVerdictV1::ComputationRefuted, 10);
        assert!(refu.blocks_success_final());

        let mat_hold = Cpn1An1AdjudicationStatusV1::from_panel_review_outcome(&An1PanelReviewOutcomeV1::MaterialProblem {
            status: "MaterialsUnauthenticated".to_owned(),
            original_id: "00".repeat(32),
            reason: "oversized leaf cannot be boundedly opened".to_owned(),
            required_scope: crate::palw_an1_input_binding_and_panel_court_v1::An1RequiredScopeV1 {
                kind: "trace".to_owned(),
                index: 7,
            },
        })
        .unwrap();
        assert!(mat_hold.blocks_success_final());
    }
}
