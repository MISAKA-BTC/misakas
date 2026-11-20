//! **RFC-0058 (`SWC1`) — Staged Work Commitments, Four Identifiers, and Five One-Time Ledgers.**
//!
//! This module implements the deterministic causal chain and one-time state ledgers specified in
//! `docs/rfc/0058-staged-work-commitments-and-consensus-eligibility.md`:
//!
//! 1. **Causal Stage Order**:
//!    `RegistrationId` / `OpportunityId` (`JobRegistration` + `TrialSlotKey`)
//!    → `JobFreezeId` & `R_job` (`JobSeed` & `JobForSeed`)
//!    → `ComputationalWorkKey` (pure computation identity, excluding aliases/signatures/fees/outputs)
//!    → `OriginalId` (committed execution result bound to `OpportunityId`, `JobSeed`, and `WorkKey`)
//!    → `OriginalFreezeId` & `R_ticket` (`WorkTicket` & `Rank = (WorkTicket, OriginalId)`)
//!    → `PanelFreezeId` & `R_panel` (post-admission Panel & local challenge assignment seed).
//!
//! 2. **Five Atomic One-Time Ledgers**:
//!    - `TrialUse[OpportunityId]`
//!    - `WorkUse[(JobFreezeId, ComputationalWorkKey)]`
//!    - `StageUse[(StageKind, StageEpoch)]`
//!    - `OriginalState[OriginalId]`
//!    - `RightsUse[(RightKind, SourceOriginalId, RightSubKey)]`
//!    Together with per-capital monotone sequence highwater and an authenticated expired-epoch
//!    floor + summary root so pruned/GC'd slots cannot be replayed.
//!
//! Inviolable rules (`docs/PRINCIPLES.md` §8 & §9):
//! - Uses only SHA-256 (`H256`), length-prefixed canonical records (`LP` / `NR`), Merkle set commitments,
//!   and post-close randomness. No cryptographic proofs of LLM/model computation correctness exist or are accepted.
//! - Threshold BLS beacon profile (`ThresholdBls12381G2Disabled`) is permanently disabled under PQ-only rules.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum byte length accepted by `lp_encode_checked` / `nr_encode_checked` before allocation.
pub const SWC1_MAX_FIELD_BYTES_V1: usize = 16 * 1024 * 1024;

/// Maximum members permitted in a single frozen stage descriptor set.
pub const SWC1_MAX_STAGE_MEMBERS_V1: usize = 262_144;

/// Maximum difficulty `D_max = 2^120 - 1` (`u128`).
pub const SWC1_MAX_DIFFICULTY_D_V1: u128 = (1u128 << 120) - 1;

/// Stage kind constants (`RFC-0058 §5.2`).
pub const SWC1_STAGE_JOB_V1: u8 = 1;
pub const SWC1_STAGE_TICKET_V1: u8 = 2;
pub const SWC1_STAGE_PANEL_V1: u8 = 3;

/// Domain separation tags (`RFC-0058 §4.2`).
pub const TAG_SWC1_REGISTRATION_V1: &[u8] = b"MISAKA/PALW/SWC1/REGISTRATION";
pub const TAG_SWC1_OPPORTUNITY_V1: &[u8] = b"MISAKA/PALW/SWC1/OPPORTUNITY";
pub const TAG_SWC1_JOB_SEED_V1: &[u8] = b"MISAKA/PALW/SWC1/JOB-SEED";
pub const TAG_SWC1_WORK_V1: &[u8] = b"MISAKA/PALW/SWC1/WORK";
pub const TAG_SWC1_ORIGINAL_V1: &[u8] = b"MISAKA/PALW/SWC1/ORIGINAL";
pub const TAG_SWC1_WORK_TICKET_V1: &[u8] = b"MISAKA/PALW/SWC1/WORK-TICKET";
pub const TAG_SWC1_FREEZE_V1: &[u8] = b"MISAKA/PALW/SWC1/FREEZE";
pub const TAG_SWC1_BEACON_MESSAGE_V1: &[u8] = b"MISAKA/PALW/SWC1/BEACON-MESSAGE";
pub const TAG_SWC1_SET_LEAF_V1: &[u8] = b"MISAKA/PALW/SWC1/SET-LEAF";
pub const TAG_SWC1_SET_PAD_V1: &[u8] = b"MISAKA/PALW/SWC1/SET-PAD";
pub const TAG_SWC1_SET_NODE_V1: &[u8] = b"MISAKA/PALW/SWC1/SET-NODE";
pub const TAG_SWC1_SET_EMPTY_V1: &[u8] = b"MISAKA/PALW/SWC1/SET-EMPTY";
pub const TAG_SWC1_ACTUAL_INPUT_V1: &[u8] = b"MISAKA/PALW/SWC1/ACTUAL-INPUT";
pub const TAG_SWC1_PRUNED_SUMMARY_V1: &[u8] = b"MISAKA/PALW/SWC1/PRUNED-SUMMARY";
pub const TAG_CAD1_SLOT_V1: &[u8] = b"MISAKA/PALW/CAD1/SLOT";

/// Errors returned by `SWC1` commitment, stage freeze, and ledger transitions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Swc1ErrorV1 {
    FieldTooLarge { len: usize, max: usize },
    InvalidVersion(u16),
    InvalidStageKind(u8),
    InvalidSeedDerivationProfile(u16),
    InvalidDifficulty(u128),
    StageMemberLimitExceeded { count: usize, max: usize },
    BeaconProfileDisabled,
    InvalidStageWindow { close_ms: u64, seed_ready_deadline_ms: u64 },
    ParentStageNotFrozen { stage_kind: u8, expected_parent: [u8; 32] },
    ParentStageSeedMissing { stage_kind: u8, parent_freeze_id: [u8; 32] },
    StageAlreadyFrozen { stage_kind: u8, stage_epoch: u64 },
    StageNotFrozen { stage_kind: u8, stage_epoch: u64 },
    StageFreezeMismatch { expected: [u8; 32], got: [u8; 32] },
    SeedAlreadyRecorded { stage_kind: u8, stage_epoch: u64 },
    SeedBeaconMessageMismatch { expected: [u8; 32], got: [u8; 32] },
    SeedArrivedBeforeClose { arrived_ms: u64, close_ms: u64 },
    SeedArrivedAfterDeadline { arrived_ms: u64, deadline_ms: u64 },
    SeedConfirmDelayInsufficient { arrived_ms: u64, close_ms: u64, required_delay_ms: u64 },
    InsufficientCapitalLock { locked_sompi: u64, required_sompi: u64 },
    InsufficientPresealFee { fee_sompi: u64, required_sompi: u64 },
    ExpiredJobEpochFloor { job_epoch: u64, floor_epoch: u64 },
    CapitalSequenceNotMonotone { capital_lock_ref: [u8; 32], sequence: u64, highwater: u64 },
    TrialOpportunityAlreadyRegistered([u8; 32]),
    TrialOpportunityNotFound([u8; 32]),
    TrialOpportunityAlreadyUsed([u8; 32]),
    JobFreezeMismatch { expected: [u8; 32], got: [u8; 32] },
    JobSeedMismatch { expected: [u8; 32], got: [u8; 32] },
    ActualInputMismatch { expected: [u8; 32], got: [u8; 32] },
    ReadyWindowExceeded { ready_at_ms: u64, max_ready_ms: u64 },
    DuplicateWorkKeyInCohort { job_freeze_id: [u8; 32], work_key: [u8; 32], existing_original_id: [u8; 32] },
    DuplicateOriginalId([u8; 32]),
    OriginalNotFound([u8; 32]),
    OriginalNotInFrozenSet([u8; 32]),
    OriginalNotTicketEligible([u8; 32]),
    InvalidOriginalStateTransition { original_id: [u8; 32], from: Swc1VerificationStageV1, to: Swc1VerificationStageV1 },
    RightAlreadyConsumed { kind: Swc1RightKindV1, source_original_id: [u8; 32], sub_key: [u8; 32] },
    OriginalNotMatureEligible { original_id: [u8; 32], current_stage: Swc1VerificationStageV1 },
}

/// Compute `SHA-256` (`H256`) over a byte slice.
#[inline]
pub fn swc1_h256_v1(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Append `LP(x) = u32_le(len(x)) || x` into `out` after checking `x.len() <= max_len`.
#[inline]
pub fn lp_append_checked_v1(out: &mut Vec<u8>, bytes: &[u8], max_len: usize) -> Result<(), Swc1ErrorV1> {
    if bytes.len() > max_len {
        return Err(Swc1ErrorV1::FieldTooLarge { len: bytes.len(), max: max_len });
    }
    let len_u32 = u32::try_from(bytes.len()).map_err(|_| Swc1ErrorV1::FieldTooLarge { len: bytes.len(), max: max_len })?;
    out.extend_from_slice(&len_u32.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

/// Encode `LP(x) = u32_le(len(x)) || x`.
#[inline]
pub fn lp_encode_v1(bytes: &[u8]) -> Result<Vec<u8>, Swc1ErrorV1> {
    let mut out = Vec::with_capacity(4 + bytes.len().min(SWC1_MAX_FIELD_BYTES_V1));
    lp_append_checked_v1(&mut out, bytes, SWC1_MAX_FIELD_BYTES_V1)?;
    Ok(out)
}

/// Append `NR(tag, v, x) = LP(ascii(tag)) || u16_le(v) || LP(x)` into `out`.
#[inline]
pub fn nr_append_checked_v1(out: &mut Vec<u8>, tag: &str, version: u16, bytes: &[u8]) -> Result<(), Swc1ErrorV1> {
    if version == 0 {
        return Err(Swc1ErrorV1::InvalidVersion(version));
    }
    lp_append_checked_v1(out, tag.as_bytes(), SWC1_MAX_FIELD_BYTES_V1)?;
    out.extend_from_slice(&version.to_le_bytes());
    lp_append_checked_v1(out, bytes, SWC1_MAX_FIELD_BYTES_V1)?;
    Ok(())
}

/// Encode `NR(tag, v, x)`.
#[inline]
pub fn nr_encode_v1(tag: &str, version: u16, bytes: &[u8]) -> Result<Vec<u8>, Swc1ErrorV1> {
    let mut out = Vec::with_capacity(10 + tag.len() + bytes.len().min(SWC1_MAX_FIELD_BYTES_V1));
    nr_append_checked_v1(&mut out, tag, version, bytes)?;
    Ok(out)
}

/// Variable-width native digest wrapper (`32` bytes for SHA-256 / BLAKE3-256, `64` bytes for `Hash64` / BLAKE2b-512).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Swc1TypedDigestV1 {
    pub version: u16,
    pub bytes: Vec<u8>,
}

impl Swc1TypedDigestV1 {
    pub fn new32(bytes: [u8; 32]) -> Self {
        Self { version: 1, bytes: bytes.to_vec() }
    }

    pub fn new64(bytes: [u8; 64]) -> Self {
        Self { version: 2, bytes: bytes.to_vec() }
    }

    pub fn validate(&self) -> Result<(), Swc1ErrorV1> {
        match (self.version, self.bytes.len()) {
            (1, 32) | (2, 64) => Ok(()),
            _ => Err(Swc1ErrorV1::InvalidVersion(self.version)),
        }
    }
}

/// Compute the canonical power-of-two padded Merkle root for a frozen stage set (`RFC-0058 §6.1`).
pub fn swc1_frozen_set_root_v1(stage_kind: u8, member_ids: &[[u8; 32]]) -> Result<[u8; 32], Swc1ErrorV1> {
    if !matches!(stage_kind, SWC1_STAGE_JOB_V1 | SWC1_STAGE_TICKET_V1 | SWC1_STAGE_PANEL_V1) {
        return Err(Swc1ErrorV1::InvalidStageKind(stage_kind));
    }
    if member_ids.len() > SWC1_MAX_STAGE_MEMBERS_V1 {
        return Err(Swc1ErrorV1::StageMemberLimitExceeded { count: member_ids.len(), max: SWC1_MAX_STAGE_MEMBERS_V1 });
    }
    let unique_sorted: Vec<[u8; 32]> = member_ids.iter().copied().collect::<BTreeSet<_>>().into_iter().collect();
    if unique_sorted.is_empty() {
        let mut buf = Vec::with_capacity(4 + TAG_SWC1_SET_EMPTY_V1.len() + 1);
        lp_append_checked_v1(&mut buf, TAG_SWC1_SET_EMPTY_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.push(stage_kind);
        return Ok(swc1_h256_v1(&buf));
    }
    let n = unique_sorted.len();
    let m = n.next_power_of_two();
    let mut level_nodes = Vec::with_capacity(m);
    for (idx, item) in unique_sorted.iter().enumerate() {
        let mut buf = Vec::with_capacity(64);
        lp_append_checked_v1(&mut buf, TAG_SWC1_SET_LEAF_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.push(stage_kind);
        buf.extend_from_slice(&(idx as u32).to_le_bytes());
        lp_append_checked_v1(&mut buf, item, SWC1_MAX_FIELD_BYTES_V1)?;
        level_nodes.push(swc1_h256_v1(&buf));
    }
    for idx in n..m {
        let mut buf = Vec::with_capacity(40);
        lp_append_checked_v1(&mut buf, TAG_SWC1_SET_PAD_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.push(stage_kind);
        buf.extend_from_slice(&(idx as u32).to_le_bytes());
        level_nodes.push(swc1_h256_v1(&buf));
    }
    let mut level = 0u32;
    while level_nodes.len() > 1 {
        let mut next = Vec::with_capacity(level_nodes.len() / 2);
        for pair in level_nodes.chunks_exact(2) {
            let mut buf = Vec::with_capacity(4 + TAG_SWC1_SET_NODE_V1.len() + 1 + 4 + 64);
            lp_append_checked_v1(&mut buf, TAG_SWC1_SET_NODE_V1, SWC1_MAX_FIELD_BYTES_V1)?;
            buf.push(stage_kind);
            buf.extend_from_slice(&level.to_le_bytes());
            buf.extend_from_slice(&pair[0]);
            buf.extend_from_slice(&pair[1]);
            next.push(swc1_h256_v1(&buf));
        }
        level_nodes = next;
        level += 1;
    }
    Ok(level_nodes[0])
}

/// Canonical stage descriptor (`RFC-0058 §6.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1StageDescriptorV1 {
    pub version: u16,
    pub stage_kind: u8,
    pub network_id: [u8; 32],
    pub policy_id: [u8; 32],
    pub stage_epoch: u64,
    pub parent_freeze_id: [u8; 32],
    pub canonical_state_cut: [u8; 32],
    pub frozen_set_root: [u8; 32],
    pub member_count: u32,
    pub close_ms: u64,
    pub seed_ready_deadline_ms: u64,
    pub beacon_round: u64,
}

impl Swc1StageDescriptorV1 {
    pub fn encode(&self) -> Result<Vec<u8>, Swc1ErrorV1> {
        if self.version != 1 {
            return Err(Swc1ErrorV1::InvalidVersion(self.version));
        }
        if !matches!(self.stage_kind, SWC1_STAGE_JOB_V1 | SWC1_STAGE_TICKET_V1 | SWC1_STAGE_PANEL_V1) {
            return Err(Swc1ErrorV1::InvalidStageKind(self.stage_kind));
        }
        if self.seed_ready_deadline_ms <= self.close_ms {
            return Err(Swc1ErrorV1::InvalidStageWindow {
                close_ms: self.close_ms,
                seed_ready_deadline_ms: self.seed_ready_deadline_ms,
            });
        }
        let mut out = Vec::with_capacity(200);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.push(self.stage_kind);
        out.extend_from_slice(&self.network_id);
        out.extend_from_slice(&self.policy_id);
        out.extend_from_slice(&self.stage_epoch.to_le_bytes());
        out.extend_from_slice(&self.parent_freeze_id);
        out.extend_from_slice(&self.canonical_state_cut);
        out.extend_from_slice(&self.frozen_set_root);
        out.extend_from_slice(&self.member_count.to_le_bytes());
        out.extend_from_slice(&self.close_ms.to_le_bytes());
        out.extend_from_slice(&self.seed_ready_deadline_ms.to_le_bytes());
        out.extend_from_slice(&self.beacon_round.to_le_bytes());
        Ok(out)
    }

    pub fn freeze_id(&self) -> Result<[u8; 32], Swc1ErrorV1> {
        let encoded = self.encode()?;
        let mut buf = Vec::with_capacity(4 + TAG_SWC1_FREEZE_V1.len() + encoded.len());
        lp_append_checked_v1(&mut buf, TAG_SWC1_FREEZE_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.extend_from_slice(&encoded);
        Ok(swc1_h256_v1(&buf))
    }

    pub fn beacon_message(&self) -> Result<[u8; 32], Swc1ErrorV1> {
        let fid = self.freeze_id()?;
        let mut buf = Vec::with_capacity(4 + TAG_SWC1_BEACON_MESSAGE_V1.len() + 32 + 8);
        lp_append_checked_v1(&mut buf, TAG_SWC1_BEACON_MESSAGE_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.extend_from_slice(&fid);
        buf.extend_from_slice(&self.beacon_round.to_le_bytes());
        Ok(swc1_h256_v1(&buf))
    }
}

/// Beacon source profile (`RFC-0058 §6.2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Swc1BeaconSourceProfileV1 {
    /// Disabled permanently under PQ-only / DNS-retirement principles (`RFC-0058 §6.2`).
    ThresholdBls12381G2Disabled,
    /// Post-close canonical DAG cut hash for private/test/PQ-native networks.
    PostCloseDagCutHashV1 { confirm_delay_ms: u64 },
}

/// Recorded stage beacon seed (`R_job`, `R_ticket`, or `R_panel`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1StageSeedRecordV1 {
    pub freeze_id: [u8; 32],
    pub beacon_round: u64,
    pub beacon_message: [u8; 32],
    pub seed: Swc1TypedDigestV1,
    pub arrived_ms: u64,
    pub source_profile: Swc1BeaconSourceProfileV1,
}

/// Seed derivation profile for `JobForSeed(reg, JobSeed)` (`RFC-0058 §7.4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum Swc1SeedDerivationProfileV1 {
    /// Profile 1: Fixed registered prompt committed before `R_job`.
    FixedRegisteredPrompt = 1,
    /// Profile 2: Seeded slice draw from a pre-committed prompt/dataset catalog root.
    SeededCatalogSlice = 2,
    /// Profile 3: Seeded perturbation / decode-challenge draw over pre-committed input.
    SeededPerturbation = 3,
}

impl Swc1SeedDerivationProfileV1 {
    pub fn from_u16(v: u16) -> Result<Self, Swc1ErrorV1> {
        match v {
            1 => Ok(Self::FixedRegisteredPrompt),
            2 => Ok(Self::SeededCatalogSlice),
            3 => Ok(Self::SeededPerturbation),
            other => Err(Swc1ErrorV1::InvalidSeedDerivationProfile(other)),
        }
    }
}

/// Pre-seed job registration (`JobRegistration`, `RFC-0058 §7.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1JobRegistrationV1 {
    pub version: u16,
    pub network_id: [u8; 32],
    pub policy_id: [u8; 32],
    pub job_epoch: u64,
    pub route_family: u8,
    pub model_id: [u8; 32],
    pub runtime_profile_id: [u8; 32],
    pub canonical_input_commitment: [u8; 32],
    pub decode_policy_id: [u8; 32],
    pub tool_policy_id: [u8; 32],
    pub seed_derivation_profile_id: u16,
    pub capital_lock_ref: [u8; 32],
    pub capital_sequence: u64,
    pub preseal_fee_commitment: [u8; 32],
    pub max_ready_window_ms: u64,
    /// Actual locked capital in sompi backing `capital_lock_ref` (verified at registration).
    pub locked_capital_sompi: u64,
    /// Actual non-refundable preseal fee in sompi backing `preseal_fee_commitment`.
    pub paid_preseal_fee_sompi: u64,
}

impl Swc1JobRegistrationV1 {
    /// Compute `TrialSlotKey` (`MISAKA/PALW/CAD1/SLOT`).
    pub fn trial_slot_key(&self) -> Result<[u8; 32], Swc1ErrorV1> {
        let mut buf = Vec::with_capacity(256);
        lp_append_checked_v1(&mut buf, TAG_CAD1_SLOT_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        nr_append_checked_v1(&mut buf, "network", 1, &self.network_id)?;
        nr_append_checked_v1(&mut buf, "policy", 1, &self.policy_id)?;
        nr_append_checked_v1(&mut buf, "capital-lock", 1, &self.capital_lock_ref)?;
        nr_append_checked_v1(&mut buf, "capital-seq", 1, &self.capital_sequence.to_le_bytes())?;
        nr_append_checked_v1(&mut buf, "input-commit", 1, &self.canonical_input_commitment)?;
        Ok(swc1_h256_v1(&buf))
    }

    /// Canonical encoding of `JobRegistration` (`RFC-0058 §7.1`).
    pub fn encode(&self) -> Result<Vec<u8>, Swc1ErrorV1> {
        if self.version != 1 {
            return Err(Swc1ErrorV1::InvalidVersion(self.version));
        }
        let _ = Swc1SeedDerivationProfileV1::from_u16(self.seed_derivation_profile_id)?;
        let mut out = Vec::with_capacity(512);
        out.extend_from_slice(&self.version.to_le_bytes());
        nr_append_checked_v1(&mut out, "network", 1, &self.network_id)?;
        nr_append_checked_v1(&mut out, "policy", 1, &self.policy_id)?;
        nr_append_checked_v1(&mut out, "job-epoch", 1, &self.job_epoch.to_le_bytes())?;
        nr_append_checked_v1(&mut out, "route-family", 1, &[self.route_family])?;
        nr_append_checked_v1(&mut out, "model", 1, &self.model_id)?;
        nr_append_checked_v1(&mut out, "runtime-profile", 1, &self.runtime_profile_id)?;
        nr_append_checked_v1(&mut out, "input-commit", 1, &self.canonical_input_commitment)?;
        nr_append_checked_v1(&mut out, "decode-policy", 1, &self.decode_policy_id)?;
        nr_append_checked_v1(&mut out, "tool-policy", 1, &self.tool_policy_id)?;
        nr_append_checked_v1(&mut out, "seed-profile", 1, &self.seed_derivation_profile_id.to_le_bytes())?;
        nr_append_checked_v1(&mut out, "capital-lock", 1, &self.capital_lock_ref)?;
        nr_append_checked_v1(&mut out, "capital-seq", 1, &self.capital_sequence.to_le_bytes())?;
        nr_append_checked_v1(&mut out, "preseal-fee", 1, &self.preseal_fee_commitment)?;
        nr_append_checked_v1(&mut out, "max-ready-window", 1, &self.max_ready_window_ms.to_le_bytes())?;
        Ok(out)
    }

    /// Compute `RegistrationId` (`MISAKA/PALW/SWC1/REGISTRATION`).
    pub fn registration_id(&self) -> Result<[u8; 32], Swc1ErrorV1> {
        let encoded = self.encode()?;
        let mut buf = Vec::with_capacity(4 + TAG_SWC1_REGISTRATION_V1.len() + encoded.len());
        lp_append_checked_v1(&mut buf, TAG_SWC1_REGISTRATION_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.extend_from_slice(&encoded);
        Ok(swc1_h256_v1(&buf))
    }

    /// Compute `OpportunityId` (`MISAKA/PALW/SWC1/OPPORTUNITY`, `RFC-0058 §7.2`).
    pub fn opportunity_id(&self) -> Result<[u8; 32], Swc1ErrorV1> {
        let slot_key = self.trial_slot_key()?;
        let reg_id = self.registration_id()?;
        let mut buf = Vec::with_capacity(140);
        lp_append_checked_v1(&mut buf, TAG_SWC1_OPPORTUNITY_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        nr_append_checked_v1(&mut buf, "trial-slot-key", 1, &slot_key)?;
        nr_append_checked_v1(&mut buf, "registration-id", 1, &reg_id)?;
        Ok(swc1_h256_v1(&buf))
    }
}

/// Compute `JobSeed` (`MISAKA/PALW/SWC1/JOB-SEED`, `RFC-0058 §7.3`).
pub fn swc1_job_seed_v1(
    network_id: &[u8; 32],
    policy_id: &[u8; 32],
    job_freeze_id: &[u8; 32],
    r_job: &Swc1TypedDigestV1,
    opportunity_id: &[u8; 32],
) -> Result<[u8; 32], Swc1ErrorV1> {
    r_job.validate()?;
    let mut buf = Vec::with_capacity(280);
    lp_append_checked_v1(&mut buf, TAG_SWC1_JOB_SEED_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    nr_append_checked_v1(&mut buf, "network", 1, network_id)?;
    nr_append_checked_v1(&mut buf, "policy", 1, policy_id)?;
    nr_append_checked_v1(&mut buf, "job-freeze", 1, job_freeze_id)?;
    nr_append_checked_v1(&mut buf, "r-job", r_job.version, &r_job.bytes)?;
    nr_append_checked_v1(&mut buf, "opportunity", 1, opportunity_id)?;
    Ok(swc1_h256_v1(&buf))
}

/// Deterministic `JobForSeed(reg, JobSeed) -> ActualInputHash` (`RFC-0058 §7.4`).
pub fn swc1_job_for_seed_actual_input_v1(reg: &Swc1JobRegistrationV1, job_seed: &[u8; 32]) -> Result<[u8; 32], Swc1ErrorV1> {
    let profile = Swc1SeedDerivationProfileV1::from_u16(reg.seed_derivation_profile_id)?;
    let mut buf = Vec::with_capacity(256);
    lp_append_checked_v1(&mut buf, TAG_SWC1_ACTUAL_INPUT_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    nr_append_checked_v1(&mut buf, "seed-profile", 1, &(profile as u16).to_le_bytes())?;
    nr_append_checked_v1(&mut buf, "input-commit", 1, &reg.canonical_input_commitment)?;
    nr_append_checked_v1(&mut buf, "decode-policy", 1, &reg.decode_policy_id)?;
    nr_append_checked_v1(&mut buf, "tool-policy", 1, &reg.tool_policy_id)?;
    nr_append_checked_v1(&mut buf, "job-seed", 1, job_seed)?;
    Ok(swc1_h256_v1(&buf))
}

/// Pure computation identity fields (`CanonicalWorkFields`, `RFC-0058 §8.1`).
///
/// Explicitly excludes signature, producer alias/pubkey, reward recipient, fee/tip,
/// carrier header/nonce, target epoch, and execution output/trace roots.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1CanonicalWorkFieldsV1 {
    pub version: u16,
    pub network_id: [u8; 32],
    pub policy_id: [u8; 32],
    pub route_family: u8,
    pub model_id: [u8; 32],
    pub runtime_profile_id: [u8; 32],
    pub actual_input_hash: [u8; 32],
    pub decode_policy_id: [u8; 32],
    pub tool_policy_id: [u8; 32],
    pub step_count: u32,
    pub leaf_count: u32,
    pub execution_shape_id: [u8; 32],
    pub obligation_set_root: [u8; 32],
}

impl Swc1CanonicalWorkFieldsV1 {
    pub fn encode(&self) -> Result<Vec<u8>, Swc1ErrorV1> {
        if self.version != 1 {
            return Err(Swc1ErrorV1::InvalidVersion(self.version));
        }
        let mut out = Vec::with_capacity(480);
        out.extend_from_slice(&self.version.to_le_bytes());
        nr_append_checked_v1(&mut out, "network", 1, &self.network_id)?;
        nr_append_checked_v1(&mut out, "policy", 1, &self.policy_id)?;
        nr_append_checked_v1(&mut out, "route-family", 1, &[self.route_family])?;
        nr_append_checked_v1(&mut out, "model", 1, &self.model_id)?;
        nr_append_checked_v1(&mut out, "runtime-profile", 1, &self.runtime_profile_id)?;
        nr_append_checked_v1(&mut out, "actual-input", 1, &self.actual_input_hash)?;
        nr_append_checked_v1(&mut out, "decode-policy", 1, &self.decode_policy_id)?;
        nr_append_checked_v1(&mut out, "tool-policy", 1, &self.tool_policy_id)?;
        nr_append_checked_v1(&mut out, "step-count", 1, &self.step_count.to_le_bytes())?;
        nr_append_checked_v1(&mut out, "leaf-count", 1, &self.leaf_count.to_le_bytes())?;
        nr_append_checked_v1(&mut out, "exec-shape", 1, &self.execution_shape_id)?;
        nr_append_checked_v1(&mut out, "obligations", 1, &self.obligation_set_root)?;
        Ok(out)
    }

    /// Compute `ComputationalWorkKey` (`MISAKA/PALW/SWC1/WORK`).
    pub fn work_key(&self) -> Result<[u8; 32], Swc1ErrorV1> {
        let encoded = self.encode()?;
        let mut buf = Vec::with_capacity(4 + TAG_SWC1_WORK_V1.len() + encoded.len());
        lp_append_checked_v1(&mut buf, TAG_SWC1_WORK_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.extend_from_slice(&encoded);
        Ok(swc1_h256_v1(&buf))
    }
}

/// Committed Original result fields (`CanonicalOriginalFields`, `RFC-0058 §8.2`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1CanonicalOriginalFieldsV1 {
    pub version: u16,
    pub opportunity_id: [u8; 32],
    pub job_freeze_id: [u8; 32],
    pub job_seed: [u8; 32],
    pub work_key: [u8; 32],
    pub producer_principal_id: [u8; 32],
    pub trace_root: [u8; 32],
    pub output_root: [u8; 32],
    pub transcript_root: [u8; 32],
    pub resource_descriptor_id: [u8; 32],
    pub readiness_root: [u8; 32],
    pub escrow_lock_id: [u8; 32],
    pub ready_at_ms: u64,
    pub hard_expiry_ms: u64,
}

impl Swc1CanonicalOriginalFieldsV1 {
    pub fn encode(&self) -> Result<Vec<u8>, Swc1ErrorV1> {
        if self.version != 1 {
            return Err(Swc1ErrorV1::InvalidVersion(self.version));
        }
        let mut out = Vec::with_capacity(560);
        out.extend_from_slice(&self.version.to_le_bytes());
        nr_append_checked_v1(&mut out, "opportunity", 1, &self.opportunity_id)?;
        nr_append_checked_v1(&mut out, "job-freeze", 1, &self.job_freeze_id)?;
        nr_append_checked_v1(&mut out, "job-seed", 1, &self.job_seed)?;
        nr_append_checked_v1(&mut out, "work-key", 1, &self.work_key)?;
        nr_append_checked_v1(&mut out, "producer", 1, &self.producer_principal_id)?;
        nr_append_checked_v1(&mut out, "trace-root", 1, &self.trace_root)?;
        nr_append_checked_v1(&mut out, "output-root", 1, &self.output_root)?;
        nr_append_checked_v1(&mut out, "transcript-root", 1, &self.transcript_root)?;
        nr_append_checked_v1(&mut out, "resource-desc", 1, &self.resource_descriptor_id)?;
        nr_append_checked_v1(&mut out, "readiness-root", 1, &self.readiness_root)?;
        nr_append_checked_v1(&mut out, "escrow-lock", 1, &self.escrow_lock_id)?;
        nr_append_checked_v1(&mut out, "ready-at-ms", 1, &self.ready_at_ms.to_le_bytes())?;
        nr_append_checked_v1(&mut out, "hard-expiry-ms", 1, &self.hard_expiry_ms.to_le_bytes())?;
        Ok(out)
    }

    /// Compute `OriginalId` (`MISAKA/PALW/SWC1/ORIGINAL`).
    pub fn original_id(&self) -> Result<[u8; 32], Swc1ErrorV1> {
        let encoded = self.encode()?;
        let mut buf = Vec::with_capacity(4 + TAG_SWC1_ORIGINAL_V1.len() + encoded.len());
        lp_append_checked_v1(&mut buf, TAG_SWC1_ORIGINAL_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        buf.extend_from_slice(&encoded);
        Ok(swc1_h256_v1(&buf))
    }
}

/// Compute `WorkTicket` bytes (`H256`, big-endian 256-bit integer `uint256_be`, `RFC-0058 §8.3`).
#[allow(clippy::too_many_arguments)]
pub fn swc1_work_ticket_bytes_v1(
    network_id: &[u8; 32],
    policy_id: &[u8; 32],
    ticket_epoch: u64,
    original_freeze_id: &[u8; 32],
    r_ticket: &Swc1TypedDigestV1,
    opportunity_id: &[u8; 32],
    work_key: &[u8; 32],
    original_id: &[u8; 32],
) -> Result<[u8; 32], Swc1ErrorV1> {
    r_ticket.validate()?;
    let mut buf = Vec::with_capacity(400);
    lp_append_checked_v1(&mut buf, TAG_SWC1_WORK_TICKET_V1, SWC1_MAX_FIELD_BYTES_V1)?;
    nr_append_checked_v1(&mut buf, "network", 1, network_id)?;
    nr_append_checked_v1(&mut buf, "policy", 1, policy_id)?;
    nr_append_checked_v1(&mut buf, "ticket-epoch", 1, &ticket_epoch.to_le_bytes())?;
    nr_append_checked_v1(&mut buf, "original-freeze", 1, original_freeze_id)?;
    nr_append_checked_v1(&mut buf, "r-ticket", r_ticket.version, &r_ticket.bytes)?;
    nr_append_checked_v1(&mut buf, "opportunity", 1, opportunity_id)?;
    nr_append_checked_v1(&mut buf, "work-key", 1, work_key)?;
    nr_append_checked_v1(&mut buf, "original-id", 1, original_id)?;
    Ok(swc1_h256_v1(&buf))
}

/// Compute exact 256-bit big-endian threshold `T(D) = floor((2^256 - 1) / D)` for `1 <= D <= 2^120 - 1`.
pub fn swc1_difficulty_threshold_be_v1(difficulty_d: u128) -> Result<[u8; 32], Swc1ErrorV1> {
    if difficulty_d == 0 || difficulty_d > SWC1_MAX_DIFFICULTY_D_V1 {
        return Err(Swc1ErrorV1::InvalidDifficulty(difficulty_d));
    }
    if difficulty_d == 1 {
        return Ok([0xFFu8; 32]);
    }
    // Long division of (2^256 - 1) = [0xFF; 32] in base 2^8 by `difficulty_d` (fits in u128, < 2^120).
    let mut out = [0u8; 32];
    let mut rem: u128 = 0;
    for slot in &mut out {
        rem = (rem << 8) | 0xFFu128;
        let q = rem / difficulty_d;
        rem %= difficulty_d;
        *slot = q as u8;
    }
    Ok(out)
}

/// Evaluate `AdmissionEligible(ticket_be, D) <=> uint256_be(ticket_be) <= T(D)`.
#[inline]
pub fn swc1_ticket_meets_difficulty_v1(ticket_be: &[u8; 32], difficulty_d: u128) -> Result<bool, Swc1ErrorV1> {
    let threshold_be = swc1_difficulty_threshold_be_v1(difficulty_d)?;
    Ok(ticket_be <= &threshold_be)
}

/// Canonical rank `(WorkTicket, OriginalId)` ordered lexicographically (`RFC-0058 §8.3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Swc1CandidateRankV1 {
    pub work_ticket_be: [u8; 32],
    pub original_id: [u8; 32],
}

/// Verification stage state machine (`RFC-0058 §10.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Swc1VerificationStageV1 {
    Registered,
    Committed,
    TicketEligible,
    VerificationPending,
    VerifiedPendingMaturity,
    MatureEligible,
    Refuted,
    AbortedUnverified,
    LateEvidenceStopsNewUses,
}

impl Swc1VerificationStageV1 {
    /// Only `MatureEligible` may contribute to consensus eligibility, DAA, weight, frontier, Rule E, or credits (`RFC-0058 §10.2`).
    #[inline]
    pub const fn is_consensus_eligible(self) -> bool {
        matches!(self, Self::MatureEligible)
    }
}

/// Ledger 1 (`TrialUse[OpportunityId]`, `RFC-0058 §9.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Swc1TrialUseStatusV1 {
    Registered,
    Committed { original_id: [u8; 32], work_key: [u8; 32] },
    ExpiredUncommitted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1TrialUseEntryV1 {
    pub registration: Swc1JobRegistrationV1,
    pub registration_id: [u8; 32],
    pub trial_slot_key: [u8; 32],
    pub opportunity_id: [u8; 32],
    pub registered_at_ms: u64,
    pub status: Swc1TrialUseStatusV1,
}

/// Ledger 3 (`StageUse[(stage_kind, stage_epoch)]`, `RFC-0058 §9.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1StageUseEntryV1 {
    pub descriptor: Swc1StageDescriptorV1,
    pub freeze_id: [u8; 32],
    pub member_ids: BTreeSet<[u8; 32]>,
    pub seed_record: Option<Swc1StageSeedRecordV1>,
}

/// Ledger 4 (`OriginalState[OriginalId]`, `RFC-0058 §9.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1OriginalLifecycleRecordV1 {
    pub original_id: [u8; 32],
    pub opportunity_id: [u8; 32],
    pub job_epoch: u64,
    pub job_freeze_id: [u8; 32],
    pub job_seed: [u8; 32],
    pub work_fields: Swc1CanonicalWorkFieldsV1,
    pub work_key: [u8; 32],
    pub original_fields: Swc1CanonicalOriginalFieldsV1,
    pub stage: Swc1VerificationStageV1,
    pub ticket_epoch: Option<u64>,
    pub original_freeze_id: Option<[u8; 32]>,
    pub work_ticket_be: Option<[u8; 32]>,
    pub panel_freeze_id: Option<[u8; 32]>,
    pub r_panel: Option<Swc1TypedDigestV1>,
    pub verified_at_daa: Option<u64>,
    pub maturity_cut_daa: Option<u64>,
    pub revocation_generation: u64,
}

/// Right kinds tracked by Ledger 5 (`RightsUse`, `RFC-0058 §9.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Swc1RightKindV1 {
    WorkReward,
    DaaWorkSample,
    SafeWeight,
    LiveWeight,
    SafeFrontier,
    SettlementDepth,
    RuleEParticipation,
    SettledAnchor,
    ExecCreditMint,
    ExecCreditReserve,
    ExecCreditConsume,
    CarrierPermit,
}

/// Ledger 5 (`RightsUse[(RightKind, SourceOriginalId, RightSubKey)]`, `RFC-0058 §9.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1RightsUseRecordV1 {
    pub kind: Swc1RightKindV1,
    pub source_original_id: [u8; 32],
    pub source_work_key: [u8; 32],
    pub sub_key: [u8; 32],
    pub consumed_at_daa: u64,
    pub state_cut: [u8; 32],
}

/// Complete state of the 5 one-time ledgers and pruning floor (`RFC-0058 §9`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1LedgerStateV1 {
    /// Ledger 1: `TrialUse[OpportunityId]`
    pub trial_use: BTreeMap<[u8; 32], Swc1TrialUseEntryV1>,
    /// Ledger 2: `WorkUse[(job_freeze_id, ComputationalWorkKey)] -> OriginalId`
    pub work_use: BTreeMap<([u8; 32], [u8; 32]), [u8; 32]>,
    /// Ledger 3: `StageUse[(stage_kind, stage_epoch)]`
    pub stage_use: BTreeMap<(u8, u64), Swc1StageUseEntryV1>,
    /// Ledger 4: `OriginalState[OriginalId]`
    pub original_state: BTreeMap<[u8; 32], Swc1OriginalLifecycleRecordV1>,
    /// Ledger 5: `RightsUse[(RightKind, SourceOriginalId, SubKey)]`
    pub rights_use: BTreeMap<(Swc1RightKindV1, [u8; 32], [u8; 32]), Swc1RightsUseRecordV1>,
    /// Capital-scoped sequence highwater surviving pruning/GC (`RFC-0058 §9.2`).
    pub capital_sequence_highwater: BTreeMap<[u8; 32], u64>,
    /// Highest expired job epoch whose detailed `TrialUse`/`WorkUse` rows have been pruned.
    pub expired_job_epoch_floor: Option<u64>,
    /// Authenticated rolling summary commitment over all pruned namespaces (`RFC-0058 §9.2`).
    pub expired_namespace_summary_root: [u8; 32],
    /// Monotone global revocation generation incremented whenever any Original is refuted or late-stopped.
    pub global_revocation_generation: u64,
}

impl Swc1LedgerStateV1 {
    pub fn new() -> Self {
        Self::default()
    }

    /// Atomic batch registration of `JobRegistration` opportunities (`RFC-0058 §7.1–§7.2, §9.2`).
    ///
    /// If any registration in the batch fails capital/fee/floor/sequence/uniqueness validation,
    /// the entire batch rolls back with zero state changes.
    pub fn register_opportunities_batch(
        &mut self,
        registrations: &[Swc1JobRegistrationV1],
        registered_at_ms: u64,
        min_capital_lock_sompi: u64,
        min_preseal_fee_sompi: u64,
    ) -> Result<Vec<[u8; 32]>, Swc1ErrorV1> {
        let mut staged = self.clone();
        let mut out_ids = Vec::with_capacity(registrations.len());
        for reg in registrations {
            if let Some(floor) = staged.expired_job_epoch_floor
                && reg.job_epoch <= floor
            {
                return Err(Swc1ErrorV1::ExpiredJobEpochFloor { job_epoch: reg.job_epoch, floor_epoch: floor });
            }
            if staged.stage_use.contains_key(&(SWC1_STAGE_JOB_V1, reg.job_epoch)) {
                return Err(Swc1ErrorV1::StageAlreadyFrozen { stage_kind: SWC1_STAGE_JOB_V1, stage_epoch: reg.job_epoch });
            }
            if reg.locked_capital_sompi < min_capital_lock_sompi {
                return Err(Swc1ErrorV1::InsufficientCapitalLock {
                    locked_sompi: reg.locked_capital_sompi,
                    required_sompi: min_capital_lock_sompi,
                });
            }
            if reg.paid_preseal_fee_sompi < min_preseal_fee_sompi {
                return Err(Swc1ErrorV1::InsufficientPresealFee {
                    fee_sompi: reg.paid_preseal_fee_sompi,
                    required_sompi: min_preseal_fee_sompi,
                });
            }
            let prev_seq = staged.capital_sequence_highwater.get(&reg.capital_lock_ref).copied().unwrap_or(0);
            if reg.capital_sequence <= prev_seq {
                return Err(Swc1ErrorV1::CapitalSequenceNotMonotone {
                    capital_lock_ref: reg.capital_lock_ref,
                    sequence: reg.capital_sequence,
                    highwater: prev_seq,
                });
            }
            let registration_id = reg.registration_id()?;
            let trial_slot_key = reg.trial_slot_key()?;
            let opportunity_id = reg.opportunity_id()?;
            if staged.trial_use.contains_key(&opportunity_id) {
                return Err(Swc1ErrorV1::TrialOpportunityAlreadyRegistered(opportunity_id));
            }
            staged.capital_sequence_highwater.insert(reg.capital_lock_ref, reg.capital_sequence);
            staged.trial_use.insert(
                opportunity_id,
                Swc1TrialUseEntryV1 {
                    registration: reg.clone(),
                    registration_id,
                    trial_slot_key,
                    opportunity_id,
                    registered_at_ms,
                    status: Swc1TrialUseStatusV1::Registered,
                },
            );
            out_ids.push(opportunity_id);
        }
        *self = staged;
        Ok(out_ids)
    }

    /// Freeze a stage cohort (`STAGE_JOB`, `STAGE_TICKET`, or `STAGE_PANEL`, `RFC-0058 §6.1`).
    #[allow(clippy::too_many_arguments)]
    pub fn freeze_stage(
        &mut self,
        stage_kind: u8,
        network_id: [u8; 32],
        policy_id: [u8; 32],
        stage_epoch: u64,
        parent_freeze_id: [u8; 32],
        canonical_state_cut: [u8; 32],
        member_ids: &[[u8; 32]],
        close_ms: u64,
        seed_ready_deadline_ms: u64,
        beacon_round: u64,
    ) -> Result<[u8; 32], Swc1ErrorV1> {
        if self.stage_use.contains_key(&(stage_kind, stage_epoch)) {
            return Err(Swc1ErrorV1::StageAlreadyFrozen { stage_kind, stage_epoch });
        }
        match stage_kind {
            SWC1_STAGE_JOB_V1 => {
                if parent_freeze_id != [0u8; 32] {
                    return Err(Swc1ErrorV1::ParentStageNotFrozen { stage_kind, expected_parent: [0u8; 32] });
                }
                for opp_id in member_ids {
                    if !self.trial_use.contains_key(opp_id) {
                        return Err(Swc1ErrorV1::TrialOpportunityNotFound(*opp_id));
                    }
                }
            }
            SWC1_STAGE_TICKET_V1 => {
                let job_stage = self
                    .stage_use
                    .values()
                    .find(|entry| entry.descriptor.stage_kind == SWC1_STAGE_JOB_V1 && entry.freeze_id == parent_freeze_id)
                    .ok_or(Swc1ErrorV1::ParentStageNotFrozen { stage_kind, expected_parent: parent_freeze_id })?;
                if job_stage.seed_record.is_none() {
                    return Err(Swc1ErrorV1::ParentStageSeedMissing { stage_kind, parent_freeze_id });
                }
                for orig_id in member_ids {
                    let orig = self.original_state.get(orig_id).ok_or(Swc1ErrorV1::OriginalNotFound(*orig_id))?;
                    if orig.job_freeze_id != parent_freeze_id {
                        return Err(Swc1ErrorV1::JobFreezeMismatch { expected: parent_freeze_id, got: orig.job_freeze_id });
                    }
                }
            }
            SWC1_STAGE_PANEL_V1 => {
                let ticket_stage = self
                    .stage_use
                    .values()
                    .find(|entry| entry.descriptor.stage_kind == SWC1_STAGE_TICKET_V1 && entry.freeze_id == parent_freeze_id)
                    .ok_or(Swc1ErrorV1::ParentStageNotFrozen { stage_kind, expected_parent: parent_freeze_id })?;
                if ticket_stage.seed_record.is_none() {
                    return Err(Swc1ErrorV1::ParentStageSeedMissing { stage_kind, parent_freeze_id });
                }
                for orig_id in member_ids {
                    let orig = self.original_state.get(orig_id).ok_or(Swc1ErrorV1::OriginalNotFound(*orig_id))?;
                    if orig.stage != Swc1VerificationStageV1::TicketEligible
                        && orig.stage != Swc1VerificationStageV1::VerificationPending
                    {
                        return Err(Swc1ErrorV1::OriginalNotTicketEligible(*orig_id));
                    }
                }
            }
            other => return Err(Swc1ErrorV1::InvalidStageKind(other)),
        }

        let unique_members: BTreeSet<[u8; 32]> = member_ids.iter().copied().collect();
        let sorted_vec: Vec<[u8; 32]> = unique_members.iter().copied().collect();
        let set_root = swc1_frozen_set_root_v1(stage_kind, &sorted_vec)?;
        let descriptor = Swc1StageDescriptorV1 {
            version: 1,
            stage_kind,
            network_id,
            policy_id,
            stage_epoch,
            parent_freeze_id,
            canonical_state_cut,
            frozen_set_root: set_root,
            member_count: sorted_vec.len() as u32,
            close_ms,
            seed_ready_deadline_ms,
            beacon_round,
        };
        let freeze_id = descriptor.freeze_id()?;
        self.stage_use.insert(
            (stage_kind, stage_epoch),
            Swc1StageUseEntryV1 { descriptor, freeze_id, member_ids: unique_members, seed_record: None },
        );
        Ok(freeze_id)
    }

    /// Record the post-close beacon seed (`R_job`, `R_ticket`, or `R_panel`) for a frozen stage (`RFC-0058 §6.2–§6.3`).
    pub fn record_stage_seed(
        &mut self,
        stage_kind: u8,
        stage_epoch: u64,
        freeze_id: [u8; 32],
        beacon_message: [u8; 32],
        seed: Swc1TypedDigestV1,
        arrived_ms: u64,
        source_profile: Swc1BeaconSourceProfileV1,
    ) -> Result<(), Swc1ErrorV1> {
        seed.validate()?;
        let confirm_delay_ms = match source_profile {
            Swc1BeaconSourceProfileV1::ThresholdBls12381G2Disabled => {
                return Err(Swc1ErrorV1::BeaconProfileDisabled);
            }
            Swc1BeaconSourceProfileV1::PostCloseDagCutHashV1 { confirm_delay_ms } => confirm_delay_ms,
        };
        let entry =
            self.stage_use.get_mut(&(stage_kind, stage_epoch)).ok_or(Swc1ErrorV1::StageNotFrozen { stage_kind, stage_epoch })?;
        if entry.freeze_id != freeze_id {
            return Err(Swc1ErrorV1::StageFreezeMismatch { expected: entry.freeze_id, got: freeze_id });
        }
        if entry.seed_record.is_some() {
            return Err(Swc1ErrorV1::SeedAlreadyRecorded { stage_kind, stage_epoch });
        }
        let expected_msg = entry.descriptor.beacon_message()?;
        if beacon_message != expected_msg {
            return Err(Swc1ErrorV1::SeedBeaconMessageMismatch { expected: expected_msg, got: beacon_message });
        }
        if arrived_ms < entry.descriptor.close_ms {
            return Err(Swc1ErrorV1::SeedArrivedBeforeClose { arrived_ms, close_ms: entry.descriptor.close_ms });
        }
        if arrived_ms < entry.descriptor.close_ms.saturating_add(confirm_delay_ms) {
            return Err(Swc1ErrorV1::SeedConfirmDelayInsufficient {
                arrived_ms,
                close_ms: entry.descriptor.close_ms,
                required_delay_ms: confirm_delay_ms,
            });
        }
        if arrived_ms > entry.descriptor.seed_ready_deadline_ms {
            return Err(Swc1ErrorV1::SeedArrivedAfterDeadline { arrived_ms, deadline_ms: entry.descriptor.seed_ready_deadline_ms });
        }
        entry.seed_record = Some(Swc1StageSeedRecordV1 {
            freeze_id,
            beacon_round: entry.descriptor.beacon_round,
            beacon_message,
            seed,
            arrived_ms,
            source_profile,
        });
        Ok(())
    }

    /// Atomic batch commitment of `Original` execution results (`RFC-0058 §8.1–§8.2, §9.1`).
    ///
    /// Enforces:
    /// - `OpportunityId` exists in `TrialUse` and is in `Registered` state (one-time per opportunity).
    /// - `OpportunityId` was included in the frozen `STAGE_JOB` set with matching `job_freeze_id` and `R_job`.
    /// - `JobSeed` and `ActualInputHash` (`JobForSeed`) match the deterministic derivation from `R_job`.
    /// - `ComputationalWorkKey` is unique within `job_freeze_id` (`WorkUse` ledger, including intra-batch duplicates).
    /// - Whole batch rolls back if any item fails.
    pub fn commit_originals_batch(
        &mut self,
        items: &[(Swc1CanonicalWorkFieldsV1, Swc1CanonicalOriginalFieldsV1)],
    ) -> Result<Vec<([u8; 32], [u8; 32])>, Swc1ErrorV1> {
        let mut staged = self.clone();
        let mut committed_ids = Vec::with_capacity(items.len());
        for (work_fields, original_fields) in items {
            let trial = staged
                .trial_use
                .get(&original_fields.opportunity_id)
                .ok_or(Swc1ErrorV1::TrialOpportunityNotFound(original_fields.opportunity_id))?
                .clone();
            if trial.status != Swc1TrialUseStatusV1::Registered {
                return Err(Swc1ErrorV1::TrialOpportunityAlreadyUsed(original_fields.opportunity_id));
            }
            let job_epoch = trial.registration.job_epoch;
            let job_stage = staged
                .stage_use
                .get(&(SWC1_STAGE_JOB_V1, job_epoch))
                .ok_or(Swc1ErrorV1::StageNotFrozen { stage_kind: SWC1_STAGE_JOB_V1, stage_epoch: job_epoch })?;
            if job_stage.freeze_id != original_fields.job_freeze_id {
                return Err(Swc1ErrorV1::JobFreezeMismatch { expected: job_stage.freeze_id, got: original_fields.job_freeze_id });
            }
            if !job_stage.member_ids.contains(&original_fields.opportunity_id) {
                return Err(Swc1ErrorV1::OriginalNotInFrozenSet(original_fields.opportunity_id));
            }
            let r_job = job_stage
                .seed_record
                .as_ref()
                .ok_or(Swc1ErrorV1::ParentStageSeedMissing { stage_kind: SWC1_STAGE_JOB_V1, parent_freeze_id: job_stage.freeze_id })?
                .seed
                .clone();
            let expected_job_seed = swc1_job_seed_v1(
                &trial.registration.network_id,
                &trial.registration.policy_id,
                &job_stage.freeze_id,
                &r_job,
                &original_fields.opportunity_id,
            )?;
            if original_fields.job_seed != expected_job_seed {
                return Err(Swc1ErrorV1::JobSeedMismatch { expected: expected_job_seed, got: original_fields.job_seed });
            }
            let expected_actual_input = swc1_job_for_seed_actual_input_v1(&trial.registration, &expected_job_seed)?;
            if work_fields.actual_input_hash != expected_actual_input
                || work_fields.network_id != trial.registration.network_id
                || work_fields.policy_id != trial.registration.policy_id
                || work_fields.route_family != trial.registration.route_family
                || work_fields.model_id != trial.registration.model_id
                || work_fields.runtime_profile_id != trial.registration.runtime_profile_id
                || work_fields.decode_policy_id != trial.registration.decode_policy_id
                || work_fields.tool_policy_id != trial.registration.tool_policy_id
            {
                return Err(Swc1ErrorV1::ActualInputMismatch { expected: expected_actual_input, got: work_fields.actual_input_hash });
            }
            let max_ready_ms = trial.registered_at_ms.saturating_add(trial.registration.max_ready_window_ms);
            if original_fields.ready_at_ms > max_ready_ms {
                return Err(Swc1ErrorV1::ReadyWindowExceeded { ready_at_ms: original_fields.ready_at_ms, max_ready_ms });
            }
            let work_key = work_fields.work_key()?;
            if original_fields.work_key != work_key {
                return Err(Swc1ErrorV1::ActualInputMismatch { expected: work_key, got: original_fields.work_key });
            }
            if let Some(existing_orig) = staged.work_use.get(&(original_fields.job_freeze_id, work_key)) {
                return Err(Swc1ErrorV1::DuplicateWorkKeyInCohort {
                    job_freeze_id: original_fields.job_freeze_id,
                    work_key,
                    existing_original_id: *existing_orig,
                });
            }
            let original_id = original_fields.original_id()?;
            if staged.original_state.contains_key(&original_id) {
                return Err(Swc1ErrorV1::DuplicateOriginalId(original_id));
            }
            staged.trial_use.get_mut(&original_fields.opportunity_id).expect("checked above").status =
                Swc1TrialUseStatusV1::Committed { original_id, work_key };
            staged.work_use.insert((original_fields.job_freeze_id, work_key), original_id);
            staged.original_state.insert(
                original_id,
                Swc1OriginalLifecycleRecordV1 {
                    original_id,
                    opportunity_id: original_fields.opportunity_id,
                    job_epoch,
                    job_freeze_id: original_fields.job_freeze_id,
                    job_seed: expected_job_seed,
                    work_fields: work_fields.clone(),
                    work_key,
                    original_fields: original_fields.clone(),
                    stage: Swc1VerificationStageV1::Committed,
                    ticket_epoch: None,
                    original_freeze_id: None,
                    work_ticket_be: None,
                    panel_freeze_id: None,
                    r_panel: None,
                    verified_at_daa: None,
                    maturity_cut_daa: None,
                    revocation_generation: 0,
                },
            );
            committed_ids.push((original_id, work_key));
        }
        *self = staged;
        Ok(committed_ids)
    }

    /// Evaluate `WorkTicket` eligibility for all Originals in a frozen `STAGE_TICKET` cohort (`RFC-0058 §8.3`).
    ///
    /// Returns the sorted canonical rank list `Vec<Swc1CandidateRankV1>` of eligible Originals.
    pub fn evaluate_ticket_cohort(&mut self, ticket_epoch: u64, difficulty_d: u128) -> Result<Vec<Swc1CandidateRankV1>, Swc1ErrorV1> {
        let _threshold = swc1_difficulty_threshold_be_v1(difficulty_d)?;
        let ticket_stage = self
            .stage_use
            .get(&(SWC1_STAGE_TICKET_V1, ticket_epoch))
            .ok_or(Swc1ErrorV1::StageNotFrozen { stage_kind: SWC1_STAGE_TICKET_V1, stage_epoch: ticket_epoch })?
            .clone();
        let r_ticket = ticket_stage
            .seed_record
            .as_ref()
            .ok_or(Swc1ErrorV1::ParentStageSeedMissing { stage_kind: SWC1_STAGE_TICKET_V1, parent_freeze_id: ticket_stage.freeze_id })?
            .seed
            .clone();
        let mut eligible_ranks = Vec::new();
        for orig_id in &ticket_stage.member_ids {
            let rec = self.original_state.get_mut(orig_id).ok_or(Swc1ErrorV1::OriginalNotFound(*orig_id))?;
            let ticket_be = swc1_work_ticket_bytes_v1(
                &rec.work_fields.network_id,
                &rec.work_fields.policy_id,
                ticket_epoch,
                &ticket_stage.freeze_id,
                &r_ticket,
                &rec.opportunity_id,
                &rec.work_key,
                &rec.original_id,
            )?;
            rec.ticket_epoch = Some(ticket_epoch);
            rec.original_freeze_id = Some(ticket_stage.freeze_id);
            rec.work_ticket_be = Some(ticket_be);
            if swc1_ticket_meets_difficulty_v1(&ticket_be, difficulty_d)? {
                rec.stage = Swc1VerificationStageV1::TicketEligible;
                eligible_ranks.push(Swc1CandidateRankV1 { work_ticket_be: ticket_be, original_id: *orig_id });
            } else {
                rec.stage = Swc1VerificationStageV1::AbortedUnverified;
            }
        }
        eligible_ranks.sort();
        Ok(eligible_ranks)
    }

    /// Bind post-admission `R_panel` to admitted Originals in a frozen `STAGE_PANEL` cohort (`RFC-0058 §5.2, §6.1`).
    pub fn bind_panel_cohort(&mut self, panel_epoch: u64) -> Result<Vec<[u8; 32]>, Swc1ErrorV1> {
        let panel_stage = self
            .stage_use
            .get(&(SWC1_STAGE_PANEL_V1, panel_epoch))
            .ok_or(Swc1ErrorV1::StageNotFrozen { stage_kind: SWC1_STAGE_PANEL_V1, stage_epoch: panel_epoch })?
            .clone();
        let r_panel = panel_stage
            .seed_record
            .as_ref()
            .ok_or(Swc1ErrorV1::ParentStageSeedMissing { stage_kind: SWC1_STAGE_PANEL_V1, parent_freeze_id: panel_stage.freeze_id })?
            .seed
            .clone();
        let mut bound = Vec::with_capacity(panel_stage.member_ids.len());
        for orig_id in &panel_stage.member_ids {
            let rec = self.original_state.get_mut(orig_id).ok_or(Swc1ErrorV1::OriginalNotFound(*orig_id))?;
            if !matches!(rec.stage, Swc1VerificationStageV1::TicketEligible | Swc1VerificationStageV1::VerificationPending) {
                return Err(Swc1ErrorV1::OriginalNotTicketEligible(*orig_id));
            }
            rec.panel_freeze_id = Some(panel_stage.freeze_id);
            rec.r_panel = Some(r_panel.clone());
            rec.stage = Swc1VerificationStageV1::VerificationPending;
            bound.push(*orig_id);
        }
        Ok(bound)
    }

    /// Advance an Original's verification lifecycle stage (`RFC-0058 §10.1`).
    pub fn transition_original_stage(
        &mut self,
        original_id: [u8; 32],
        next_stage: Swc1VerificationStageV1,
        current_daa: u64,
        maturity_window_daa: u64,
    ) -> Result<(), Swc1ErrorV1> {
        let next_revocation_gen = self.global_revocation_generation.saturating_add(1);
        let rec = self.original_state.get_mut(&original_id).ok_or(Swc1ErrorV1::OriginalNotFound(original_id))?;
        let valid = match (rec.stage, next_stage) {
            (Swc1VerificationStageV1::TicketEligible, Swc1VerificationStageV1::VerificationPending)
            | (Swc1VerificationStageV1::TicketEligible, Swc1VerificationStageV1::AbortedUnverified)
            | (Swc1VerificationStageV1::VerificationPending, Swc1VerificationStageV1::VerifiedPendingMaturity)
            | (Swc1VerificationStageV1::VerificationPending, Swc1VerificationStageV1::Refuted)
            | (Swc1VerificationStageV1::VerificationPending, Swc1VerificationStageV1::AbortedUnverified)
            | (Swc1VerificationStageV1::VerifiedPendingMaturity, Swc1VerificationStageV1::MatureEligible)
            | (Swc1VerificationStageV1::VerifiedPendingMaturity, Swc1VerificationStageV1::Refuted)
            | (Swc1VerificationStageV1::VerifiedPendingMaturity, Swc1VerificationStageV1::AbortedUnverified)
            | (Swc1VerificationStageV1::MatureEligible, Swc1VerificationStageV1::Refuted)
            | (Swc1VerificationStageV1::MatureEligible, Swc1VerificationStageV1::LateEvidenceStopsNewUses) => true,
            _ => false,
        };
        if !valid {
            return Err(Swc1ErrorV1::InvalidOriginalStateTransition { original_id, from: rec.stage, to: next_stage });
        }
        if next_stage == Swc1VerificationStageV1::VerifiedPendingMaturity {
            rec.verified_at_daa = Some(current_daa);
            rec.maturity_cut_daa = Some(current_daa.saturating_add(maturity_window_daa));
        }
        if next_stage == Swc1VerificationStageV1::MatureEligible {
            if let Some(cut) = rec.maturity_cut_daa
                && current_daa < cut
            {
                return Err(Swc1ErrorV1::InvalidOriginalStateTransition { original_id, from: rec.stage, to: next_stage });
            }
        }
        if matches!(
            next_stage,
            Swc1VerificationStageV1::Refuted
                | Swc1VerificationStageV1::AbortedUnverified
                | Swc1VerificationStageV1::LateEvidenceStopsNewUses
        ) {
            self.global_revocation_generation = next_revocation_gen;
            rec.revocation_generation = next_revocation_gen;
        }
        rec.stage = next_stage;
        Ok(())
    }

    /// Consume a one-time right in Ledger 5 (`RightsUse`, `RFC-0058 §9.1, §10.2`).
    ///
    /// Requires the source Original to be in `MatureEligible` state and the `(kind, source_original_id, sub_key)`
    /// tuple to be unused.
    pub fn consume_right_once(
        &mut self,
        kind: Swc1RightKindV1,
        source_original_id: [u8; 32],
        sub_key: [u8; 32],
        consumed_at_daa: u64,
        state_cut: [u8; 32],
    ) -> Result<Swc1RightsUseRecordV1, Swc1ErrorV1> {
        let orig = self.original_state.get(&source_original_id).ok_or(Swc1ErrorV1::OriginalNotFound(source_original_id))?;
        if !orig.stage.is_consensus_eligible() {
            return Err(Swc1ErrorV1::OriginalNotMatureEligible { original_id: source_original_id, current_stage: orig.stage });
        }
        let key = (kind, source_original_id, sub_key);
        if self.rights_use.contains_key(&key) {
            return Err(Swc1ErrorV1::RightAlreadyConsumed { kind, source_original_id, sub_key });
        }
        let record =
            Swc1RightsUseRecordV1 { kind, source_original_id, source_work_key: orig.work_key, sub_key, consumed_at_daa, state_cut };
        self.rights_use.insert(key, record.clone());
        Ok(record)
    }

    /// Prune expired job epochs up to `prune_through_job_epoch` while preserving the authenticated
    /// expired-epoch floor and summary root (`RFC-0058 §9.2`).
    pub fn prune_expired_job_epochs(&mut self, prune_through_job_epoch: u64) -> Result<usize, Swc1ErrorV1> {
        let mut pruned_items = Vec::new();
        let mut to_remove_trials = Vec::new();
        for (opp_id, entry) in &self.trial_use {
            if entry.registration.job_epoch <= prune_through_job_epoch {
                let can_prune = match entry.status {
                    Swc1TrialUseStatusV1::Registered | Swc1TrialUseStatusV1::ExpiredUncommitted => true,
                    Swc1TrialUseStatusV1::Committed { original_id, .. } => self.original_state.get(&original_id).is_none_or(|orig| {
                        matches!(orig.stage, Swc1VerificationStageV1::Refuted | Swc1VerificationStageV1::AbortedUnverified)
                    }),
                };
                if can_prune {
                    let mut leaf = Vec::with_capacity(72);
                    leaf.extend_from_slice(&entry.registration.job_epoch.to_le_bytes());
                    leaf.extend_from_slice(opp_id);
                    leaf.extend_from_slice(&entry.registration_id);
                    pruned_items.push(swc1_h256_v1(&leaf));
                    to_remove_trials.push(*opp_id);
                }
            }
        }
        for opp_id in &to_remove_trials {
            if let Some(removed) = self.trial_use.remove(opp_id)
                && let Swc1TrialUseStatusV1::Committed { original_id, work_key } = removed.status
                && let Some(orig) = self.original_state.remove(&original_id)
            {
                self.work_use.remove(&(orig.job_freeze_id, work_key));
            }
        }
        pruned_items.sort();
        let batch_root = swc1_frozen_set_root_v1(SWC1_STAGE_JOB_V1, &pruned_items)?;
        let mut summary_buf = Vec::with_capacity(120);
        lp_append_checked_v1(&mut summary_buf, TAG_SWC1_PRUNED_SUMMARY_V1, SWC1_MAX_FIELD_BYTES_V1)?;
        summary_buf.extend_from_slice(&self.expired_namespace_summary_root);
        summary_buf.extend_from_slice(&prune_through_job_epoch.to_le_bytes());
        summary_buf.extend_from_slice(&batch_root);
        self.expired_namespace_summary_root = swc1_h256_v1(&summary_buf);
        self.expired_job_epoch_floor =
            Some(self.expired_job_epoch_floor.map_or(prune_through_job_epoch, |prev| prev.max(prune_through_job_epoch)));
        Ok(to_remove_trials.len())
    }
}

/// Bridge connecting `PALW-AN1` (`RFC-0059` / `RFC-0058 §16`) with `SWC1` staged work commitments:
/// - Does NOT layer a second `R_ticket` lottery over `AN1` (`RFC-0058 §16.1`).
/// - Separates `slot_id` / `s` (`AttemptUse`), `ComputationalWorkKey` (bound to actual `x`, `WorkUse`),
///   and `BaseWorkKey` (bound to original user request `J` before challenge insertion, `BaseWorkUse`) (`RFC-0058 §16.2`).
/// - Explicitly preserves `c_min_llm_cost_proven == false` (`RFC-0058 §16.7`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swc1An1StageBridgeV1 {
    pub an1_ledger: crate::palw_an1_input_binding_and_panel_court_v1::An1LedgerStateV1,
}

impl Swc1An1StageBridgeV1 {
    pub fn new() -> Self {
        Self { an1_ledger: crate::palw_an1_input_binding_and_panel_court_v1::An1LedgerStateV1::default() }
    }

    /// True if physical LLM cost lower bound `c_min` is proven (`RFC-0058 §16.7`: always `false`, never claimed).
    #[inline]
    pub const fn is_physical_llm_cost_lower_bound_proven(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_registration(seq: u64, job_epoch: u64, input_byte: u8) -> Swc1JobRegistrationV1 {
        Swc1JobRegistrationV1 {
            version: 1,
            network_id: [0x11; 32],
            policy_id: [0x22; 32],
            job_epoch,
            route_family: 1,
            model_id: [0x33; 32],
            runtime_profile_id: [0x44; 32],
            canonical_input_commitment: [input_byte; 32],
            decode_policy_id: [0x55; 32],
            tool_policy_id: [0x66; 32],
            seed_derivation_profile_id: 1,
            capital_lock_ref: [0x77; 32],
            capital_sequence: seq,
            preseal_fee_commitment: [0x88; 32],
            max_ready_window_ms: 60_000,
            locked_capital_sompi: 10_000_000,
            paid_preseal_fee_sompi: 100_000,
        }
    }

    #[test]
    fn swc1_causal_chain_four_identifiers_and_five_ledgers_enforced() {
        let mut state = Swc1LedgerStateV1::new();
        let reg1 = sample_registration(1, 10, 0xAA);
        let reg2 = sample_registration(2, 10, 0xAA); // Same input, distinct funded slot!

        let opp_ids = state
            .register_opportunities_batch(&[reg1.clone(), reg2.clone()], 1_000, 1_000_000, 10_000)
            .expect("batch registration must succeed");
        assert_eq!(opp_ids.len(), 2);
        assert_ne!(opp_ids[0], opp_ids[1]);

        // Freeze STAGE_JOB at epoch 10
        let job_freeze_id = state
            .freeze_stage(SWC1_STAGE_JOB_V1, reg1.network_id, reg1.policy_id, 10, [0u8; 32], [0x99; 32], &opp_ids, 2_000, 5_000, 100)
            .expect("job freeze");
        let job_beacon_msg = state.stage_use[&(SWC1_STAGE_JOB_V1, 10)].descriptor.beacon_message().unwrap();

        // Threshold BLS profile is permanently disabled
        assert_eq!(
            state.record_stage_seed(
                SWC1_STAGE_JOB_V1,
                10,
                job_freeze_id,
                job_beacon_msg,
                Swc1TypedDigestV1::new64([0xAB; 64]),
                2_500,
                Swc1BeaconSourceProfileV1::ThresholdBls12381G2Disabled,
            ),
            Err(Swc1ErrorV1::BeaconProfileDisabled)
        );

        let r_job = Swc1TypedDigestV1::new64([0xAB; 64]);
        state
            .record_stage_seed(
                SWC1_STAGE_JOB_V1,
                10,
                job_freeze_id,
                job_beacon_msg,
                r_job.clone(),
                2_500,
                Swc1BeaconSourceProfileV1::PostCloseDagCutHashV1 { confirm_delay_ms: 200 },
            )
            .expect("record R_job");

        let job_seed_1 = swc1_job_seed_v1(&reg1.network_id, &reg1.policy_id, &job_freeze_id, &r_job, &opp_ids[0]).unwrap();
        let actual_input_1 = swc1_job_for_seed_actual_input_v1(&reg1, &job_seed_1).unwrap();
        let work_fields_1 = Swc1CanonicalWorkFieldsV1 {
            version: 1,
            network_id: reg1.network_id,
            policy_id: reg1.policy_id,
            route_family: reg1.route_family,
            model_id: reg1.model_id,
            runtime_profile_id: reg1.runtime_profile_id,
            actual_input_hash: actual_input_1,
            decode_policy_id: reg1.decode_policy_id,
            tool_policy_id: reg1.tool_policy_id,
            step_count: 64,
            leaf_count: 1024,
            execution_shape_id: [0xCC; 32],
            obligation_set_root: [0xDD; 32],
        };
        let work_key_1 = work_fields_1.work_key().unwrap();
        let orig_fields_1 = Swc1CanonicalOriginalFieldsV1 {
            version: 1,
            opportunity_id: opp_ids[0],
            job_freeze_id,
            job_seed: job_seed_1,
            work_key: work_key_1,
            producer_principal_id: [0xEE; 32],
            trace_root: [0x01; 32],
            output_root: [0x02; 32],
            transcript_root: [0x03; 32],
            resource_descriptor_id: [0x04; 32],
            readiness_root: [0x05; 32],
            escrow_lock_id: [0x06; 32],
            ready_at_ms: 3_000,
            hard_expiry_ms: 90_000,
        };
        let committed = state.commit_originals_batch(&[(work_fields_1.clone(), orig_fields_1.clone())]).expect("commit original 1");
        let (orig_id_1, _) = committed[0];

        // Re-using the same OpportunityId with a different output root must fail (TrialUse one-time ledger)
        let mut reroll_orig = orig_fields_1.clone();
        reroll_orig.output_root = [0x99; 32];
        assert!(matches!(
            state.commit_originals_batch(&[(work_fields_1.clone(), reroll_orig)]),
            Err(Swc1ErrorV1::TrialOpportunityAlreadyUsed(_))
        ));

        // Freeze STAGE_TICKET and evaluate WorkTicket at D=1
        let ticket_freeze_id = state
            .freeze_stage(
                SWC1_STAGE_TICKET_V1,
                reg1.network_id,
                reg1.policy_id,
                10,
                job_freeze_id,
                [0x9A; 32],
                &[orig_id_1],
                6_000,
                9_000,
                101,
            )
            .expect("ticket freeze");
        let ticket_beacon_msg = state.stage_use[&(SWC1_STAGE_TICKET_V1, 10)].descriptor.beacon_message().unwrap();
        state
            .record_stage_seed(
                SWC1_STAGE_TICKET_V1,
                10,
                ticket_freeze_id,
                ticket_beacon_msg,
                Swc1TypedDigestV1::new64([0xBC; 64]),
                6_500,
                Swc1BeaconSourceProfileV1::PostCloseDagCutHashV1 { confirm_delay_ms: 200 },
            )
            .expect("record R_ticket");

        let ranks = state.evaluate_ticket_cohort(10, 1).expect("evaluate ticket cohort");
        assert_eq!(ranks.len(), 1);
        assert_eq!(ranks[0].original_id, orig_id_1);

        // Freeze STAGE_PANEL and bind R_panel
        let panel_freeze_id = state
            .freeze_stage(
                SWC1_STAGE_PANEL_V1,
                reg1.network_id,
                reg1.policy_id,
                10,
                ticket_freeze_id,
                [0x9B; 32],
                &[orig_id_1],
                10_000,
                14_000,
                102,
            )
            .expect("panel freeze");
        let panel_beacon_msg = state.stage_use[&(SWC1_STAGE_PANEL_V1, 10)].descriptor.beacon_message().unwrap();
        state
            .record_stage_seed(
                SWC1_STAGE_PANEL_V1,
                10,
                panel_freeze_id,
                panel_beacon_msg,
                Swc1TypedDigestV1::new64([0xCD; 64]),
                10_500,
                Swc1BeaconSourceProfileV1::PostCloseDagCutHashV1 { confirm_delay_ms: 200 },
            )
            .expect("record R_panel");
        let bound = state.bind_panel_cohort(10).expect("bind panel cohort");
        assert_eq!(bound, vec![orig_id_1]);

        // Unverified candidate cannot consume any right yet
        assert!(matches!(
            state.consume_right_once(Swc1RightKindV1::WorkReward, orig_id_1, [0u8; 32], 100, [0x11; 32]),
            Err(Swc1ErrorV1::OriginalNotMatureEligible { .. })
        ));

        // Transition through VerifiedPendingMaturity -> MatureEligible
        state.transition_original_stage(orig_id_1, Swc1VerificationStageV1::VerifiedPendingMaturity, 200, 50).unwrap();
        // Before maturity cut (250) -> rejected
        assert!(state.transition_original_stage(orig_id_1, Swc1VerificationStageV1::MatureEligible, 249, 50).is_err());
        state.transition_original_stage(orig_id_1, Swc1VerificationStageV1::MatureEligible, 250, 50).unwrap();

        // Now right can be consumed once, and second consumption is rejected
        state.consume_right_once(Swc1RightKindV1::WorkReward, orig_id_1, [0u8; 32], 250, [0x11; 32]).expect("first consume");
        assert!(matches!(
            state.consume_right_once(Swc1RightKindV1::WorkReward, orig_id_1, [0u8; 32], 251, [0x11; 32]),
            Err(Swc1ErrorV1::RightAlreadyConsumed { .. })
        ));

        // Prune expired epoch 10 and verify replay of epoch <= 10 or sequence <= 2 is rejected
        state.prune_expired_job_epochs(10).expect("prune");
        assert!(matches!(
            state.register_opportunities_batch(&[sample_registration(3, 10, 0xBB)], 20_000, 1_000_000, 10_000),
            Err(Swc1ErrorV1::ExpiredJobEpochFloor { job_epoch: 10, floor_epoch: 10 })
        ));
        assert!(matches!(
            state.register_opportunities_batch(&[sample_registration(2, 11, 0xBB)], 20_000, 1_000_000, 10_000),
            Err(Swc1ErrorV1::CapitalSequenceNotMonotone { .. })
        ));
    }

    #[test]
    fn swc1_an1_stage_bridge_no_extra_r_ticket_and_work_vs_base_work_ledgers_an1_w01_to_w09() {
        use crate::palw_an1_input_binding_and_panel_court_v1::{
            An1PanelReviewOutcomeV1, An1RegistrationContextV1, An1TargetThreshold257V1, an1_build_claim_v1,
            an1_deterministic_weights_v1, an1_job_tree_v1, an1_model_tree_v1, an1_panel_review_v1, an1_profile_id_hex_v1,
        };

        let mut bridge = Swc1An1StageBridgeV1::new();
        assert!(!bridge.is_physical_llm_cost_lower_bound_proven());

        let weights = an1_deterministic_weights_v1(23);
        let job_tokens = vec![17i64, 25];
        let ctx = An1RegistrationContextV1 {
            accounting_version: "AN1-TEST-USEFUL-ONCE-1".to_owned(),
            beacon_key: "fixture-key-1".to_owned(),
            beacon_round: 13,
            chain: "an1-independent-test-chain".to_owned(),
            deadline: 300,
            epoch: 7,
            job_root: an1_job_tree_v1(&job_tokens).unwrap().root_hex(),
            model_root: an1_model_tree_v1(&weights).unwrap().root_hex(),
            nonce: "22".repeat(32),
            owner: "11".repeat(32),
            panel_event: 100,
            profile_id: an1_profile_id_hex_v1(),
            target: An1TargetThreshold257V1::MaxTwoPow256,
        };
        let sigma = [0x33u8; 32];
        let slot = bridge.an1_ledger.register(&ctx, &job_tokens, 10, &ctx.owner, "bond-1").unwrap();
        let (claim, material) = an1_build_claim_v1(&ctx, &sigma, &job_tokens, &weights).unwrap();
        bridge.an1_ledger.seal(&slot, &claim, 25, &ctx.owner).unwrap();
        let candidates: Vec<String> = (1..=6).map(|i| format!("panel-{i}")).collect();
        let selected = bridge.an1_ledger.select_panel(&slot, &[0x44u8; 32], 100, &candidates).unwrap();
        let native = bridge.an1_ledger.native(&slot).unwrap();
        let review = an1_panel_review_v1(&native, Some(&material));
        assert!(matches!(review, An1PanelReviewOutcomeV1::ApprovedFullReplay { .. }));
        bridge.an1_ledger.approve(&slot, &selected[0]).unwrap();
        {
            let row = bridge.an1_ledger.rows.get_mut(&slot).unwrap();
            row.materials_ready = true;
            row.closed = true;
            row.delivered = true;
        }
        let outcome = bridge.an1_ledger.apply_batch(&slot, &[], true, 120, &[]).unwrap();
        assert!(outcome.is_final);
        assert!(bridge.an1_ledger.consumed_slots.contains(&slot));
        assert_eq!(bridge.an1_ledger.work_consumed.len(), 1);
        assert_eq!(bridge.an1_ledger.useful_consumed.len(), 1);
    }
}
