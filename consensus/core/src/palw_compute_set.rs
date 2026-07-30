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

use crate::tx::TransactionOutpoint;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash64, blake2b_512_keyed};
use std::collections::BTreeMap;
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

/// §19 — commitment to a provider/auditor's sorted-unique supported `compute_set_id` list.
pub const PALW_SUPPORTED_SETS_ROOT_DOMAIN: &[u8] = b"misaka-palw-supported-sets-root-v1";

/// §20.2 — `leaf_hash = Hash64_k(public-leaf-v2, borsh(PalwPublicLeafV2))`.
pub const PALW_PUBLIC_LEAF_V2_DOMAIN: &[u8] = b"misaka-palw-public-leaf-v2";

/// §20.3 — `certificate_hash = Hash64_k(audit-cert-v1, borsh(PalwComputeSetAuditCertificateV1))`.
/// Disjoint from the batch-certificate domain (`PALW_LEAF_DOMAIN`) so a set-centric and a
/// batch-centric certificate can never alias.
pub const PALW_COMPUTE_SET_AUDIT_CERT_DOMAIN: &[u8] = b"misaka-palw-compute-set-audit-cert-v1";

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

/// The minimal precedent facts a policy revision is validated against — what the fork-local
/// registry view retains per set (full records live in the content-addressed store, addressed
/// by `policy_id`, so byte-equality of a re-submission reduces to id equality).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PolicyPrecedent {
    pub highest_sequence: u64,
    pub state: ComputeSetState,
}

/// §9 + §22.2 + §18.1 — validate one policy revision against the set's recorded history.
///
/// * `descriptor_registered` — the §9 rule 「未登録DescriptorへのPolicy」→ reject.
/// * `activation_certified` — §17.3/§18.1: entering `Shadow` requires the validator-quorum
///   activation certificate; a set that never certified can never leave `Proposed`.
/// * `precedent` — highest-sequence facts so far (if any); `same_sequence_policy_id` — the
///   content id already stored under `next.policy_sequence` (if any): equal id → idempotent,
///   different id → the §9 「同一set/sequenceで異なるpayload」consensus error.
/// * `current_daa` — the DAA score of the block applying the revision (future-dating base).
pub fn validate_policy_progression(
    descriptor_registered: bool,
    activation_certified: bool,
    precedent: Option<PolicyPrecedent>,
    same_sequence_policy_id: Option<Hash64>,
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
    // §17.3/§18.1 — the Shadow stage is gated on the validator-quorum activation certificate.
    // Enforcing at the Shadow edge covers the whole lifecycle: the transition matrix forces
    // every path to Active THROUGH Shadow, so no uncertified set can ever mint.
    if next.state == ComputeSetState::Shadow && !activation_certified {
        return Err(ComputeSetRegistryError::ShadowRequiresActivationCertificate(next.compute_set_id));
    }

    // Same-sequence handling: byte-identical replay (same content id) is idempotent, divergence
    // is an error (§9).
    if let Some(existing_id) = same_sequence_policy_id {
        return if existing_id == next.policy_id() {
            Ok(PolicyRegistrationOutcome::Idempotent)
        } else {
            Err(ComputeSetRegistryError::PolicySequenceDiverged { set: next.compute_set_id, sequence: next.policy_sequence })
        };
    }

    match precedent {
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
            if next.policy_sequence <= prev.highest_sequence {
                return Err(ComputeSetRegistryError::PolicySequenceRollback {
                    set: next.compute_set_id,
                    prev: prev.highest_sequence,
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
// §17 — generic on-chain registry transaction payloads (one fixed band forever, §17.1)
// =============================================================================================

pub const PALW_COMPUTE_SET_PROPOSAL_PAYLOAD_VERSION: u16 = 1;
pub const PALW_COMPUTE_SET_ACTIVATION_CERT_VERSION: u16 = 1;
pub const PALW_COMPUTE_SET_POLICY_UPDATE_VERSION: u16 = 1;
pub const PALW_COMPUTE_SET_EMERGENCY_HALT_VERSION: u16 = 1;

/// §17.2 — registers a NEW Compute Set (state `Proposed`). Carries the full immutable
/// descriptor; `compute_set_id` is derived from it, never named.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeSetProposalV1 {
    pub version: u16,

    pub descriptor: PalwComputeSetDescriptorV2,

    pub proposer_credential: Hash64,
    /// The §23.2 anti-explosion bond (value floor enforced at the tx-rule layer, like provider
    /// bonds).
    pub proposal_bond_ref: TransactionOutpoint,

    pub artifact_distribution_root: Hash64,
    pub independent_build_attestation_root: Hash64,

    pub requested_shadow_activation_daa: u64,
}

/// §17.3 — the validator-quorum certificate gating the Shadow stage. Reuses the existing
/// ML-DSA-87 validator quorum machinery; a single operator key can never activate a set. The
/// cryptographic vote verification runs at the process layer (the batch-certificate
/// `verify_certificate_attestation` pattern); this record carries what was certified.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeSetActivationCertificateV1 {
    pub version: u16,

    pub compute_set_id: Hash64,

    pub descriptor_hash: Hash64,
    pub conformance_result_root: Hash64,
    pub auditor_capacity_evidence_root: Hash64,
    pub artifact_reproducibility_root: Hash64,

    pub validator_set_commitment: Hash64,
    pub approving_stake: u128,
    pub total_selected_stake: u128,

    pub effective_from_daa: u64,
    pub votes_root: Hash64,
}

/// §17.1 — one mutable-policy revision, wrapped so the payload kind is versioned independently
/// of the policy record it carries.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeSetPolicyUpdateV1 {
    pub version: u16,
    pub policy: PalwComputeSetPolicyV1,
}

/// §18.6 — immediate stop of new tickets / PALW blocks / jobs for one set. History stays valid;
/// the set's shares are zeroed by the NEXT allocation plan, and recovery (or retirement) happens
/// through later policy revisions under the §8.1 transition matrix.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeSetEmergencyHaltV1 {
    pub version: u16,
    pub compute_set_id: Hash64,
    /// Commitment to the halt rationale/evidence (content-addressed, may be zero for a pure
    /// governance stop).
    pub evidence_root: Hash64,
}

/// The decoded, canonical form of one registry transaction (the registry-band analogue of
/// `PalwOverlayEffect`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwComputeRegistryEffect {
    Proposal(PalwComputeSetProposalV1),
    ActivationCertificate(PalwComputeSetActivationCertificateV1),
    PolicyUpdate(PalwComputeSetPolicyUpdateV1),
    AllocationPlan(PalwModelAllocationPlanV1),
    EmergencyHalt(PalwComputeSetEmergencyHaltV1),
}

/// Strict, canonical decode of a registry-band payload (`0x40..=0x44`). Mirrors
/// `parse_palw_overlay`: strict borsh (`from_slice` rejects trailing bytes), a re-encode
/// equality check (one canonical byte form per payload), and per-kind version pins. Unknown
/// bytes in the band fail closed.
pub fn parse_palw_compute_registry(
    subnet_first_byte: u8,
    payload: &[u8],
) -> Result<PalwComputeRegistryEffect, ComputeSetRegistryError> {
    use crate::subnets::*;

    fn strict<T: BorshDeserialize + BorshSerialize>(payload: &[u8], kind: &'static str) -> Result<T, ComputeSetRegistryError> {
        let decoded: T =
            borsh::from_slice(payload).map_err(|_| ComputeSetRegistryError::MalformedRegistryPayload(kind))?;
        // Round-trip canonicality (the repo idiom): the wire bytes must BE the canonical bytes,
        // or two nodes could store different preimages for one content id.
        if borsh::to_vec(&decoded).map(|canonical| canonical != payload).unwrap_or(true) {
            return Err(ComputeSetRegistryError::NonCanonicalRegistryPayload(kind));
        }
        Ok(decoded)
    }

    let kind = Some(subnet_first_byte);
    if kind == SUBNETWORK_ID_PALW_COMPUTE_SET_PROPOSAL.palw_compute_registry_tx_kind() {
        let proposal: PalwComputeSetProposalV1 = strict(payload, "compute_set_proposal")?;
        if proposal.version != PALW_COMPUTE_SET_PROPOSAL_PAYLOAD_VERSION {
            return Err(ComputeSetRegistryError::UnsupportedRegistryPayloadVersion("compute_set_proposal", proposal.version));
        }
        proposal.descriptor.validate_in_isolation()?;
        Ok(PalwComputeRegistryEffect::Proposal(proposal))
    } else if kind == SUBNETWORK_ID_PALW_COMPUTE_SET_ACTIVATION_CERT.palw_compute_registry_tx_kind() {
        let certificate: PalwComputeSetActivationCertificateV1 = strict(payload, "compute_set_activation_cert")?;
        if certificate.version != PALW_COMPUTE_SET_ACTIVATION_CERT_VERSION {
            return Err(ComputeSetRegistryError::UnsupportedRegistryPayloadVersion(
                "compute_set_activation_cert",
                certificate.version,
            ));
        }
        Ok(PalwComputeRegistryEffect::ActivationCertificate(certificate))
    } else if kind == SUBNETWORK_ID_PALW_COMPUTE_SET_POLICY_UPDATE.palw_compute_registry_tx_kind() {
        let update: PalwComputeSetPolicyUpdateV1 = strict(payload, "compute_set_policy_update")?;
        if update.version != PALW_COMPUTE_SET_POLICY_UPDATE_VERSION {
            return Err(ComputeSetRegistryError::UnsupportedRegistryPayloadVersion("compute_set_policy_update", update.version));
        }
        Ok(PalwComputeRegistryEffect::PolicyUpdate(update))
    } else if kind == SUBNETWORK_ID_PALW_MODEL_ALLOCATION_PLAN.palw_compute_registry_tx_kind() {
        let plan: PalwModelAllocationPlanV1 = strict(payload, "model_allocation_plan")?;
        if plan.version != PALW_MODEL_ALLOCATION_PLAN_VERSION {
            return Err(ComputeSetRegistryError::UnsupportedRegistryPayloadVersion("model_allocation_plan", plan.version));
        }
        Ok(PalwComputeRegistryEffect::AllocationPlan(plan))
    } else if kind == SUBNETWORK_ID_PALW_COMPUTE_SET_EMERGENCY_HALT.palw_compute_registry_tx_kind() {
        let halt: PalwComputeSetEmergencyHaltV1 = strict(payload, "compute_set_emergency_halt")?;
        if halt.version != PALW_COMPUTE_SET_EMERGENCY_HALT_VERSION {
            return Err(ComputeSetRegistryError::UnsupportedRegistryPayloadVersion("compute_set_emergency_halt", halt.version));
        }
        Ok(PalwComputeRegistryEffect::EmergencyHalt(halt))
    } else {
        Err(ComputeSetRegistryError::UnhandledRegistrySubnet(subnet_first_byte))
    }
}

// =============================================================================================
// §21.2 — the fork-local registry view (the `PalwBatchViewV1` analogue for Compute Sets)
// =============================================================================================

/// Anti-explosion bound on the number of sets a single view tracks (§23.2). Proposal bonds are
/// the economic limit; this is the hard structural one.
pub const MAX_REGISTRY_VIEW_SETS: usize = 256;

/// One row of a set's policy history index: enough to (a) validate the next revision and
/// (b) resolve the policy in force at any source DAA (§9) — the record CONTENT lives in the
/// content-addressed store under `policy_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputePolicyIndexEntryV1 {
    pub sequence: u64,
    pub effective_from_daa: u64,
    pub policy_id: Hash64,
    pub state: ComputeSetState,
}

/// Per-set view row.
#[derive(Clone, Debug, PartialEq, Eq, Default, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeSetViewEntryV1 {
    /// §17.3 — a quorum-verified activation certificate has been recorded.
    pub activation_certified: bool,
    /// §18.6 — an emergency halt is in force (cleared only by a later policy revision whose
    /// transition the §8.1 matrix admits from `EmergencyHalted`).
    pub halted: bool,
    /// Ascending by `sequence`; grows only by governance actions, so its size is bounded by
    /// governance cadence, not by chain length.
    pub policy_index: Vec<PalwComputePolicyIndexEntryV1>,
}

impl PalwComputeSetViewEntryV1 {
    /// The lifecycle state currently in force. An emergency halt overrides the policy state
    /// (§18.6 — the halt is immediate; the policy record catching up is a later revision).
    /// A set with no policy yet is `Proposed` (§17.2 — the proposal itself registers it).
    pub fn current_state(&self) -> ComputeSetState {
        if self.halted {
            return ComputeSetState::EmergencyHalted;
        }
        self.policy_index.last().map(|entry| entry.state).unwrap_or(ComputeSetState::Proposed)
    }

    fn precedent(&self) -> Option<PolicyPrecedent> {
        self.policy_index.last().map(|entry| PolicyPrecedent {
            highest_sequence: entry.sequence,
            state: if self.halted { ComputeSetState::EmergencyHalted } else { entry.state },
        })
    }

    /// §9 resolution over the index: the highest sequence already effective at `daa_score`.
    pub fn resolve_policy_id_at(&self, daa_score: u64) -> Option<Hash64> {
        self.policy_index
            .iter()
            .filter(|entry| entry.effective_from_daa <= daa_score)
            .max_by_key(|entry| entry.sequence)
            .map(|entry| entry.policy_id)
    }
}

/// One row of the plan history index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputePlanIndexEntryV1 {
    pub sequence: u64,
    pub effective_from_daa: u64,
    pub plan_id: Hash64,
}

/// The fork-local, block-keyed Compute Set registry view (§21.2): content-addressed records
/// land in write-once stores; THIS is the mutable index a block's past determines — applied
/// fork-locally, persisted per block, reverted by reorg exactly like `PalwBatchViewV1`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeRegistryViewV1 {
    pub version: u16,
    pub sets: BTreeMap<Hash64, PalwComputeSetViewEntryV1>,
    /// Ascending by `sequence`.
    pub plan_index: Vec<PalwComputePlanIndexEntryV1>,
    /// The latest plan's full entry list — the §22.1 ramp baseline for validating its successor.
    pub latest_plan: Option<PalwModelAllocationPlanV1>,
}

impl Default for PalwComputeRegistryViewV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl PalwComputeRegistryViewV1 {
    pub fn new() -> Self {
        Self { version: 1, sets: BTreeMap::new(), plan_index: Vec::new(), latest_plan: None }
    }

    /// §17.2/§7.2 — apply a proposal: absent → insert (`Proposed`), present → idempotent
    /// (`compute_set_id` is the content hash, so byte divergence under one id cannot occur
    /// through this path).
    pub fn apply_proposal(
        &mut self,
        proposal: &PalwComputeSetProposalV1,
    ) -> Result<DescriptorRegistrationOutcome, ComputeSetRegistryError> {
        proposal.descriptor.validate_in_isolation()?;
        let set_id = proposal.descriptor.compute_set_id();
        if self.sets.contains_key(&set_id) {
            return Ok(DescriptorRegistrationOutcome::Idempotent);
        }
        if self.sets.len() >= MAX_REGISTRY_VIEW_SETS {
            return Err(ComputeSetRegistryError::RegistryViewFull(MAX_REGISTRY_VIEW_SETS));
        }
        self.sets.insert(set_id, PalwComputeSetViewEntryV1::default());
        Ok(DescriptorRegistrationOutcome::Inserted)
    }

    /// §17.3 — record a quorum-verified activation certificate. `quorum_verified` MUST be the
    /// result of the process-layer ML-DSA-87 vote verification (the batch-certificate
    /// `verify_certificate_attestation` pattern); this method refuses unverified certificates
    /// rather than trusting the payload's own stake claims.
    pub fn apply_certificate(
        &mut self,
        certificate: &PalwComputeSetActivationCertificateV1,
        quorum_verified: bool,
    ) -> Result<(), ComputeSetRegistryError> {
        if !quorum_verified {
            return Err(ComputeSetRegistryError::CertificateQuorumNotVerified(certificate.compute_set_id));
        }
        if certificate.descriptor_hash != certificate.compute_set_id {
            // The descriptor hash IS the set id (§7.1); a certificate naming two different
            // identities certified nothing.
            return Err(ComputeSetRegistryError::CertificateDescriptorMismatch {
                set: certificate.compute_set_id,
                descriptor_hash: certificate.descriptor_hash,
            });
        }
        if certificate.approving_stake == 0 || certificate.approving_stake > certificate.total_selected_stake {
            return Err(ComputeSetRegistryError::CertificateStakeInvalid {
                approving: certificate.approving_stake,
                total: certificate.total_selected_stake,
            });
        }
        let entry = self
            .sets
            .get_mut(&certificate.compute_set_id)
            .ok_or(ComputeSetRegistryError::CertificateForUnregisteredSet(certificate.compute_set_id))?;
        entry.activation_certified = true;
        Ok(())
    }

    /// §9/§22.2 — apply one policy revision through [`validate_policy_progression`], then index
    /// it. On success the revision also clears an emergency halt (the matrix only admits
    /// transitions FROM `EmergencyHalted` that governance is allowed to take).
    pub fn apply_policy(
        &mut self,
        policy: &PalwComputeSetPolicyV1,
        caps: &PolicyGovernanceCapsV1,
        current_daa: u64,
    ) -> Result<PolicyRegistrationOutcome, ComputeSetRegistryError> {
        let entry = self
            .sets
            .get(&policy.compute_set_id)
            .ok_or(ComputeSetRegistryError::PolicyForUnregisteredSet(policy.compute_set_id))?;
        let same_sequence_policy_id =
            entry.policy_index.iter().find(|row| row.sequence == policy.policy_sequence).map(|row| row.policy_id);
        let outcome = validate_policy_progression(
            true,
            entry.activation_certified,
            entry.precedent(),
            same_sequence_policy_id,
            policy,
            caps,
            current_daa,
        )?;
        if outcome == PolicyRegistrationOutcome::Inserted {
            let entry = self.sets.get_mut(&policy.compute_set_id).expect("presence checked above");
            entry.policy_index.push(PalwComputePolicyIndexEntryV1 {
                sequence: policy.policy_sequence,
                effective_from_daa: policy.effective_from_daa,
                policy_id: policy.policy_id(),
                state: policy.state,
            });
            entry.halted = false;
        }
        Ok(outcome)
    }

    /// §18.6 — apply an emergency halt: immediate (never future-dated — it is an emergency),
    /// idempotent, and it never rewrites history (the policy index is untouched).
    pub fn apply_halt(&mut self, halt: &PalwComputeSetEmergencyHaltV1) -> Result<(), ComputeSetRegistryError> {
        let entry =
            self.sets.get_mut(&halt.compute_set_id).ok_or(ComputeSetRegistryError::HaltForUnregisteredSet(halt.compute_set_id))?;
        entry.halted = true;
        Ok(())
    }

    /// §10.3/§22.1 — apply an allocation plan, resolving every entry's lifecycle state from
    /// THIS view (halt override included) and ramp-validating against the latest recorded plan.
    pub fn apply_plan(
        &mut self,
        plan: &PalwModelAllocationPlanV1,
        limits: &AllocationGovernanceLimitsV1,
        current_daa: u64,
    ) -> Result<PlanRegistrationOutcome, ComputeSetRegistryError> {
        // Content identity by plan_id: same sequence + equal id → idempotent, different → diverged.
        if let Some(existing) = self.plan_index.iter().find(|row| row.sequence == plan.sequence) {
            return if existing.plan_id == plan.derive_plan_id() {
                Ok(PlanRegistrationOutcome::Idempotent)
            } else {
                Err(ComputeSetRegistryError::PlanSequenceDiverged(plan.sequence))
            };
        }
        let sets = &self.sets;
        let state_of = |set_id: &Hash64| -> Option<ComputeSetState> { sets.get(set_id).map(|entry| entry.current_state()) };
        let outcome = validate_allocation_plan(plan, &state_of, self.latest_plan.as_ref(), None, limits, current_daa)?;
        if outcome == PlanRegistrationOutcome::Inserted {
            self.plan_index.push(PalwComputePlanIndexEntryV1 {
                sequence: plan.sequence,
                effective_from_daa: plan.effective_from_daa,
                plan_id: plan.plan_id,
            });
            self.latest_plan = Some(plan.clone());
        }
        Ok(outcome)
    }

    /// §21.4 resolution for plans: the plan in force at `daa_score`.
    pub fn resolve_plan_id_at(&self, daa_score: u64) -> Option<Hash64> {
        self.plan_index
            .iter()
            .filter(|row| row.effective_from_daa <= daa_score)
            .max_by_key(|row| row.sequence)
            .map(|row| row.plan_id)
    }
}

// =============================================================================================
// §14/§21.4 — historical work resolution (source header → exact immutable records → credit)
// =============================================================================================

/// The two numbers GHOSTDAG needs from a source block's committed records: the quantum→hash
/// normalization scale and the DAG-credit weight. Everything else about §14 is the VALIDATION
/// that these came from the exact records the header names, active at the source DAA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoricalComputeResolution {
    pub compute_work_scale: u64,
    pub weight_factor_bps: u16,
}

/// §14 — `source header → exact policy ID → exact allocation plan ID → historical immutable
/// resolution`. Validates that:
///
/// * the supplied records ARE the committed ones (content-id equality — a peer cannot swap in
///   a different revision, §23.4);
/// * the policy governs the header's own `compute_set_id`;
/// * both records were effective at the SOURCE block's DAA (never "currently", ADR-MA-007);
/// * the policy state was `Active` and the plan allotted the set nonzero share (§13.1).
///
/// Absence of any record is the caller's fail-closed reject (§22.3 `missing historical data`);
/// this function only ever strengthens — it never substitutes a default.
pub fn resolve_source_policy_for_credit(
    policy: &PalwComputeSetPolicyV1,
    plan: &PalwModelAllocationPlanV1,
    expected_policy_id: Hash64,
    expected_plan_id: Hash64,
    header_compute_set_id: Hash64,
    source_daa_score: u64,
) -> Result<HistoricalComputeResolution, ComputeSetRegistryError> {
    if policy.policy_id() != expected_policy_id {
        return Err(ComputeSetRegistryError::CommittedPolicyMismatch { expected: expected_policy_id, actual: policy.policy_id() });
    }
    if policy.compute_set_id != header_compute_set_id {
        return Err(ComputeSetRegistryError::PolicyGovernsDifferentSet {
            policy_set: policy.compute_set_id,
            header_set: header_compute_set_id,
        });
    }
    if plan.plan_id != expected_plan_id || !plan.plan_id_is_canonical() {
        return Err(ComputeSetRegistryError::CommittedPlanMismatch { expected: expected_plan_id, actual: plan.plan_id });
    }
    if policy.effective_from_daa > source_daa_score {
        return Err(ComputeSetRegistryError::PolicyNotEffectiveAtSource {
            effective_from_daa: policy.effective_from_daa,
            source_daa_score,
        });
    }
    if plan.effective_from_daa > source_daa_score {
        return Err(ComputeSetRegistryError::PlanNotEffectiveAtSource { effective_from_daa: plan.effective_from_daa, source_daa_score });
    }
    if policy.state != ComputeSetState::Active {
        return Err(ComputeSetRegistryError::SourcePolicyNotActive(policy.state));
    }
    let entry = plan
        .entries
        .iter()
        .find(|entry| entry.compute_set_id == header_compute_set_id)
        .ok_or(ComputeSetRegistryError::PlanOmitsSourceSet(header_compute_set_id))?;
    if entry.target_share_bps == 0 {
        return Err(ComputeSetRegistryError::SourceSetHasZeroShare(header_compute_set_id));
    }
    Ok(HistoricalComputeResolution { compute_work_scale: policy.compute_work_scale, weight_factor_bps: policy.weight_factor_bps })
}

/// §14 — `credited = mul_div_floor(normalized, weight_factor_bps, 10000)` over `BlueWorkType`,
/// overflow-saturating exactly like `normalize_palw_work` (an attacker gains nothing from
/// overflow; the compute-to-hash cap still bounds the credit downstream).
pub fn credited_compute_work(normalized: crate::BlueWorkType, weight_factor_bps: u16) -> crate::BlueWorkType {
    let weight = weight_factor_bps.min(BPS_DENOMINATOR) as u64;
    let (scaled, overflow) = normalized.overflowing_mul_u64(weight);
    let scaled = if overflow { crate::BlueWorkType::MAX } else { scaled };
    scaled / crate::BlueWorkType::from_u64(BPS_DENOMINATOR as u64)
}

// =============================================================================================
// §19 — provider / auditor capability, Compute Set-centric
// =============================================================================================

/// A bonded credential's lifecycle at a point in time (§19 selection gate). Minimal, on-chain-
/// derivable states — availability (DA-01) is a SEPARATE gate resolved from the DA state, not
/// encoded here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum BondState {
    /// Bonded but still inside the maturity delay — not yet selectable.
    Maturing = 0,
    /// Matured and active — selectable if every other §19 gate passes.
    Active = 1,
    /// An unbond has been requested — no longer selectable (§19 `not unbonding`).
    Unbonding = 2,
    /// Slashed — permanently unselectable (§19 `not slashed`).
    Slashed = 3,
}

impl BondState {
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[inline]
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Maturing),
            1 => Some(Self::Active),
            2 => Some(Self::Unbonding),
            3 => Some(Self::Slashed),
            _ => None,
        }
    }
}

impl BorshSerialize for BondState {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        self.as_u8().serialize(writer)
    }
}

impl BorshDeserialize for BondState {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        let raw = u8::deserialize_reader(reader)?;
        BondState::from_u8(raw)
            .ok_or_else(|| borsh::io::Error::new(borsh::io::ErrorKind::InvalidData, format!("unknown BondState {raw}")))
    }
}

/// §19 — the Compute Set-centric capability record, replacing the `runtime_classes` view. A
/// provider/auditor commits the SORTED-UNIQUE list of `compute_set_id`s it can execute as
/// `supported_compute_sets_root` (§16 `supported_compute_sets = [A, B, C]`); a set is added or
/// removed by publishing a new capability at a higher `capability_sequence` — never node code
/// (§19 「Nodeコード変更は不要」).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwProviderCapabilityV2 {
    pub version: u16,
    pub credential_id: Hash64,
    pub supported_compute_sets_root: Hash64,
    pub conformance_valid_until: u64,
    pub capability_sequence: u64,
    pub bonded_value: u128,
    pub bond_state: BondState,
}

pub const PALW_PROVIDER_CAPABILITY_VERSION: u16 = 2;

/// §19 — the commitment a capability publishes over its supported set list: order- and
/// count-sensitive keyed hash of the SORTED-UNIQUE `compute_set_id`s (the `palw_audit_sample_root`
/// idiom). Returns `None` if the list is unsorted or has duplicates — the canonical form is
/// mandatory so the root is a function of the SET, not of an ordering.
pub fn palw_supported_compute_sets_root(sets: &[Hash64]) -> Option<Hash64> {
    if sets.windows(2).any(|w| w[0].as_bytes() >= w[1].as_bytes()) {
        return None; // unsorted or duplicate
    }
    let mut preimage = Vec::with_capacity(8 + sets.len() * 64);
    preimage.extend_from_slice(&(sets.len() as u64).to_le_bytes());
    for set in sets {
        preimage.extend_from_slice(set.as_bytes().as_slice());
    }
    Some(blake2b_512_keyed(PALW_SUPPORTED_SETS_ROOT_DOMAIN, &preimage))
}

impl PalwProviderCapabilityV2 {
    /// True iff `supported_sets_preimage` is the canonical sorted-unique list this capability
    /// committed AND it contains `set_id`. The preimage is supplied out-of-band (the root is the
    /// only on-chain field, §19); this checks it against the commitment before trusting it.
    pub fn supports_set(&self, supported_sets_preimage: &[Hash64], set_id: &Hash64) -> bool {
        match palw_supported_compute_sets_root(supported_sets_preimage) {
            Some(root) if root == self.supported_compute_sets_root => supported_sets_preimage.binary_search_by(|s| s.as_bytes().cmp(&set_id.as_bytes())).is_ok(),
            _ => false,
        }
    }

    /// §19 selection gate (the on-chain-resolvable half): supported set + valid conformance +
    /// matured, active, non-unbonding, non-slashed bond. Availability (DA-01) is the caller's
    /// separate gate — a provider passing this is a *candidate*, still subject to the DA check.
    pub fn eligible_for_selection(&self, set_id: &Hash64, supported_sets_preimage: &[Hash64], current_epoch: u64) -> bool {
        self.version == PALW_PROVIDER_CAPABILITY_VERSION
            && self.bond_state == BondState::Active
            && current_epoch <= self.conformance_valid_until
            && self.supports_set(supported_sets_preimage, set_id)
    }
}

// =============================================================================================
// §20 — Receipt / Leaf / Certificate binding, Compute Set-centric
// =============================================================================================
//
// §20.1 (Receipt) is ALREADY satisfied by the frozen Receipt v3 (`mil/palw/src/receipt_v3.rs`):
// `MatchProjectionV2` carries compute_set_id / job_challenge / output_commitment /
// schedule_root / execution_root / route_root / state_root / canonical_compute_units /
// token_count / stop_reason as exact-match consensus input, and `implementation_id` lives in
// the signed-but-non-matched `ImplementationTelemetryV3` (§20.1 «telemetryへ分離»). The two
// §20.1 items not literal fields there are committed transitively: `compute_vm_id` through
// `compute_set_id` (the descriptor pins it, §7), and the shape through `schedule_root` (the
// canonical schedule is a function of the drawn shape). Receipt v3 is one frozen breaking
// bundle — extending it is a v4 event, not an edit.

/// §20.2 — the leaf with `compute_set_id` and `compute_policy_id` as first-class fields (the v1
/// leaf carried the set id as a trailing receipt-v3 field). `implementation_id` / runtime-class
/// telemetry is deliberately ABSENT — validity is the set id + the execution roots, never the
/// implementation (§16.1). A parallel type to `PalwPublicLeafV1`: the migration is a new leaf
/// version adopted at re-genesis, not an in-place edit (ADR-MA-004 / §24 «Leaf に新 field → fork»).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwPublicLeafV2 {
    pub version: u16,

    pub compute_set_id: Hash64,
    pub compute_policy_id: Hash64,

    pub job_nullifier: Hash64,
    pub challenge_commitment: Hash64,

    pub replica_set_root: Hash64,

    pub output_commitment: Hash64,
    pub schedule_root: Hash64,
    pub execution_root: Hash64,
    pub route_root: Hash64,
    pub state_root: Hash64,

    pub canonical_compute_units: u128,

    pub receipt_da_root: Hash64,
    pub reward_set_root: Hash64,
}

pub const PALW_PUBLIC_LEAF_V2_VERSION: u16 = 2;

impl PalwPublicLeafV2 {
    /// §20.2 — the leaf-descriptor hash, under a domain disjoint from the v1 leaf so a v1 and a
    /// v2 leaf can never collide.
    pub fn leaf_hash(&self) -> Hash64 {
        blake2b_512_keyed(PALW_PUBLIC_LEAF_V2_DOMAIN, &borsh::to_vec(self).expect("borsh"))
    }
}

// =============================================================================================
// §20.3 — the Compute Set-centric audit certificate
// =============================================================================================

pub const PALW_COMPUTE_SET_AUDIT_CERT_VERSION: u16 = 1;

/// §20.3 — the set-centric audit certificate: commits that auditors CAPABLE OF REPRODUCING the
/// target set (§19 selection gate) were selected, replayed the sampled leaves, and voted. The
/// parallel type to `PalwBatchCertificateV2` for the re-genesis leaf-V2 world — batch identity
/// is replaced by the (set, policy, leaf root) triple the certificate certifies.
///
/// Verification split follows the §17.3 activation-certificate precedent: this record carries
/// WHAT was certified (content commitments only); the cryptographic vote verification and the
/// stake tally recomputation run at the process layer (`verify_certificate_attestation`
/// pattern), so `approving_stake` is a declared commitment that verifiers recompute and reject
/// on mismatch — never a trusted input (the `PalwBatchCertificateV2::approving_stake` idiom).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwComputeSetAuditCertificateV1 {
    pub version: u16,

    /// The set whose execution this certificate attests (§20.3 `compute_set_id`).
    pub compute_set_id: Hash64,
    /// Hash of the exact immutable descriptor bytes (§21.4 exact-record resolution — committed
    /// separately from `compute_set_id` so a verifier holds a direct content handle, the §17.3
    /// activation-certificate idiom).
    pub descriptor_hash: Hash64,
    /// The EXACT policy revision that governed the audited execution (§20.3 `policy ID`,
    /// ADR-MA-007 — never "the current policy").
    pub compute_policy_id: Hash64,

    /// Root over the audited `PalwPublicLeafV2::leaf_hash`es (§20.3 `leaf root`).
    pub leaf_root: Hash64,

    /// Snapshot commitment of the auditor capabilities at selection time (§20.3 `auditor
    /// capability snapshot`): every selected auditor's `PalwProviderCapabilityV2` passed
    /// `eligible_for_selection` for THIS set — the snapshot makes that claim replayable.
    pub auditor_capability_root: Hash64,
    /// Commitment to the beacon-derived sample plan (§20.3 `sample plan` — which leaves were
    /// drawn, the `palw_audit_sample_root` idiom).
    pub sample_plan_root: Hash64,
    /// Commitment to the auditors' replay verdicts (§20.3 `replay result`): per-sampled-leaf
    /// `MatchProjectionV2` digests recomputed under the set's pinned Compute IR.
    pub replay_result_root: Hash64,

    /// Audit epoch this certificate settles.
    pub certificate_epoch: u64,
    /// Declared stake-weighted PASS tally (recomputed + rejected on mismatch at verify time).
    pub approving_stake: u128,
    /// Total selected auditor stake the tally is measured against (quorum denominator).
    pub total_selected_stake: u128,
    /// Root over the embedded auditor votes (§20.3 `votes` — §17.3 `votes_root` idiom).
    pub votes_root: Hash64,
}

impl PalwComputeSetAuditCertificateV1 {
    /// The certificate's content identity, under its own pinned domain.
    pub fn certificate_hash(&self) -> Hash64 {
        blake2b_512_keyed(PALW_COMPUTE_SET_AUDIT_CERT_DOMAIN, &borsh::to_vec(self).expect("borsh"))
    }
}

// =============================================================================================
// §12 — per-set virtual sublane difficulty target
// =============================================================================================

/// §12 — the target block interval (ms) for a Compute Set holding `target_share_bps` of the PALW
/// lane, given the lane's own target interval.
///
/// Every set mines the SAME `pow_algo_id = PALW`; difficulty separates them logically by
/// `compute_set_id`. A set that should win `share/10000` of the lane's blocks must therefore
/// space its own blocks `10000/share` times as far apart as the whole lane:
///
/// ```text
/// per_set_interval_ms = ceil(lane_target_interval_ms × 10000 / target_share_bps)
/// ```
///
/// So `share = 10000` reproduces the lane interval exactly, and a `3000`-bps set targets one
/// block per `10000/3000 ≈ 3.33` lane intervals. Integer, overflow-saturating, and rejects a
/// zero share (a zero-share set mines no PALW blocks at all — §12.2, no auto-borrow).
pub fn per_set_target_interval_ms(lane_target_interval_ms: u64, target_share_bps: u16) -> Option<u64> {
    if target_share_bps == 0 {
        return None;
    }
    // ceil(a × D / s) = (a × D + s − 1) / s, with u128 to avoid overflow before the divide.
    let numerator = (lane_target_interval_ms as u128).saturating_mul(BPS_DENOMINATOR as u128);
    let share = target_share_bps as u128;
    let ceil = numerator.div_ceil(share);
    Some(u64::try_from(ceil).unwrap_or(u64::MAX))
}

/// §12.1 — the target interval derived the other way, straight from a lane block-rate. Kept as the
/// spec's stated form (`ceil(1000 × 10000 / (BPS × share))`, BPS = blocks-per-SECOND) for the
/// difficulty path that has a rate rather than an interval on hand. `palw_lane_blocks_per_second`
/// is the WHOLE PALW lane's rate; a zero rate or share yields `None`.
pub fn per_set_target_interval_ms_from_rate(palw_lane_blocks_per_second: u64, target_share_bps: u16) -> Option<u64> {
    if target_share_bps == 0 || palw_lane_blocks_per_second == 0 {
        return None;
    }
    let numerator = 1_000u128 * BPS_DENOMINATOR as u128;
    let denominator = (palw_lane_blocks_per_second as u128).saturating_mul(target_share_bps as u128);
    Some(u64::try_from(numerator.div_ceil(denominator)).unwrap_or(u64::MAX))
}

// Cache accounting for the DB store layer (fixed-size records count as one unit; the view
// scales with the number of tracked sets, the PalwBatchViewV1 precedent).
impl kaspa_utils::mem_size::MemSizeEstimator for PalwComputeSetDescriptorV2 {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwComputeSetPolicyV1 {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwModelAllocationPlanV1 {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwComputeSetActivationCertificateV1 {}
impl kaspa_utils::mem_size::MemSizeEstimator for PalwComputeRegistryViewV1 {
    fn estimate_mem_units(&self) -> usize {
        self.sets.len().max(1)
    }
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

    #[error("malformed {0} registry payload (strict borsh decode failed)")]
    MalformedRegistryPayload(&'static str),

    #[error("{0} registry payload is not in canonical byte form (re-encode mismatch)")]
    NonCanonicalRegistryPayload(&'static str),

    #[error("{0} registry payload version {1} is not supported")]
    UnsupportedRegistryPayloadVersion(&'static str, u16),

    #[error("subnetwork byte {0:#04x} is not a Compute Set registry kind (§22.3 fail-closed)")]
    UnhandledRegistrySubnet(u8),

    #[error("registry view already tracks {0} sets — proposal rejected (§23.2)")]
    RegistryViewFull(usize),

    #[error("set {0} cannot enter Shadow without a quorum-verified activation certificate (§17.3/§18.1)")]
    ShadowRequiresActivationCertificate(Hash64),

    #[error("activation certificate for set {0} was not quorum-verified — single-key activation is forbidden (§17.3)")]
    CertificateQuorumNotVerified(Hash64),

    #[error("activation certificate binds set {set} but descriptor hash {descriptor_hash} (§7.1: they must be equal)")]
    CertificateDescriptorMismatch { set: Hash64, descriptor_hash: Hash64 },

    #[error("activation certificate stake is inconsistent: approving {approving} of total {total}")]
    CertificateStakeInvalid { approving: u128, total: u128 },

    #[error("activation certificate targets unregistered set {0}")]
    CertificateForUnregisteredSet(Hash64),

    #[error("emergency halt targets unregistered set {0}")]
    HaltForUnregisteredSet(Hash64),

    #[error("a DIFFERENT activation certificate already exists for set {0} — supersession goes through the view fold, never a silent overwrite")]
    CertificateDiverged(Hash64),

    #[error("compute set registry store fault: {0} — consensus-load-bearing state is unreadable/unwritable")]
    RegistryStoreFault(String),

    #[error("supplied policy record {actual} is not the header-committed policy {expected} (§14/§23.4)")]
    CommittedPolicyMismatch { expected: Hash64, actual: Hash64 },

    #[error("committed policy governs set {policy_set}, but the header claims set {header_set} (§13.1)")]
    PolicyGovernsDifferentSet { policy_set: Hash64, header_set: Hash64 },

    #[error("supplied plan record {actual} is not the header-committed plan {expected} (§14/§23.4)")]
    CommittedPlanMismatch { expected: Hash64, actual: Hash64 },

    #[error("committed policy effective at {effective_from_daa} was not yet in force at source DAA {source_daa_score} (§13.1)")]
    PolicyNotEffectiveAtSource { effective_from_daa: u64, source_daa_score: u64 },

    #[error("committed plan effective at {effective_from_daa} was not yet in force at source DAA {source_daa_score} (§13.1)")]
    PlanNotEffectiveAtSource { effective_from_daa: u64, source_daa_score: u64 },

    #[error("source block's committed policy state is {0:?}, not Active (§13.1)")]
    SourcePolicyNotActive(ComputeSetState),

    #[error("committed allocation plan has no entry for source set {0} (§13.1)")]
    PlanOmitsSourceSet(Hash64),

    #[error("committed allocation plan gives source set {0} zero share — no PALW block may cite it (§13.1)")]
    SourceSetHasZeroShare(Hash64),
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
        assert_eq!(PALW_SUPPORTED_SETS_ROOT_DOMAIN, b"misaka-palw-supported-sets-root-v1");
        assert_eq!(PALW_PUBLIC_LEAF_V2_DOMAIN, b"misaka-palw-public-leaf-v2");
        assert_eq!(PALW_COMPUTE_SET_AUDIT_CERT_DOMAIN, b"misaka-palw-compute-set-audit-cert-v1");
        let domains = [
            PALW_COMPUTE_SET_ID_DOMAIN,
            PALW_COMPUTE_VM_ID_DOMAIN,
            PALW_COMPUTE_POLICY_ID_DOMAIN,
            PALW_ALLOCATION_PLAN_ID_DOMAIN,
            PALW_SUPPORTED_SETS_ROOT_DOMAIN,
            PALW_PUBLIC_LEAF_V2_DOMAIN,
            PALW_COMPUTE_SET_AUDIT_CERT_DOMAIN,
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

    fn precedent(sequence: u64, state: ComputeSetState) -> Option<PolicyPrecedent> {
        Some(PolicyPrecedent { highest_sequence: sequence, state })
    }

    #[test]
    fn policy_progression_rules() {
        let caps = PolicyGovernanceCapsV1::default();
        let set = descriptor(1).compute_set_id();
        use ComputeSetState::*;

        // Unregistered descriptor → reject.
        let first = policy(set, 1, 100, Proposed);
        assert!(matches!(
            validate_policy_progression(false, false, None, None, &first, &caps, 50),
            Err(ComputeSetRegistryError::PolicyForUnregisteredSet(_))
        ));
        // First policy must be Proposed.
        let premature = policy(set, 1, 100, Active);
        assert!(matches!(
            validate_policy_progression(true, true, None, None, &premature, &caps, 50),
            Err(ComputeSetRegistryError::FirstPolicyMustBeProposed(Active))
        ));
        // Happy path: Proposed inserted (no certificate needed to exist as a proposal).
        assert_eq!(
            validate_policy_progression(true, false, None, None, &first, &caps, 50),
            Ok(PolicyRegistrationOutcome::Inserted)
        );

        // Shadow requires the quorum-verified activation certificate (§17.3/§18.1)…
        let shadow = policy(set, 2, 200, Shadow);
        assert!(matches!(
            validate_policy_progression(true, false, precedent(1, Proposed), None, &shadow, &caps, 150),
            Err(ComputeSetRegistryError::ShadowRequiresActivationCertificate(_))
        ));
        // …and proceeds once certified; then Shadow → Active.
        assert_eq!(
            validate_policy_progression(true, true, precedent(1, Proposed), None, &shadow, &caps, 150),
            Ok(PolicyRegistrationOutcome::Inserted)
        );
        let active = policy(set, 3, 300, Active);
        assert_eq!(
            validate_policy_progression(true, true, precedent(2, Shadow), None, &active, &caps, 250),
            Ok(PolicyRegistrationOutcome::Inserted)
        );

        // Sequence rollback.
        let rollback = policy(set, 3, 400, Active);
        assert!(matches!(
            validate_policy_progression(true, true, precedent(3, Active), None, &rollback, &caps, 350),
            Err(ComputeSetRegistryError::PolicySequenceRollback { .. })
        ));
        // Same sequence: identical content id idempotent, divergent content rejected.
        assert_eq!(
            validate_policy_progression(true, true, precedent(3, Active), Some(active.policy_id()), &active, &caps, 350),
            Ok(PolicyRegistrationOutcome::Idempotent)
        );
        let mut divergent = active.clone();
        divergent.job_timeout_daa += 1;
        assert!(matches!(
            validate_policy_progression(true, true, precedent(3, Active), Some(active.policy_id()), &divergent, &caps, 350),
            Err(ComputeSetRegistryError::PolicySequenceDiverged { .. })
        ));
        // Past-dated effective.
        let past = policy(set, 4, 300, Active);
        assert!(matches!(
            validate_policy_progression(true, true, precedent(3, Active), None, &past, &caps, 350),
            Err(ComputeSetRegistryError::PolicyEffectiveNotFuture { .. })
        ));
        // Illegal transition Proposed → Active (skipping the mandatory Shadow soak).
        let skip = policy(set, 2, 200, Active);
        assert!(matches!(
            validate_policy_progression(true, true, precedent(1, Proposed), None, &skip, &caps, 150),
            Err(ComputeSetRegistryError::InvalidStateTransition { .. })
        ));
        // Retired terminal.
        let after_retired = policy(set, 5, 500, Active);
        assert!(matches!(
            validate_policy_progression(true, true, precedent(4, Retired), None, &after_retired, &caps, 450),
            Err(ComputeSetRegistryError::RetiredSetIsTerminal(_))
        ));

        // §22.2 caps.
        let mut heavy = policy(set, 4, 400, Active);
        heavy.weight_factor_bps = 10_001;
        assert!(matches!(
            validate_policy_progression(true, true, precedent(3, Active), None, &heavy, &caps, 350),
            Err(ComputeSetRegistryError::WeightFactorOutOfRange(10_001))
        ));
        let mut pricey = policy(set, 4, 400, Active);
        pricey.premium_pi_bps = caps.max_premium_pi_bps + 1;
        assert!(matches!(
            validate_policy_progression(true, true, precedent(3, Active), None, &pricey, &caps, 350),
            Err(ComputeSetRegistryError::PremiumAboveCap { .. })
        ));
        let mut zero_scale = policy(set, 4, 400, Active);
        zero_scale.compute_work_scale = 0;
        assert!(matches!(
            validate_policy_progression(true, true, precedent(3, Active), None, &zero_scale, &caps, 350),
            Err(ComputeSetRegistryError::ActiveRequiresNonzero("compute_work_scale"))
        ));
        // Shadow (or any non-Active) must carry zero weight — Shadow soaks earn nothing (§8.1).
        let mut weighted_shadow = policy(set, 2, 200, Shadow);
        weighted_shadow.weight_factor_bps = 1;
        assert!(matches!(
            validate_policy_progression(true, true, precedent(1, Proposed), None, &weighted_shadow, &caps, 150),
            Err(ComputeSetRegistryError::NonActiveRequiresZeroWeight(Shadow))
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

    fn proposal(tag: u8) -> PalwComputeSetProposalV1 {
        PalwComputeSetProposalV1 {
            version: PALW_COMPUTE_SET_PROPOSAL_PAYLOAD_VERSION,
            descriptor: descriptor(tag),
            proposer_credential: h(0x50),
            proposal_bond_ref: TransactionOutpoint::new(h(0x51), 0),
            artifact_distribution_root: h(0x52),
            independent_build_attestation_root: h(0x53),
            requested_shadow_activation_daa: 1_000,
        }
    }

    fn certificate_for(set: Hash64) -> PalwComputeSetActivationCertificateV1 {
        PalwComputeSetActivationCertificateV1 {
            version: PALW_COMPUTE_SET_ACTIVATION_CERT_VERSION,
            compute_set_id: set,
            descriptor_hash: set,
            conformance_result_root: h(0x60),
            auditor_capacity_evidence_root: h(0x61),
            artifact_reproducibility_root: h(0x62),
            validator_set_commitment: h(0x63),
            approving_stake: 21_000_000,
            total_selected_stake: 30_000_000,
            effective_from_daa: 500,
            votes_root: h(0x64),
        }
    }

    #[test]
    fn registry_payload_parse_is_strict_and_versioned() {
        use crate::subnets::*;
        let proposal_kind = SUBNETWORK_ID_PALW_COMPUTE_SET_PROPOSAL.palw_compute_registry_tx_kind().unwrap();
        let plan_kind = SUBNETWORK_ID_PALW_MODEL_ALLOCATION_PLAN.palw_compute_registry_tx_kind().unwrap();
        let halt_kind = SUBNETWORK_ID_PALW_COMPUTE_SET_EMERGENCY_HALT.palw_compute_registry_tx_kind().unwrap();

        // Round-trips.
        let p = proposal(1);
        let bytes = borsh::to_vec(&p).unwrap();
        assert_eq!(parse_palw_compute_registry(proposal_kind, &bytes), Ok(PalwComputeRegistryEffect::Proposal(p.clone())));
        let a_plan = plan(1, 100, vec![(h(0xa1), 10_000)]);
        assert_eq!(
            parse_palw_compute_registry(plan_kind, &borsh::to_vec(&a_plan).unwrap()),
            Ok(PalwComputeRegistryEffect::AllocationPlan(a_plan))
        );
        let halt = PalwComputeSetEmergencyHaltV1 {
            version: PALW_COMPUTE_SET_EMERGENCY_HALT_VERSION,
            compute_set_id: h(0xa1),
            evidence_root: Hash64::default(),
        };
        assert_eq!(
            parse_palw_compute_registry(halt_kind, &borsh::to_vec(&halt).unwrap()),
            Ok(PalwComputeRegistryEffect::EmergencyHalt(halt))
        );

        // Strictness: trailing bytes are not canonical.
        let mut padded = bytes.clone();
        padded.push(0);
        assert!(matches!(
            parse_palw_compute_registry(proposal_kind, &padded),
            Err(ComputeSetRegistryError::MalformedRegistryPayload("compute_set_proposal"))
        ));
        // Version pin.
        let mut wrong_version = p.clone();
        wrong_version.version = 2;
        assert!(matches!(
            parse_palw_compute_registry(proposal_kind, &borsh::to_vec(&wrong_version).unwrap()),
            Err(ComputeSetRegistryError::UnsupportedRegistryPayloadVersion("compute_set_proposal", 2))
        ));
        // Unknown band byte fails closed.
        assert!(matches!(
            parse_palw_compute_registry(0x45, &bytes),
            Err(ComputeSetRegistryError::UnhandledRegistrySubnet(0x45))
        ));
    }

    #[test]
    fn registry_view_full_lifecycle() {
        let caps = PolicyGovernanceCapsV1::default();
        let mut view = PalwComputeRegistryViewV1::new();
        let p = proposal(1);
        let set = p.descriptor.compute_set_id();
        use ComputeSetState::*;

        // Proposal: insert, then idempotent; state is Proposed.
        assert_eq!(view.apply_proposal(&p), Ok(DescriptorRegistrationOutcome::Inserted));
        assert_eq!(view.apply_proposal(&p), Ok(DescriptorRegistrationOutcome::Idempotent));
        assert_eq!(view.sets[&set].current_state(), Proposed);

        // First policy (Proposed) lands without a certificate.
        assert_eq!(view.apply_policy(&policy(set, 1, 100, Proposed), &caps, 50), Ok(PolicyRegistrationOutcome::Inserted));
        // Shadow blocked until the certificate is recorded.
        assert!(matches!(
            view.apply_policy(&policy(set, 2, 200, Shadow), &caps, 150),
            Err(ComputeSetRegistryError::ShadowRequiresActivationCertificate(_))
        ));

        // Certificate gating (§17.3).
        let cert = certificate_for(set);
        assert!(matches!(
            view.apply_certificate(&cert, false),
            Err(ComputeSetRegistryError::CertificateQuorumNotVerified(_))
        ));
        let mut mismatched = cert.clone();
        mismatched.descriptor_hash = h(0x99);
        assert!(matches!(
            view.apply_certificate(&mismatched, true),
            Err(ComputeSetRegistryError::CertificateDescriptorMismatch { .. })
        ));
        let mut overstaked = cert.clone();
        overstaked.approving_stake = overstaked.total_selected_stake + 1;
        assert!(matches!(view.apply_certificate(&overstaked, true), Err(ComputeSetRegistryError::CertificateStakeInvalid { .. })));
        let mut unknown_set = cert.clone();
        unknown_set.compute_set_id = h(0x77);
        unknown_set.descriptor_hash = h(0x77);
        assert!(matches!(
            view.apply_certificate(&unknown_set, true),
            Err(ComputeSetRegistryError::CertificateForUnregisteredSet(_))
        ));
        assert_eq!(view.apply_certificate(&cert, true), Ok(()));

        // Shadow → Active now proceed; the index resolves by DAA.
        assert_eq!(view.apply_policy(&policy(set, 2, 200, Shadow), &caps, 150), Ok(PolicyRegistrationOutcome::Inserted));
        let active_policy = policy(set, 3, 300, Active);
        assert_eq!(view.apply_policy(&active_policy, &caps, 250), Ok(PolicyRegistrationOutcome::Inserted));
        assert_eq!(view.sets[&set].resolve_policy_id_at(50), None);
        assert_eq!(view.sets[&set].resolve_policy_id_at(250), Some(policy(set, 2, 200, Shadow).policy_id()));
        assert_eq!(view.sets[&set].resolve_policy_id_at(900), Some(active_policy.policy_id()));
        // Idempotent replay via content id.
        assert_eq!(view.apply_policy(&active_policy, &caps, 350), Ok(PolicyRegistrationOutcome::Idempotent));

        // Allocation plan lifecycle on the live view.
        let limits = AllocationGovernanceLimitsV1 { max_change_per_plan_bps: 10_000, ..Default::default() };
        let p1 = plan(1, 400, vec![(set, 10_000)]);
        assert_eq!(view.apply_plan(&p1, &limits, 350), Ok(PlanRegistrationOutcome::Inserted));
        assert_eq!(view.apply_plan(&p1, &limits, 350), Ok(PlanRegistrationOutcome::Idempotent));
        let mut diverged = plan(1, 401, vec![(set, 10_000)]);
        diverged.plan_id = diverged.derive_plan_id();
        assert!(matches!(view.apply_plan(&diverged, &limits, 350), Err(ComputeSetRegistryError::PlanSequenceDiverged(1))));
        assert_eq!(view.resolve_plan_id_at(400), Some(p1.plan_id));
        assert_eq!(view.resolve_plan_id_at(50), None);

        // Emergency halt: immediate state override; nonzero share now rejected; recovery via a
        // policy the §8.1 matrix admits from EmergencyHalted clears the halt.
        assert!(matches!(
            view.apply_halt(&PalwComputeSetEmergencyHaltV1 {
                version: PALW_COMPUTE_SET_EMERGENCY_HALT_VERSION,
                compute_set_id: h(0x88),
                evidence_root: Hash64::default(),
            }),
            Err(ComputeSetRegistryError::HaltForUnregisteredSet(_))
        ));
        let halt = PalwComputeSetEmergencyHaltV1 {
            version: PALW_COMPUTE_SET_EMERGENCY_HALT_VERSION,
            compute_set_id: set,
            evidence_root: h(0x89),
        };
        view.apply_halt(&halt).unwrap();
        assert_eq!(view.sets[&set].current_state(), EmergencyHalted);
        let banned = plan(2, 500, vec![(set, 10_000)]);
        assert!(matches!(
            view.apply_plan(&banned, &limits, 450),
            Err(ComputeSetRegistryError::NonActiveSetWithShare { .. })
        ));
        let recovery = policy(set, 4, 600, Active);
        assert_eq!(view.apply_policy(&recovery, &caps, 550), Ok(PolicyRegistrationOutcome::Inserted));
        assert!(!view.sets[&set].halted);
        assert_eq!(view.sets[&set].current_state(), Active);

        // View snapshot round-trips (the block-keyed persistence form).
        let bytes = borsh::to_vec(&view).unwrap();
        assert_eq!(borsh::from_slice::<PalwComputeRegistryViewV1>(&bytes).unwrap(), view);
    }

    /// §14 — the historical resolver returns the committed records' numbers and rejects every
    /// substitution/expiry/inactive path (§28.3's core half: past work is a pure function of the
    /// exact committed records, so later governance can never move it).
    #[test]
    fn historical_credit_resolution() {
        let set = h(0xa1);
        let mut active = policy(set, 3, 300, ComputeSetState::Active);
        active.compute_work_scale = 41_692;
        active.weight_factor_bps = 2_500;
        let plan_v1 = plan(1, 300, vec![(set, 10_000)]);
        let policy_id = active.policy_id();
        let plan_id = plan_v1.plan_id;

        let resolved = resolve_source_policy_for_credit(&active, &plan_v1, policy_id, plan_id, set, 500).unwrap();
        assert_eq!(resolved, HistoricalComputeResolution { compute_work_scale: 41_692, weight_factor_bps: 2_500 });

        // A different revision cannot stand in for the committed one (§23.4 historical rewrite).
        let mut newer = active.clone();
        newer.policy_sequence = 4;
        newer.compute_work_scale = 999_999;
        assert!(matches!(
            resolve_source_policy_for_credit(&newer, &plan_v1, policy_id, plan_id, set, 500),
            Err(ComputeSetRegistryError::CommittedPolicyMismatch { .. })
        ));
        // Wrong set binding.
        assert!(matches!(
            resolve_source_policy_for_credit(&active, &plan_v1, policy_id, plan_id, h(0xb2), 500),
            Err(ComputeSetRegistryError::PolicyGovernsDifferentSet { .. })
        ));
        // Not yet effective at the source DAA.
        assert!(matches!(
            resolve_source_policy_for_credit(&active, &plan_v1, policy_id, plan_id, set, 200),
            Err(ComputeSetRegistryError::PolicyNotEffectiveAtSource { .. })
        ));
        // Inactive policy state.
        let mut halted = active.clone();
        halted.state = ComputeSetState::EmergencyHalted;
        halted.weight_factor_bps = 0;
        let halted_id = halted.policy_id();
        assert!(matches!(
            resolve_source_policy_for_credit(&halted, &plan_v1, halted_id, plan_id, set, 500),
            Err(ComputeSetRegistryError::SourcePolicyNotActive(ComputeSetState::EmergencyHalted))
        ));
        // Plan omits / zeroes the set.
        let other_only = plan(2, 300, vec![(h(0xb2), 10_000)]);
        assert!(matches!(
            resolve_source_policy_for_credit(&active, &other_only, policy_id, other_only.plan_id, set, 500),
            Err(ComputeSetRegistryError::PlanOmitsSourceSet(_))
        ));

        // Credit arithmetic: floor(normalized × weight / 10000), saturating on overflow.
        let normalized = crate::BlueWorkType::from_u64(1_000_000);
        assert_eq!(credited_compute_work(normalized, 2_500), crate::BlueWorkType::from_u64(250_000));
        assert_eq!(credited_compute_work(normalized, 0), crate::BlueWorkType::from_u64(0));
        assert_eq!(credited_compute_work(normalized, 10_000), normalized);
        // Overflow saturates to MAX before the divide (the normalize_palw_work convention).
        assert_eq!(
            credited_compute_work(crate::BlueWorkType::MAX, 10_000),
            crate::BlueWorkType::MAX / crate::BlueWorkType::from_u64(10_000)
        );
    }

    #[test]
    fn per_set_target_interval() {
        // Full share reproduces the lane interval exactly.
        assert_eq!(per_set_target_interval_ms(1_000, 10_000), Some(1_000));
        // Half share ⇒ twice the interval; a 3000-bps set ⇒ ceil(10000/3000)×interval.
        assert_eq!(per_set_target_interval_ms(1_000, 5_000), Some(2_000));
        assert_eq!(per_set_target_interval_ms(1_000, 3_000), Some(3_334)); // ceil(10_000_000/3000)
        // §12.2: a zero-share set targets nothing (no auto-borrow).
        assert_eq!(per_set_target_interval_ms(1_000, 0), None);
        // Monotonic: smaller share ⇒ strictly longer interval.
        let full = per_set_target_interval_ms(400, 10_000).unwrap();
        let tenth = per_set_target_interval_ms(400, 1_000).unwrap();
        assert!(tenth > full);
        // Overflow saturates rather than wrapping.
        assert_eq!(per_set_target_interval_ms(u64::MAX, 1), Some(u64::MAX));

        // Rate form: 10 BPS whole lane, 100% share ⇒ 100 ms; 25% share ⇒ 400 ms.
        assert_eq!(per_set_target_interval_ms_from_rate(10, 10_000), Some(100));
        assert_eq!(per_set_target_interval_ms_from_rate(10, 2_500), Some(400));
        assert_eq!(per_set_target_interval_ms_from_rate(0, 10_000), None);
        assert_eq!(per_set_target_interval_ms_from_rate(10, 0), None);
    }

    #[test]
    fn registry_view_is_bounded() {
        let mut view = PalwComputeRegistryViewV1::new();
        for i in 0..MAX_REGISTRY_VIEW_SETS {
            let mut d = descriptor(1);
            let mut family = [0u8; 64];
            family[..8].copy_from_slice(&(i as u64).to_le_bytes());
            d.model_family_id = Hash64::from_bytes(family);
            let p = PalwComputeSetProposalV1 { descriptor: d, ..proposal(1) };
            assert_eq!(view.apply_proposal(&p), Ok(DescriptorRegistrationOutcome::Inserted));
        }
        let mut d = descriptor(1);
        d.model_family_id = h(0xfe);
        let overflow = PalwComputeSetProposalV1 { descriptor: d, ..proposal(1) };
        assert!(matches!(view.apply_proposal(&overflow), Err(ComputeSetRegistryError::RegistryViewFull(_))));
    }

    // =========================================================================================
    // §19 — capability V2
    // =========================================================================================

    fn capability(sets_root: Hash64) -> PalwProviderCapabilityV2 {
        PalwProviderCapabilityV2 {
            version: PALW_PROVIDER_CAPABILITY_VERSION,
            credential_id: h(0xc0),
            supported_compute_sets_root: sets_root,
            conformance_valid_until: 100,
            capability_sequence: 1,
            bonded_value: 1_000_000,
            bond_state: BondState::Active,
        }
    }

    #[test]
    fn bond_state_wire_form_is_pinned_u8_and_fail_closed() {
        for (state, raw) in
            [(BondState::Maturing, 0u8), (BondState::Active, 1), (BondState::Unbonding, 2), (BondState::Slashed, 3)]
        {
            assert_eq!(borsh::to_vec(&state).unwrap(), vec![raw]);
            assert_eq!(borsh::from_slice::<BondState>(&[raw]).unwrap(), state);
        }
        // Unknown wire value fails closed — an old node must never coerce a future state.
        assert!(borsh::from_slice::<BondState>(&[4]).is_err());
    }

    #[test]
    fn supported_sets_root_requires_canonical_sorted_unique() {
        let (a, b, c) = (h(1), h(2), h(3));
        // Canonical: sorted-unique (h(n) bytes sort by tag).
        let root = palw_supported_compute_sets_root(&[a, b, c]).unwrap();
        // Unsorted and duplicate forms are refused outright — no silent normalization.
        assert_eq!(palw_supported_compute_sets_root(&[b, a, c]), None);
        assert_eq!(palw_supported_compute_sets_root(&[a, a, b]), None);
        // The root is count- and content-sensitive: subset / superset / disjoint all differ.
        let of = |sets: &[Hash64]| palw_supported_compute_sets_root(sets).unwrap();
        assert_ne!(root, of(&[a, b]));
        assert_ne!(of(&[a]), of(&[b]));
        // The empty capability (no supported set) is canonical and distinct.
        assert_ne!(of(&[]), of(&[a]));
    }

    #[test]
    fn capability_supports_set_checks_commitment_before_membership() {
        let sets = [h(1), h(2), h(3)];
        let cap = capability(palw_supported_compute_sets_root(&sets).unwrap());
        assert!(cap.supports_set(&sets, &h(2)));
        assert!(!cap.supports_set(&sets, &h(4))); // committed list, but not a member
        // A preimage that does not match the commitment proves nothing — even if it contains
        // the set (§19: the root is the only on-chain field, the list is untrusted input).
        let forged = [h(1), h(2), h(3), h(4)];
        assert!(!cap.supports_set(&forged, &h(4)));
        // A non-canonical (unsorted) preimage can never match any commitment.
        assert!(!cap.supports_set(&[h(2), h(1), h(3)], &h(2)));
    }

    #[test]
    fn capability_selection_gate() {
        let sets = [h(1), h(2)];
        let root = palw_supported_compute_sets_root(&sets).unwrap();
        let cap = capability(root);
        // Every §19 on-chain gate passes (availability is the caller's separate DA gate).
        assert!(cap.eligible_for_selection(&h(1), &sets, 50));
        // Conformance boundary is inclusive: valid THROUGH `conformance_valid_until`.
        assert!(cap.eligible_for_selection(&h(1), &sets, 100));
        assert!(!cap.eligible_for_selection(&h(1), &sets, 101)); // conformance expired
        assert!(!cap.eligible_for_selection(&h(3), &sets, 50)); // set not supported
        // Every non-Active bond state is unselectable (§19 matured/not-unbonding/not-slashed).
        for state in [BondState::Maturing, BondState::Unbonding, BondState::Slashed] {
            let gated = PalwProviderCapabilityV2 { bond_state: state, ..capability(root) };
            assert!(!gated.eligible_for_selection(&h(1), &sets, 50));
        }
        // Unknown capability version fails closed.
        let vnext = PalwProviderCapabilityV2 { version: 3, ..capability(root) };
        assert!(!vnext.eligible_for_selection(&h(1), &sets, 50));
    }

    // =========================================================================================
    // §20.2 / §20.3 — leaf V2 and the set-centric audit certificate
    // =========================================================================================

    fn leaf_v2() -> PalwPublicLeafV2 {
        PalwPublicLeafV2 {
            version: PALW_PUBLIC_LEAF_V2_VERSION,
            compute_set_id: h(1),
            compute_policy_id: h(2),
            job_nullifier: h(3),
            challenge_commitment: h(4),
            replica_set_root: h(5),
            output_commitment: h(6),
            schedule_root: h(7),
            execution_root: h(8),
            route_root: h(9),
            state_root: h(10),
            canonical_compute_units: 41_692,
            receipt_da_root: h(11),
            reward_set_root: h(12),
        }
    }

    #[test]
    fn leaf_v2_roundtrip_and_content_sensitive_hash() {
        let leaf = leaf_v2();
        let bytes = borsh::to_vec(&leaf).unwrap();
        assert_eq!(borsh::from_slice::<PalwPublicLeafV2>(&bytes).unwrap(), leaf);
        // Every commitment field moves the leaf hash — spot-check the set binding (§20.2's
        // whole point: the set id is first-class, not a trailing receipt field).
        let mut other = leaf_v2();
        other.compute_set_id = h(0x55);
        assert_ne!(leaf.leaf_hash(), other.leaf_hash());
        let mut policy_swap = leaf_v2();
        policy_swap.compute_policy_id = h(0x56);
        assert_ne!(leaf.leaf_hash(), policy_swap.leaf_hash());
    }

    #[test]
    fn audit_certificate_roundtrip_and_content_identity() {
        let cert = PalwComputeSetAuditCertificateV1 {
            version: PALW_COMPUTE_SET_AUDIT_CERT_VERSION,
            compute_set_id: h(1),
            descriptor_hash: h(2),
            compute_policy_id: h(3),
            leaf_root: h(4),
            auditor_capability_root: h(5),
            sample_plan_root: h(6),
            replay_result_root: h(7),
            certificate_epoch: 9,
            approving_stake: 700,
            total_selected_stake: 1000,
            votes_root: h(8),
        };
        let bytes = borsh::to_vec(&cert).unwrap();
        assert_eq!(borsh::from_slice::<PalwComputeSetAuditCertificateV1>(&bytes).unwrap(), cert);
        // The declared tally is a commitment: two certificates differing only in
        // `approving_stake` are different objects (the batch-cert V2 idiom).
        let mut inflated = cert.clone();
        inflated.approving_stake = 1000;
        assert_ne!(cert.certificate_hash(), inflated.certificate_hash());
        // And the policy binding is content-relevant (ADR-MA-007: the EXACT revision).
        let mut repoliced = cert.clone();
        repoliced.compute_policy_id = h(0x33);
        assert_ne!(cert.certificate_hash(), repoliced.certificate_hash());
    }
}
