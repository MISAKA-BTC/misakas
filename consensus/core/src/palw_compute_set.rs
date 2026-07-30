//! # Model-Agnostic Compute Set Registry — core types (Descriptor / Policy / Allocation Plan)
//!
//! Implements the consensus-core layer of `PALW_Model_Agnostic_Compute_Set_Architecture_v0.1`
//! (ADR-MA-001..007): an LLM is never node code, it is registered DATA — an immutable
//! [`PalwComputeSetDescriptorV2`] naming the bit-exact computation, a mutable
//! [`PalwComputeSetPolicyV1`] carrying the economics that may change over time, and an atomic
//! [`PalwModelAllocationPlanV1`] distributing PALW-lane block share across every active set.
//!
//! Design invariants enforced here (the rest of the stack builds on these):
//!
//! * **IDs are derived, never chosen** (§7.1, §10.2): `compute_set_id`, `compute_policy_id` and
//!   `plan_id` are keyed BLAKE2b-512 digests of the canonical (borsh) encoding under disjoint
//!   `misaka-palw-*` domains, following the repo-wide idiom (`Hash64_k(domain, borsh(obj))`).
//!   A self-referential id field is zeroed before hashing (the `batch_id` idiom).
//! * **Descriptors are write-once** (§7.2): absent → insert, byte-identical → idempotent,
//!   divergent bytes for the same id → consensus error. Updates are NEW sets (ADR-MA-004).
//! * **Policies resolve by source-DAA, never by current state** (§9, ADR-MA-007): the resolver is
//!   a pure function over the recorded history; nothing here reads clocks or tips.
//! * **Allocation is atomic** (§10, ADR-MA-003): one plan re-states EVERY entry; per-set patching
//!   does not exist as an operation.
//! * **Fail-closed** (§22.3): unknown states, unknown discriminants and out-of-range shares are
//!   decode/validation errors — there is no default model, default scale or default share.
//!
//! Wire note: [`ComputeSetState`] follows the `PalwProofType` precedent — the on-wire form is a
//! pinned plain `u8` (manual borsh, explicit discriminants), so the enum's declaration order can
//! never silently re-number persisted or hashed bytes.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash64, blake2b_512_keyed};
use thiserror::Error;

// =============================================================================================
// Keyed-hash domains (registered in `palw::tests::palw_keyed_domains_are_pairwise_distinct`).
// =============================================================================================

/// §7.1 — `compute_set_id = Hash64_k(compute-set-id, borsh(PalwComputeSetDescriptorV2))`.
pub const PALW_COMPUTE_SET_ID_DOMAIN: &[u8] = b"misaka-palw-compute-set-id-v2";

/// §15.4 — `compute_vm_id = Hash64_k(compute-vm-id, canonical VM surface)`; see `palw_compute_ir`.
pub const PALW_COMPUTE_VM_ID_DOMAIN: &[u8] = b"misaka-palw-compute-vm-id-v1";

/// §8/§13 — the EXACT policy-record id a header commits to: `Hash64_k(policy-id, borsh(policy))`.
pub const PALW_COMPUTE_POLICY_ID_DOMAIN: &[u8] = b"misaka-palw-compute-policy-id-v1";

/// §10.2 — `plan_id = Hash64_k(alloc-plan-id, borsh(plan with plan_id zeroed))`.
pub const PALW_ALLOCATION_PLAN_ID_DOMAIN: &[u8] = b"misaka-palw-alloc-plan-id-v1";

/// Basis-points denominator (§10.1 — `bps` here is ALWAYS basis points, never blocks/sec).
pub const BPS_DENOMINATOR: u16 = 10_000;

/// Version pins for the three record kinds.
pub const PALW_COMPUTE_SET_DESCRIPTOR_VERSION: u16 = 2;
pub const PALW_COMPUTE_SET_POLICY_VERSION: u16 = 1;
pub const PALW_MODEL_ALLOCATION_PLAN_VERSION: u16 = 1;

// =============================================================================================
// §7 — Immutable Compute Set Descriptor
// =============================================================================================

/// The immutable identity of one bit-exact LLM computation (§5.1/§7). Everything the replica /
/// auditor path must agree on byte-for-byte is committed here as a content root; changing ANY
/// root produces a different `compute_set_id`, i.e. a different Compute Set (ADR-MA-004).
///
/// The struct is intentionally all fixed-size commitments: the descriptor names artifacts, it
/// never carries them (§7.3 — weights live in the content-addressed DA/distribution layer).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeSetDescriptorV2 {
    pub version: u16,

    /// The Compute VM that interprets `semantic_program_root` (§15.4). A node that does not
    /// implement this VM id must fail closed (§22.3 `unsupported VM -> reject`).
    pub compute_vm_id: Hash64,

    /// Display/classification only — identity is the whole descriptor, never this field (§7).
    pub model_family_id: Hash64,

    /// Canonical model artifact content root.
    pub model_artifact_root: Hash64,
    /// Tensor inventory, topology, layer map.
    pub model_manifest_root: Hash64,

    pub tokenizer_root: Hash64,
    pub chat_template_root: Hash64,
    pub preprocessing_root: Hash64,
    pub decode_policy_root: Hash64,

    /// Compute IR bytecode / semantic DAG (§15).
    pub semantic_program_root: Hash64,

    pub shape_table_root: Hash64,
    pub shape_cost_table_root: Hash64,

    pub arithmetic_rules_root: Hash64,
    pub overflow_budget_root: Hash64,
    pub lut_root: Hash64,

    pub trace_policy_root: Hash64,
    pub checkpoint_policy_root: Hash64,
    pub conformance_vector_root: Hash64,

    /// Modality bitmask (text/vision/audio…). v1 governance restricts which bits may register.
    pub modality_mask: u32,

    /// Commits the size/instruction/tensor bounds the program payload was validated against.
    pub resource_limits_root: Hash64,
}

impl PalwComputeSetDescriptorV2 {
    /// §7.1 — the derived, never-chosen identity of this Compute Set. One flipped bit anywhere in
    /// the canonical encoding is a different set.
    pub fn compute_set_id(&self) -> Hash64 {
        blake2b_512_keyed(PALW_COMPUTE_SET_ID_DOMAIN, &borsh::to_vec(self).expect("borsh"))
    }

    /// Structural self-checks that need no registry context.
    pub fn validate_in_isolation(&self) -> Result<(), ComputeSetRegistryError> {
        if self.version != PALW_COMPUTE_SET_DESCRIPTOR_VERSION {
            return Err(ComputeSetRegistryError::UnsupportedDescriptorVersion(self.version));
        }
        if self.modality_mask == 0 {
            return Err(ComputeSetRegistryError::EmptyModalityMask);
        }
        Ok(())
    }
}

/// §7.2 — the write-once outcome for a descriptor registration attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorRegistrationOutcome {
    /// No prior record for this `compute_set_id`: insert.
    Inserted,
    /// A byte-identical record already exists: no-op, not an error.
    Idempotent,
}

/// §7.2 — pure write-once rule: `不存在 -> insert / 同一bytes -> idempotent / 異なるbytes -> error`.
///
/// `existing_canonical` is the canonical encoding already stored under the SAME `compute_set_id`
/// (if any). Divergent bytes under one id are cryptographically impossible unless the store is
/// corrupt or an implementation hashed a different preimage — either way consensus must stop
/// treating the id as well-defined, hence a hard error rather than any overwrite.
pub fn descriptor_registration_outcome(
    existing_canonical: Option<&[u8]>,
    incoming: &PalwComputeSetDescriptorV2,
) -> Result<DescriptorRegistrationOutcome, ComputeSetRegistryError> {
    incoming.validate_in_isolation()?;
    let incoming_bytes = borsh::to_vec(incoming).expect("borsh");
    match existing_canonical {
        None => Ok(DescriptorRegistrationOutcome::Inserted),
        Some(existing) if existing == incoming_bytes.as_slice() => Ok(DescriptorRegistrationOutcome::Idempotent),
        Some(_) => Err(ComputeSetRegistryError::DescriptorDiverged(incoming.compute_set_id())),
    }
}

// =============================================================================================
// §8.1 — Lifecycle state (wire form: pinned u8, fail-closed decode)
// =============================================================================================

/// Compute Set lifecycle (§8.1). Explicit discriminants; the borsh wire form is the plain `u8`,
/// so reordering the declaration can never re-number persisted bytes (the `PalwProofType`
/// precedent). Unknown discriminants fail decode — never a default state (§22.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum ComputeSetState {
    /// Registered proposal; unusable for jobs and mint.
    Proposed = 0,
    /// Executable + auditable, but share = 0, work credit = 0, reward = 0.
    Shadow = 1,
    /// Jobs, tickets, PALW blocks and work credit permitted.
    Active = 2,
    /// No NEW jobs; existing maturity/settlement and historical validation continue.
    Deprecated = 3,
    /// New tickets/blocks stop immediately; history stays valid (§18.6).
    EmergencyHalted = 4,
    /// Terminal after all maturity windows; history retained forever.
    Retired = 5,
}

impl ComputeSetState {
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[inline]
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Proposed),
            1 => Some(Self::Shadow),
            2 => Some(Self::Active),
            3 => Some(Self::Deprecated),
            4 => Some(Self::EmergencyHalted),
            5 => Some(Self::Retired),
            _ => None,
        }
    }

    /// §8.1/§18 transition matrix (§28.4). Same-state is always allowed — an economics-only
    /// policy revision keeps the state. `Retired` is terminal; re-activation from `Deprecated`
    /// or `EmergencyHalted` is representable and gated further by the sequence/future rules in
    /// [`validate_policy_progression`] (§9 「再有効化…より大きいsequenceとfuture activationを必須」).
    pub const fn can_transition_to(self, next: ComputeSetState) -> bool {
        use ComputeSetState::*;
        if self as u8 == next as u8 {
            return true;
        }
        matches!(
            (self, next),
            (Proposed, Shadow)
                | (Shadow, Active)
                | (Shadow, Retired)
                | (Active, Deprecated)
                | (Active, EmergencyHalted)
                | (Deprecated, Active)
                | (Deprecated, EmergencyHalted)
                | (Deprecated, Retired)
                | (EmergencyHalted, Active)
                | (EmergencyHalted, Deprecated)
                | (EmergencyHalted, Retired)
        )
    }
}

impl BorshSerialize for ComputeSetState {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        self.as_u8().serialize(writer)
    }
}

impl BorshDeserialize for ComputeSetState {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        let raw = u8::deserialize_reader(reader)?;
        ComputeSetState::from_u8(raw)
            .ok_or_else(|| borsh::io::Error::new(borsh::io::ErrorKind::InvalidData, format!("unknown ComputeSetState {raw}")))
    }
}

// =============================================================================================
// §8 — Mutable Compute Set Policy
// =============================================================================================

/// The time-varying economics/operations record for one Compute Set (§8). Identity of the model
/// never lives here (ADR-MA-002); a header commits the EXACT record via [`Self::policy_id`], so
/// historical work is reconstructed from immutable bytes, never from current values (ADR-MA-007).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeSetPolicyV1 {
    pub version: u16,

    pub compute_set_id: Hash64,
    pub policy_sequence: u64,
    pub effective_from_daa: u64,

    pub state: ComputeSetState,

    pub no_new_jobs_from_daa: Option<u64>,
    pub retired_from_daa: Option<u64>,

    /// Normalizes 1 quantum of this set's compute into hash-work units (§11).
    pub compute_work_scale: u64,

    /// Portion of the normalized compute work credited to the DAG, in basis points (§11).
    pub weight_factor_bps: u16,

    /// Common floor for provider/replica leaf bonds.
    pub min_leaf_bond_sompi: u64,

    pub job_timeout_daa: u64,
    pub receipt_retention_daa: u64,

    pub auditor_capacity_threshold: u32,

    /// Provider reward premium in basis points (§10.1 — never blocks/sec).
    pub premium_pi_bps: u16,

    pub max_prompt_tokens: u32,
    pub max_output_tokens: u32,
    pub allowed_shape_set_root: Hash64,
}

impl PalwComputeSetPolicyV1 {
    /// §13/§14 — the exact-record id a Header-v5 commits as `compute_policy_id`.
    pub fn policy_id(&self) -> Hash64 {
        blake2b_512_keyed(PALW_COMPUTE_POLICY_ID_DOMAIN, &borsh::to_vec(self).expect("borsh"))
    }
}

/// §22.2 — governed caps a policy revision is validated against. Carried as data (a Params-level
/// choice per network), defaulted to the v0.1 initial numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PolicyGovernanceCapsV1 {
    pub max_premium_pi_bps: u16,
}

impl Default for PolicyGovernanceCapsV1 {
    fn default() -> Self {
        Self { max_premium_pi_bps: 5_000 }
    }
}

/// §9 — resolve the policy in force for `set_id` at `daa_score`: the highest sequence among
/// records already effective. Pure over the recorded history; `None` = fail closed.
pub fn resolve_compute_policy<'a>(
    policies: &'a [PalwComputeSetPolicyV1],
    set_id: &Hash64,
    daa_score: u64,
) -> Option<&'a PalwComputeSetPolicyV1> {
    policies
        .iter()
        .filter(|policy| &policy.compute_set_id == set_id && policy.effective_from_daa <= daa_score)
        .max_by_key(|policy| policy.policy_sequence)
}

/// The idempotence carve-out of §9: re-submitting the SAME `(set, sequence)` with byte-identical
/// payload is a no-op, while a divergent payload for that key is a consensus error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyRegistrationOutcome {
    Inserted,
    Idempotent,
}

/// §9 + §22.2 — validate one policy revision against the set's recorded history.
///
/// * `descriptor_registered` — the §9 rule 「未登録DescriptorへのPolicy」→ reject.
/// * `highest_existing` — the record with the highest `policy_sequence` for this set (if any);
///   `same_sequence_existing` — a record already stored under `next.policy_sequence` (if any).
/// * `current_daa` — the DAA score of the block applying the revision (future-dating base).
pub fn validate_policy_progression(
    descriptor_registered: bool,
    highest_existing: Option<&PalwComputeSetPolicyV1>,
    same_sequence_existing: Option<&PalwComputeSetPolicyV1>,
    next: &PalwComputeSetPolicyV1,
    caps: &PolicyGovernanceCapsV1,
    current_daa: u64,
) -> Result<PolicyRegistrationOutcome, ComputeSetRegistryError> {
    if next.version != PALW_COMPUTE_SET_POLICY_VERSION {
        return Err(ComputeSetRegistryError::UnsupportedPolicyVersion(next.version));
    }
    if !descriptor_registered {
        return Err(ComputeSetRegistryError::PolicyForUnregisteredSet(next.compute_set_id));
    }
    // §22.2 field caps — enforced before any history comparison so a malformed record can never
    // become the "existing bytes" a later idempotence check normalizes against.
    if next.weight_factor_bps > BPS_DENOMINATOR {
        return Err(ComputeSetRegistryError::WeightFactorOutOfRange(next.weight_factor_bps));
    }
    if next.premium_pi_bps > caps.max_premium_pi_bps {
        return Err(ComputeSetRegistryError::PremiumAboveCap { premium_pi_bps: next.premium_pi_bps, cap: caps.max_premium_pi_bps });
    }
    match next.state {
        ComputeSetState::Active => {
            // §22.2: an Active set must have real economics — zero scale/bond/threshold would
            // mint credit or select auditors from nothing.
            if next.compute_work_scale == 0 {
                return Err(ComputeSetRegistryError::ActiveRequiresNonzero("compute_work_scale"));
            }
            if next.min_leaf_bond_sompi == 0 {
                return Err(ComputeSetRegistryError::ActiveRequiresNonzero("min_leaf_bond_sompi"));
            }
            if next.auditor_capacity_threshold == 0 {
                return Err(ComputeSetRegistryError::ActiveRequiresNonzero("auditor_capacity_threshold"));
            }
        }
        // §8.1: every non-Active stage carries zero DAG credit (Shadow soaks, Deprecated winds
        // down, Halted stops, Proposed/Retired never mint). `Canary Active / weight 0` (§11)
        // remains expressible — Active does not force nonzero weight.
        _ => {
            if next.weight_factor_bps != 0 {
                return Err(ComputeSetRegistryError::NonActiveRequiresZeroWeight(next.state));
            }
        }
    }

    // Same-sequence handling: byte-identical replay is idempotent, divergence is an error (§9).
    if let Some(existing) = same_sequence_existing {
        return if existing == next {
            Ok(PolicyRegistrationOutcome::Idempotent)
        } else {
            Err(ComputeSetRegistryError::PolicySequenceDiverged { set: next.compute_set_id, sequence: next.policy_sequence })
        };
    }

    match highest_existing {
        None => {
            // §18.1 — a set's life starts as a Proposal; the first policy record must say so.
            if next.state != ComputeSetState::Proposed {
                return Err(ComputeSetRegistryError::FirstPolicyMustBeProposed(next.state));
            }
        }
        Some(prev) => {
            if prev.state == ComputeSetState::Retired {
                // §9 「Retired後の不正な再有効化」/ §28.1 retired set reactivation.
                return Err(ComputeSetRegistryError::RetiredSetIsTerminal(next.compute_set_id));
            }
            if next.policy_sequence <= prev.policy_sequence {
                return Err(ComputeSetRegistryError::PolicySequenceRollback {
                    set: next.compute_set_id,
                    prev: prev.policy_sequence,
                    next: next.policy_sequence,
                });
            }
            if !prev.state.can_transition_to(next.state) {
                return Err(ComputeSetRegistryError::InvalidStateTransition { from: prev.state, to: next.state });
            }
        }
    }

    // §9 「effective_from_daaの不正な過去指定」+ §22.2 future-dated activation: a revision takes
    // effect strictly after the block that registers it, so no in-flight block re-resolves.
    if next.effective_from_daa <= current_daa {
        return Err(ComputeSetRegistryError::PolicyEffectiveNotFuture {
            effective_from_daa: next.effective_from_daa,
            current_daa,
        });
    }

    Ok(PolicyRegistrationOutcome::Inserted)
}

// =============================================================================================
// §10 — Atomic Model Allocation Plan
// =============================================================================================

/// One entry of an allocation plan: a Compute Set and its target PALW-lane block share in basis
/// points (§10.1 — basis points, NEVER blocks-per-second).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwModelAllocationEntryV1 {
    pub compute_set_id: Hash64,
    /// Target PALW-lane block share, `0..=10000`.
    pub target_share_bps: u16,
}

/// §10.2 — the atomic, whole-lane allocation statement (ADR-MA-003). A plan restates EVERY
/// tracked set (zero-share entries keep wound-down sets visible, cf. §10.3's `A = 0` example);
/// per-set patching does not exist.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwModelAllocationPlanV1 {
    pub version: u16,

    /// Derived (§10.2): zeroed in the id preimage; see [`Self::derive_plan_id`].
    pub plan_id: Hash64,
    pub sequence: u64,
    pub effective_from_daa: u64,

    pub entries: Vec<PalwModelAllocationEntryV1>,
}

impl PalwModelAllocationPlanV1 {
    /// §10.2 — `plan_id = Hash64_k(alloc-plan-id, borsh(plan with plan_id zeroed))`, the
    /// `batch_id` self-reference-zeroing idiom.
    pub fn derive_plan_id(&self) -> Hash64 {
        let mut canonical = self.clone();
        canonical.plan_id = Hash64::default();
        blake2b_512_keyed(PALW_ALLOCATION_PLAN_ID_DOMAIN, &borsh::to_vec(&canonical).expect("borsh"))
    }

    /// True when the carried `plan_id` equals the derived one (a chosen id is rejected, §7.1's
    /// rule applied to plans).
    pub fn plan_id_is_canonical(&self) -> bool {
        self.plan_id == self.derive_plan_id()
    }
}

/// §22.1 — governed allocation safety limits (initial v0.1 numbers as defaults).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationGovernanceLimitsV1 {
    pub max_active_sets: u16,
    pub min_nonzero_share_bps: u16,
    pub max_change_per_plan_bps: u16,
}

impl Default for AllocationGovernanceLimitsV1 {
    fn default() -> Self {
        Self { max_active_sets: 16, min_nonzero_share_bps: 100, max_change_per_plan_bps: 500 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanRegistrationOutcome {
    Inserted,
    Idempotent,
}

/// §10.3 + §22.1 — validate one allocation plan against the registry view and its predecessor.
///
/// * `set_state_of` — resolves a set's CURRENT lifecycle state (`None` = unregistered, §22.3
///   fail-closed).
/// * `prev` — the plan with the highest sequence so far (share baseline for the ramp limit; a
///   set absent from `prev` ramps from 0).
/// * `same_sequence_existing` — §10.3 「same sequence has one canonical payload」.
pub fn validate_allocation_plan(
    plan: &PalwModelAllocationPlanV1,
    set_state_of: &dyn Fn(&Hash64) -> Option<ComputeSetState>,
    prev: Option<&PalwModelAllocationPlanV1>,
    same_sequence_existing: Option<&PalwModelAllocationPlanV1>,
    limits: &AllocationGovernanceLimitsV1,
    current_daa: u64,
) -> Result<PlanRegistrationOutcome, ComputeSetRegistryError> {
    if plan.version != PALW_MODEL_ALLOCATION_PLAN_VERSION {
        return Err(ComputeSetRegistryError::UnsupportedPlanVersion(plan.version));
    }
    if !plan.plan_id_is_canonical() {
        return Err(ComputeSetRegistryError::PlanIdNotCanonical(plan.plan_id));
    }
    if plan.entries.is_empty() {
        return Err(ComputeSetRegistryError::EmptyPlan);
    }

    let mut share_sum: u64 = 0;
    let mut active_entries: u16 = 0;
    for (index, entry) in plan.entries.iter().enumerate() {
        // §10.3 duplicate detection — entries are few (≤ MAX_ACTIVE_SETS + wind-downs), so the
        // quadratic scan is cheaper than hashing and keeps this fn allocation-free.
        if plan.entries[..index].iter().any(|prior| prior.compute_set_id == entry.compute_set_id) {
            return Err(ComputeSetRegistryError::DuplicatePlanEntry(entry.compute_set_id));
        }
        let state = set_state_of(&entry.compute_set_id)
            .ok_or(ComputeSetRegistryError::PlanReferencesUnregisteredSet(entry.compute_set_id))?;
        if entry.target_share_bps > 0 {
            if state != ComputeSetState::Active {
                return Err(ComputeSetRegistryError::NonActiveSetWithShare { set: entry.compute_set_id, state });
            }
            if entry.target_share_bps < limits.min_nonzero_share_bps {
                return Err(ComputeSetRegistryError::ShareBelowFloor {
                    set: entry.compute_set_id,
                    share_bps: entry.target_share_bps,
                    floor_bps: limits.min_nonzero_share_bps,
                });
            }
            active_entries += 1;
        }
        share_sum += entry.target_share_bps as u64;
    }
    if share_sum != BPS_DENOMINATOR as u64 {
        return Err(ComputeSetRegistryError::ShareSumInvalid(share_sum));
    }
    if active_entries > limits.max_active_sets {
        return Err(ComputeSetRegistryError::TooManyActiveSets { count: active_entries, max: limits.max_active_sets });
    }

    if let Some(existing) = same_sequence_existing {
        return if existing == plan {
            Ok(PlanRegistrationOutcome::Idempotent)
        } else {
            Err(ComputeSetRegistryError::PlanSequenceDiverged(plan.sequence))
        };
    }

    if let Some(prev) = prev {
        if plan.sequence <= prev.sequence {
            return Err(ComputeSetRegistryError::PlanSequenceRollback { prev: prev.sequence, next: plan.sequence });
        }
        // §22.1 ramp limit — every set's |new − old| is bounded; absent-from-prev ramps from 0,
        // and DROPPING an entry is also a change to 0, so removed sets are checked too.
        let old_share = |set: &Hash64| -> u16 {
            prev.entries.iter().find(|e| &e.compute_set_id == set).map(|e| e.target_share_bps).unwrap_or(0)
        };
        for entry in &plan.entries {
            let old = old_share(&entry.compute_set_id);
            let delta = entry.target_share_bps.abs_diff(old);
            if delta > limits.max_change_per_plan_bps {
                return Err(ComputeSetRegistryError::RampLimitExceeded {
                    set: entry.compute_set_id,
                    delta_bps: delta,
                    limit_bps: limits.max_change_per_plan_bps,
                });
            }
        }
        for old_entry in &prev.entries {
            if !plan.entries.iter().any(|e| e.compute_set_id == old_entry.compute_set_id) && old_entry.target_share_bps > 0 {
                return Err(ComputeSetRegistryError::RampLimitExceeded {
                    set: old_entry.compute_set_id,
                    delta_bps: old_entry.target_share_bps,
                    limit_bps: limits.max_change_per_plan_bps,
                });
            }
        }
    }

    if plan.effective_from_daa <= current_daa {
        return Err(ComputeSetRegistryError::PlanEffectiveNotFuture { effective_from_daa: plan.effective_from_daa, current_daa });
    }

    Ok(PlanRegistrationOutcome::Inserted)
}

/// §9-style DAA-point resolution for plans: highest sequence among plans already effective.
pub fn resolve_allocation_plan<'a>(
    plans: &'a [PalwModelAllocationPlanV1],
    daa_score: u64,
) -> Option<&'a PalwModelAllocationPlanV1> {
    plans.iter().filter(|plan| plan.effective_from_daa <= daa_score).max_by_key(|plan| plan.sequence)
}

// =============================================================================================
// Errors
// =============================================================================================

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ComputeSetRegistryError {
    #[error("descriptor version {0} is not supported (expected {PALW_COMPUTE_SET_DESCRIPTOR_VERSION})")]
    UnsupportedDescriptorVersion(u16),

    #[error("descriptor modality mask is empty")]
    EmptyModalityMask,

    #[error("descriptor bytes diverge for existing compute_set_id {0} — descriptors are write-once (§7.2)")]
    DescriptorDiverged(Hash64),

    #[error("policy version {0} is not supported (expected {PALW_COMPUTE_SET_POLICY_VERSION})")]
    UnsupportedPolicyVersion(u16),

    #[error("policy targets unregistered compute set {0} (§9 fail-closed)")]
    PolicyForUnregisteredSet(Hash64),

    #[error("weight_factor_bps {0} exceeds 10000")]
    WeightFactorOutOfRange(u16),

    #[error("premium_pi_bps {premium_pi_bps} exceeds governed cap {cap}")]
    PremiumAboveCap { premium_pi_bps: u16, cap: u16 },

    #[error("Active policy requires nonzero {0} (§22.2)")]
    ActiveRequiresNonzero(&'static str),

    #[error("state {0:?} requires weight_factor_bps == 0 — only Active sets earn DAG credit (§8.1)")]
    NonActiveRequiresZeroWeight(ComputeSetState),

    #[error("first policy for a set must be Proposed, got {0:?} (§18.1)")]
    FirstPolicyMustBeProposed(ComputeSetState),

    #[error("compute set {0} is Retired — terminal, no further policy revisions (§9)")]
    RetiredSetIsTerminal(Hash64),

    #[error("policy sequence rollback for set {set}: prev {prev}, next {next} (§9)")]
    PolicySequenceRollback { set: Hash64, prev: u64, next: u64 },

    #[error("policy (set {set}, sequence {sequence}) already exists with different payload (§9)")]
    PolicySequenceDiverged { set: Hash64, sequence: u64 },

    #[error("invalid lifecycle transition {from:?} -> {to:?} (§8.1)")]
    InvalidStateTransition { from: ComputeSetState, to: ComputeSetState },

    #[error("policy effective_from_daa {effective_from_daa} is not in the future of {current_daa} (§9/§22.2)")]
    PolicyEffectiveNotFuture { effective_from_daa: u64, current_daa: u64 },

    #[error("allocation plan version {0} is not supported (expected {PALW_MODEL_ALLOCATION_PLAN_VERSION})")]
    UnsupportedPlanVersion(u16),

    #[error("plan_id {0} does not match the canonical derivation (§10.2)")]
    PlanIdNotCanonical(Hash64),

    #[error("allocation plan has no entries")]
    EmptyPlan,

    #[error("allocation plan lists compute set {0} more than once (§10.3)")]
    DuplicatePlanEntry(Hash64),

    #[error("allocation plan references unregistered compute set {0} (§10.3/§22.3)")]
    PlanReferencesUnregisteredSet(Hash64),

    #[error("set {set} has share > 0 while {state:?} — only Active sets may hold share (§10.3)")]
    NonActiveSetWithShare { set: Hash64, state: ComputeSetState },

    #[error("set {set} share {share_bps} bps is below the governed floor {floor_bps} bps (§22.1)")]
    ShareBelowFloor { set: Hash64, share_bps: u16, floor_bps: u16 },

    #[error("allocation shares sum to {0}, expected exactly 10000 (§10.3)")]
    ShareSumInvalid(u64),

    #[error("{count} active entries exceed MAX_ACTIVE_SETS {max} (§22.1)")]
    TooManyActiveSets { count: u16, max: u16 },

    #[error("allocation plan sequence {0} already exists with different payload (§10.3)")]
    PlanSequenceDiverged(u64),

    #[error("allocation plan sequence rollback: prev {prev}, next {next} (§10.3)")]
    PlanSequenceRollback { prev: u64, next: u64 },

    #[error("set {set} share change {delta_bps} bps exceeds ramp limit {limit_bps} bps (§22.1)")]
    RampLimitExceeded { set: Hash64, delta_bps: u16, limit_bps: u16 },

    #[error("plan effective_from_daa {effective_from_daa} is not in the future of {current_daa} (§10.3)")]
    PlanEffectiveNotFuture { effective_from_daa: u64, current_daa: u64 },
}

// =============================================================================================
// Tests (§28.1 registry / §28.2 allocation / §28.4 lifecycle — the core-layer halves)
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn h(tag: u8) -> Hash64 {
        Hash64::from_bytes([tag; 64])
    }

    fn descriptor(tag: u8) -> PalwComputeSetDescriptorV2 {
        PalwComputeSetDescriptorV2 {
            version: PALW_COMPUTE_SET_DESCRIPTOR_VERSION,
            compute_vm_id: h(tag),
            model_family_id: h(0x10),
            model_artifact_root: h(0x11),
            model_manifest_root: h(0x12),
            tokenizer_root: h(0x13),
            chat_template_root: h(0x14),
            preprocessing_root: h(0x15),
            decode_policy_root: h(0x16),
            semantic_program_root: h(0x17),
            shape_table_root: h(0x18),
            shape_cost_table_root: h(0x19),
            arithmetic_rules_root: h(0x1a),
            overflow_budget_root: h(0x1b),
            lut_root: h(0x1c),
            trace_policy_root: h(0x1d),
            checkpoint_policy_root: h(0x1e),
            conformance_vector_root: h(0x1f),
            modality_mask: 1,
            resource_limits_root: h(0x20),
        }
    }

    fn policy(set: Hash64, sequence: u64, effective: u64, state: ComputeSetState) -> PalwComputeSetPolicyV1 {
        let active = state == ComputeSetState::Active;
        PalwComputeSetPolicyV1 {
            version: PALW_COMPUTE_SET_POLICY_VERSION,
            compute_set_id: set,
            policy_sequence: sequence,
            effective_from_daa: effective,
            state,
            no_new_jobs_from_daa: None,
            retired_from_daa: None,
            compute_work_scale: if active { 7 } else { 0 },
            weight_factor_bps: 0,
            min_leaf_bond_sompi: if active { 1_000_000 } else { 0 },
            job_timeout_daa: 600,
            receipt_retention_daa: 86_400,
            auditor_capacity_threshold: if active { 2 } else { 0 },
            premium_pi_bps: 0,
            max_prompt_tokens: 4096,
            max_output_tokens: 4096,
            allowed_shape_set_root: h(0x30),
        }
    }

    fn plan(sequence: u64, effective: u64, entries: Vec<(Hash64, u16)>) -> PalwModelAllocationPlanV1 {
        let mut plan = PalwModelAllocationPlanV1 {
            version: PALW_MODEL_ALLOCATION_PLAN_VERSION,
            plan_id: Hash64::default(),
            sequence,
            effective_from_daa: effective,
            entries: entries
                .into_iter()
                .map(|(compute_set_id, target_share_bps)| PalwModelAllocationEntryV1 { compute_set_id, target_share_bps })
                .collect(),
        };
        plan.plan_id = plan.derive_plan_id();
        plan
    }

    #[test]
    fn domains_are_pinned_distinct_and_fit_key_limit() {
        assert_eq!(PALW_COMPUTE_SET_ID_DOMAIN, b"misaka-palw-compute-set-id-v2");
        assert_eq!(PALW_COMPUTE_VM_ID_DOMAIN, b"misaka-palw-compute-vm-id-v1");
        assert_eq!(PALW_COMPUTE_POLICY_ID_DOMAIN, b"misaka-palw-compute-policy-id-v1");
        assert_eq!(PALW_ALLOCATION_PLAN_ID_DOMAIN, b"misaka-palw-alloc-plan-id-v1");
        let domains = [
            PALW_COMPUTE_SET_ID_DOMAIN,
            PALW_COMPUTE_VM_ID_DOMAIN,
            PALW_COMPUTE_POLICY_ID_DOMAIN,
            PALW_ALLOCATION_PLAN_ID_DOMAIN,
        ];
        for (i, a) in domains.iter().enumerate() {
            assert!(a.len() <= 64, "domain exceeds BLAKE2b key limit");
            for b in domains.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn compute_set_id_is_content_derived() {
        let a = descriptor(1);
        let mut b = descriptor(1);
        assert_eq!(a.compute_set_id(), b.compute_set_id());
        // One changed root — e.g. a re-quantization (§18.3) — is a different Compute Set.
        b.lut_root = h(0x77);
        assert_ne!(a.compute_set_id(), b.compute_set_id());
    }

    #[test]
    fn descriptor_write_once() {
        let a = descriptor(1);
        let bytes = borsh::to_vec(&a).unwrap();
        assert_eq!(descriptor_registration_outcome(None, &a), Ok(DescriptorRegistrationOutcome::Inserted));
        assert_eq!(descriptor_registration_outcome(Some(&bytes), &a), Ok(DescriptorRegistrationOutcome::Idempotent));
        let mut divergent = a.clone();
        divergent.tokenizer_root = h(0x99);
        assert!(matches!(
            descriptor_registration_outcome(Some(&bytes), &divergent),
            Err(ComputeSetRegistryError::DescriptorDiverged(_))
        ));
    }

    #[test]
    fn descriptor_isolation_guards() {
        let mut wrong_version = descriptor(1);
        wrong_version.version = 1;
        assert!(matches!(wrong_version.validate_in_isolation(), Err(ComputeSetRegistryError::UnsupportedDescriptorVersion(1))));
        let mut no_modality = descriptor(1);
        no_modality.modality_mask = 0;
        assert!(matches!(no_modality.validate_in_isolation(), Err(ComputeSetRegistryError::EmptyModalityMask)));
    }

    #[test]
    fn state_wire_form_is_pinned_u8_and_fail_closed() {
        let pinned = [
            (ComputeSetState::Proposed, 0u8),
            (ComputeSetState::Shadow, 1),
            (ComputeSetState::Active, 2),
            (ComputeSetState::Deprecated, 3),
            (ComputeSetState::EmergencyHalted, 4),
            (ComputeSetState::Retired, 5),
        ];
        for (state, disc) in pinned {
            assert_eq!(state.as_u8(), disc);
            assert_eq!(ComputeSetState::from_u8(disc), Some(state));
            assert_eq!(borsh::to_vec(&state).unwrap(), vec![disc]);
            assert_eq!(borsh::from_slice::<ComputeSetState>(&[disc]).unwrap(), state);
        }
        // Fail-closed decode: an unknown lifecycle byte is never a default state (§22.3).
        assert!(borsh::from_slice::<ComputeSetState>(&[6]).is_err());
        assert!(borsh::from_slice::<ComputeSetState>(&[255]).is_err());
    }

    #[test]
    fn state_transition_matrix() {
        use ComputeSetState::*;
        for s in [Proposed, Shadow, Active, Deprecated, EmergencyHalted, Retired] {
            assert!(s.can_transition_to(s), "{s:?} economics-only revision must be allowed");
            assert!(!Retired.can_transition_to(s) || s == Retired, "Retired is terminal");
        }
        assert!(Proposed.can_transition_to(Shadow));
        assert!(Shadow.can_transition_to(Active));
        assert!(Shadow.can_transition_to(Retired), "Shadow expiry (§23.2)");
        assert!(Active.can_transition_to(Deprecated));
        assert!(Active.can_transition_to(EmergencyHalted));
        assert!(Deprecated.can_transition_to(Retired));
        assert!(Deprecated.can_transition_to(Active), "re-activation pre-Retired (§9)");
        assert!(EmergencyHalted.can_transition_to(Retired));
        assert!(!Proposed.can_transition_to(Active), "Shadow soak is mandatory (§18.1)");
        assert!(!Proposed.can_transition_to(Deprecated));
        assert!(!Active.can_transition_to(Proposed));
        assert!(!Deprecated.can_transition_to(Shadow));
    }

    #[test]
    fn policy_progression_rules() {
        let caps = PolicyGovernanceCapsV1::default();
        let set = descriptor(1).compute_set_id();

        // Unregistered descriptor → reject.
        let first = policy(set, 1, 100, ComputeSetState::Proposed);
        assert!(matches!(
            validate_policy_progression(false, None, None, &first, &caps, 50),
            Err(ComputeSetRegistryError::PolicyForUnregisteredSet(_))
        ));
        // First policy must be Proposed.
        let premature = policy(set, 1, 100, ComputeSetState::Active);
        assert!(matches!(
            validate_policy_progression(true, None, None, &premature, &caps, 50),
            Err(ComputeSetRegistryError::FirstPolicyMustBeProposed(ComputeSetState::Active))
        ));
        // Happy path: Proposed inserted.
        assert_eq!(validate_policy_progression(true, None, None, &first, &caps, 50), Ok(PolicyRegistrationOutcome::Inserted));

        // Progression Proposed → Shadow → Active.
        let shadow = policy(set, 2, 200, ComputeSetState::Shadow);
        assert_eq!(validate_policy_progression(true, Some(&first), None, &shadow, &caps, 150), Ok(PolicyRegistrationOutcome::Inserted));
        let active = policy(set, 3, 300, ComputeSetState::Active);
        assert_eq!(validate_policy_progression(true, Some(&shadow), None, &active, &caps, 250), Ok(PolicyRegistrationOutcome::Inserted));

        // Sequence rollback.
        let rollback = policy(set, 3, 400, ComputeSetState::Active);
        assert!(matches!(
            validate_policy_progression(true, Some(&active), None, &rollback, &caps, 350),
            Err(ComputeSetRegistryError::PolicySequenceRollback { .. })
        ));
        // Same sequence: identical payload idempotent, divergent payload rejected.
        assert_eq!(
            validate_policy_progression(true, Some(&active), Some(&active), &active, &caps, 350),
            Ok(PolicyRegistrationOutcome::Idempotent)
        );
        let mut divergent = active.clone();
        divergent.job_timeout_daa += 1;
        assert!(matches!(
            validate_policy_progression(true, Some(&active), Some(&active), &divergent, &caps, 350),
            Err(ComputeSetRegistryError::PolicySequenceDiverged { .. })
        ));
        // Past-dated effective.
        let past = policy(set, 4, 300, ComputeSetState::Active);
        assert!(matches!(
            validate_policy_progression(true, Some(&active), None, &past, &caps, 350),
            Err(ComputeSetRegistryError::PolicyEffectiveNotFuture { .. })
        ));
        // Illegal transition Proposed → Active (skipping Shadow).
        let skip = policy(set, 2, 200, ComputeSetState::Active);
        assert!(matches!(
            validate_policy_progression(true, Some(&first), None, &skip, &caps, 150),
            Err(ComputeSetRegistryError::InvalidStateTransition { .. })
        ));
        // Retired terminal.
        let retired = policy(set, 4, 400, ComputeSetState::Retired);
        let after_retired = policy(set, 5, 500, ComputeSetState::Active);
        assert!(matches!(
            validate_policy_progression(true, Some(&retired), None, &after_retired, &caps, 450),
            Err(ComputeSetRegistryError::RetiredSetIsTerminal(_))
        ));

        // §22.2 caps.
        let mut heavy = policy(set, 4, 400, ComputeSetState::Active);
        heavy.weight_factor_bps = 10_001;
        assert!(matches!(
            validate_policy_progression(true, Some(&active), None, &heavy, &caps, 350),
            Err(ComputeSetRegistryError::WeightFactorOutOfRange(10_001))
        ));
        let mut pricey = policy(set, 4, 400, ComputeSetState::Active);
        pricey.premium_pi_bps = caps.max_premium_pi_bps + 1;
        assert!(matches!(
            validate_policy_progression(true, Some(&active), None, &pricey, &caps, 350),
            Err(ComputeSetRegistryError::PremiumAboveCap { .. })
        ));
        let mut zero_scale = policy(set, 4, 400, ComputeSetState::Active);
        zero_scale.compute_work_scale = 0;
        assert!(matches!(
            validate_policy_progression(true, Some(&active), None, &zero_scale, &caps, 350),
            Err(ComputeSetRegistryError::ActiveRequiresNonzero("compute_work_scale"))
        ));
        // Shadow (or any non-Active) must carry zero weight — Shadow soaks earn nothing (§8.1).
        let mut weighted_shadow = policy(set, 2, 200, ComputeSetState::Shadow);
        weighted_shadow.weight_factor_bps = 1;
        assert!(matches!(
            validate_policy_progression(true, Some(&first), None, &weighted_shadow, &caps, 150),
            Err(ComputeSetRegistryError::NonActiveRequiresZeroWeight(ComputeSetState::Shadow))
        ));
    }

    #[test]
    fn resolver_picks_highest_effective_sequence() {
        let set_a = h(0xa1);
        let set_b = h(0xb2);
        let history = vec![
            policy(set_a, 1, 100, ComputeSetState::Proposed),
            policy(set_a, 2, 200, ComputeSetState::Shadow),
            policy(set_a, 3, 900, ComputeSetState::Active), // not yet effective at 500
            policy(set_b, 7, 150, ComputeSetState::Proposed),
        ];
        let resolved = resolve_compute_policy(&history, &set_a, 500).unwrap();
        assert_eq!((resolved.policy_sequence, resolved.state), (2, ComputeSetState::Shadow));
        let resolved = resolve_compute_policy(&history, &set_a, 900).unwrap();
        assert_eq!(resolved.policy_sequence, 3);
        assert!(resolve_compute_policy(&history, &set_a, 50).is_none(), "nothing effective yet — fail closed");
        assert_eq!(resolve_compute_policy(&history, &set_b, 500).unwrap().policy_sequence, 7);
    }

    #[test]
    fn plan_id_derivation_zeroes_self_reference() {
        let set = h(0xa1);
        let mut p = plan(1, 100, vec![(set, 10_000)]);
        assert!(p.plan_id_is_canonical());
        let derived = p.derive_plan_id();
        // The stored plan_id never feeds its own derivation.
        p.plan_id = h(0xff);
        assert_eq!(p.derive_plan_id(), derived);
        assert!(!p.plan_id_is_canonical());
    }

    #[test]
    fn plan_validation_rules() {
        let limits = AllocationGovernanceLimitsV1::default();
        let a = h(0xa1);
        let b = h(0xb2);
        let unknown = h(0xee);
        let states = move |set: &Hash64| -> Option<ComputeSetState> {
            if *set == a {
                Some(ComputeSetState::Active)
            } else if *set == b {
                Some(ComputeSetState::Shadow)
            } else {
                None
            }
        };

        // Initial single-set plan.
        let genesis_plan = plan(1, 100, vec![(a, 10_000)]);
        assert_eq!(
            validate_allocation_plan(&genesis_plan, &states, None, None, &limits, 50),
            Ok(PlanRegistrationOutcome::Inserted)
        );

        // §28.2 sum != 10000.
        let short = plan(2, 200, vec![(a, 9_999)]);
        assert!(matches!(
            validate_allocation_plan(&short, &states, Some(&genesis_plan), None, &limits, 150),
            Err(ComputeSetRegistryError::ShareSumInvalid(9_999))
        ));
        // §28.2 duplicate set.
        let dup = plan(2, 200, vec![(a, 5_000), (a, 5_000)]);
        assert!(matches!(
            validate_allocation_plan(&dup, &states, Some(&genesis_plan), None, &limits, 150),
            Err(ComputeSetRegistryError::DuplicatePlanEntry(_))
        ));
        // §28.2 unknown set.
        let unregistered = plan(2, 200, vec![(a, 9_900), (unknown, 100)]);
        assert!(matches!(
            validate_allocation_plan(&unregistered, &states, Some(&genesis_plan), None, &limits, 150),
            Err(ComputeSetRegistryError::PlanReferencesUnregisteredSet(_))
        ));
        // §28.2 inactive set with nonzero share (b is Shadow).
        let shadow_share = plan(2, 200, vec![(a, 9_900), (b, 100)]);
        assert!(matches!(
            validate_allocation_plan(&shadow_share, &states, Some(&genesis_plan), None, &limits, 150),
            Err(ComputeSetRegistryError::NonActiveSetWithShare { .. })
        ));
        // Zero-share entry for a non-Active set is FINE (§10.3 wind-down example).
        let wind_down = plan(2, 200, vec![(a, 10_000), (b, 0)]);
        assert_eq!(
            validate_allocation_plan(&wind_down, &states, Some(&genesis_plan), None, &limits, 150),
            Ok(PlanRegistrationOutcome::Inserted)
        );
        // §22.1 share floor.
        let tiny = AllocationGovernanceLimitsV1 { max_change_per_plan_bps: 10_000, ..limits };
        let below_floor = plan(2, 200, vec![(a, 9_901), (h(0xa1), 0)]);
        let _ = below_floor; // (duplicate id builder guard — replaced by explicit case below)
        let below_floor = {
            let states2 = move |set: &Hash64| -> Option<ComputeSetState> {
                if *set == a || *set == b { Some(ComputeSetState::Active) } else { None }
            };
            let p = plan(2, 200, vec![(a, 9_901), (b, 99)]);
            validate_allocation_plan(&p, &states2, Some(&genesis_plan), None, &tiny, 150)
        };
        assert!(matches!(below_floor, Err(ComputeSetRegistryError::ShareBelowFloor { share_bps: 99, .. })));
        // §28.2 too many active sets.
        let crowded_limits = AllocationGovernanceLimitsV1 { max_active_sets: 2, max_change_per_plan_bps: 10_000, ..limits };
        let all_active = move |_: &Hash64| -> Option<ComputeSetState> { Some(ComputeSetState::Active) };
        let crowded = plan(2, 200, vec![(h(0x01), 3_400), (h(0x02), 3_300), (h(0x03), 3_300)]);
        assert!(matches!(
            validate_allocation_plan(&crowded, &all_active, Some(&genesis_plan), None, &crowded_limits, 150),
            Err(ComputeSetRegistryError::TooManyActiveSets { count: 3, max: 2 })
        ));
        // §28.2 ramp limit exceed: a jumps 10000 → 9000 (Δ1000 > 500).
        let states_ab = move |set: &Hash64| -> Option<ComputeSetState> {
            (*set == a || *set == b).then_some(ComputeSetState::Active)
        };
        let jump = plan(2, 200, vec![(a, 9_000), (b, 1_000)]);
        assert!(matches!(
            validate_allocation_plan(&jump, &states_ab, Some(&genesis_plan), None, &limits, 150),
            Err(ComputeSetRegistryError::RampLimitExceeded { .. })
        ));
        // Dropping a nonzero entry is a change to 0 and ramps too.
        let dropped = plan(2, 200, vec![(b, 10_000)]);
        assert!(matches!(
            validate_allocation_plan(&dropped, &states_ab, Some(&genesis_plan), None, &limits, 150),
            Err(ComputeSetRegistryError::RampLimitExceeded { .. })
        ));
        // Within ramp: 10000 → 9500 / 500.
        let gentle = plan(2, 200, vec![(a, 9_500), (b, 500)]);
        assert_eq!(
            validate_allocation_plan(&gentle, &states_ab, Some(&genesis_plan), None, &limits, 150),
            Ok(PlanRegistrationOutcome::Inserted)
        );
        // §28.2 same sequence different plan.
        assert!(matches!(
            validate_allocation_plan(&gentle, &states_ab, Some(&genesis_plan), Some(&genesis_plan), &limits, 150),
            Err(ComputeSetRegistryError::PlanSequenceDiverged(_))
        ));
        assert_eq!(
            validate_allocation_plan(&gentle, &states_ab, Some(&genesis_plan), Some(&gentle), &limits, 150),
            Ok(PlanRegistrationOutcome::Idempotent)
        );
        // Sequence rollback.
        let stale = plan(1, 300, vec![(a, 10_000)]);
        assert!(matches!(
            validate_allocation_plan(&stale, &states_ab, Some(&gentle), None, &limits, 250),
            Err(ComputeSetRegistryError::PlanSequenceRollback { prev: 2, next: 1 })
        ));
        // Future-dated activation.
        let immediate = plan(3, 150, vec![(a, 9_500), (b, 500)]);
        assert!(matches!(
            validate_allocation_plan(&immediate, &states_ab, Some(&gentle), None, &limits, 150),
            Err(ComputeSetRegistryError::PlanEffectiveNotFuture { .. })
        ));
        // Non-canonical plan_id.
        let mut forged = plan(3, 400, vec![(a, 9_500), (b, 500)]);
        forged.plan_id = h(0x66);
        assert!(matches!(
            validate_allocation_plan(&forged, &states_ab, Some(&gentle), None, &limits, 150),
            Err(ComputeSetRegistryError::PlanIdNotCanonical(_))
        ));
        // Empty plan.
        let empty = plan(3, 400, vec![]);
        assert!(matches!(
            validate_allocation_plan(&empty, &states_ab, Some(&gentle), None, &limits, 150),
            Err(ComputeSetRegistryError::EmptyPlan)
        ));
    }

    #[test]
    fn plan_resolver_picks_highest_effective_sequence() {
        let a = h(0xa1);
        let history = vec![plan(1, 100, vec![(a, 10_000)]), plan(2, 500, vec![(a, 10_000)]), plan(3, 900, vec![(a, 10_000)])];
        assert_eq!(resolve_allocation_plan(&history, 600).unwrap().sequence, 2);
        assert_eq!(resolve_allocation_plan(&history, 900).unwrap().sequence, 3);
        assert!(resolve_allocation_plan(&history, 50).is_none());
    }
}
