//! ADR-0039 PALW Replica-GEMM — provider-side deterministic-runtime identity (design §6, §7, §21).
//!
//! The **two runtime tiers** that widen the participation base, and the exact-match commitment
//! helpers a provider computes off-chain. Nothing here touches consensus; it pins the identities the
//! k=2 replica match compares (design §7.5): `model_profile_id`, `runtime_class_id`, `shape_id`,
//! `job_set_commitment`, `output_commitment`, `canonical_gemm_trace_root`,
//! `operation_schedule_commitment`. A candidate leaf is minted only when two independent providers
//! agree on all of these — but **exact-match weight is granted only within one `runtime_class_id`**
//! (invariant I-9): a `PALW Standard` (4B) leaf pairs with a Standard leaf, a `PALW Quality` (9B)
//! leaf with a Quality leaf, never cross-tier.
//!
//! Consensus pins the exact **manifest hash**, never the human model name; the tier `project_name`s
//! here (`MISAKA-QW4/QW9-PALW-v1`) are the fixed project forks, turned into a stable id by a keyed
//! hash so an ambiguous common name is never used as a wire identity.

use crate::domains::{
    MIL_PALW_EXEC_CHALLENGE_DOMAIN, MIL_PALW_GEMM_TRACE_DOMAIN, MIL_PALW_JOB_CAPABILITY_DOMAIN, MIL_PALW_JOBSET_DOMAIN,
    MIL_PALW_OP_ID_DOMAIN, MIL_PALW_OP_SCHEDULE_DOMAIN, MIL_PALW_OUTPUT_DOMAIN, MIL_PALW_PROFILE_DOMAIN,
    MIL_PALW_RUNTIME_CLASS_DOMAIN, MIL_PALW_SHAPE_DOMAIN, MIL_PALW_TIER_MODEL_DOMAIN, MIL_PALW_TRACE_STEP_DOMAIN,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{HASH64_SIZE, Hash64, blake2b_512_keyed};

// =============================================================================================
// The two participation tiers (design §6.1, ADR-0039 D8).
// =============================================================================================

/// The fixed project fork name of the **Standard** tier — Qwen3.5-4B Q4, RAM ≥ 8 GB, VPS / node
/// co-location / broad participation.
pub const PALW_TIER_STANDARD_NAME: &[u8] = b"MISAKA-QW4-PALW-v1";
/// The fixed project fork name of the **Quality** tier — `huihui_ai/Qwen3.6-abliterated:35b-Claude-4.7`
/// at Q4, RAM ≥ 24 GB, the launch (QW9-genesis-slot) model. The fork name pins a NEW `model_id` (a
/// different model is a different runtime class); PALW is inert, so this is a launch-spec change, not a
/// re-genesis of any live net.
pub const PALW_TIER_QUALITY_NAME: &[u8] = b"MISAKA-QW35A-PALW-v1";

/// A PALW runtime tier. Two tiers today; the enum is the taxonomy the match/quota logic keys on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwTier {
    /// `MISAKA-QW4-PALW-v1` — Qwen3.5-4B Q4, RAM ≥ 8 GB.
    Standard,
    /// `MISAKA-QW35A-PALW-v1` — `huihui_ai/Qwen3.6-abliterated:35b-Claude-4.7` Q4, RAM ≥ 24 GB (launch model).
    Quality,
}

impl PalwTier {
    /// The fixed project fork name pinned on-chain.
    pub const fn project_name(self) -> &'static [u8] {
        match self {
            PalwTier::Standard => PALW_TIER_STANDARD_NAME,
            PalwTier::Quality => PALW_TIER_QUALITY_NAME,
        }
    }

    /// Minimum provider RAM (GiB) — the participation-widening lever. Advisory (providers self-select
    /// by capacity), not a consensus check.
    pub const fn ram_floor_gib(self) -> u32 {
        match self {
            PalwTier::Standard => 8,
            PalwTier::Quality => 24,
        }
    }

    /// The source model label (documentation / provider tooling).
    pub const fn source_model(self) -> &'static str {
        match self {
            PalwTier::Standard => "Qwen3.5-4B",
            PalwTier::Quality => "huihui_ai/Qwen3.6-abliterated:35b-Claude-4.7",
        }
    }

    /// `tier_model_id = Hash64_k(tier-model, project_name)` — the stable id for the pinned fork name.
    pub fn model_id(self) -> Hash64 {
        blake2b_512_keyed(MIL_PALW_TIER_MODEL_DOMAIN, self.project_name())
    }
}

// =============================================================================================
// Runtime profile (design §6.2). The pinned deterministic runtime; a change to any field changes
// `runtime_class_id`, so a differently-built runtime can never exact-match.
// =============================================================================================

/// Minimal sampling parameters. v0.2 requires greedy decode (`temperature_milli == 0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSamplingParams {
    /// True = greedy / argmax (required in v0.2).
    pub greedy: bool,
    /// Temperature × 1000, integer-pinned. 0 for greedy.
    pub temperature_milli: u32,
}

impl PalwSamplingParams {
    pub const fn greedy() -> Self {
        Self { greedy: true, temperature_milli: 0 }
    }
}

/// The pinned deterministic runtime for one tier (design §6.2).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRuntimeProfileV1 {
    pub version: u16,
    pub tier: PalwTier,
    /// [`PalwTier::model_id`] of the tier fork, or a manifest-derived id.
    pub model_id: Hash64,
    pub tokenizer_hash: Hash64,
    pub quantization_manifest_hash: Hash64,
    pub runtime_image_hash: Hash64,
    pub kernel_graph_hash: Hash64,
    pub operation_table_hash: Hash64,
    pub shape_table_hash: Hash64,
    pub gpu_arch_class: u32,
    pub tensor_parallel_degree: u16,
    pub pipeline_parallel_degree: u16,
    pub deterministic_reduction: bool,
    pub batch_invariant: bool,
    pub speculative_decode: bool,
    pub sampling: PalwSamplingParams,
}

impl PalwRuntimeProfileV1 {
    /// `model_profile_id` = keyed hash over the *model* identity (model ‖ tokenizer ‖ quant ‖ shape
    /// table). Independent of the serving stack, so the same weights served two ways share a profile
    /// but differ in [`Self::runtime_class_id`].
    pub fn model_profile_id(&self) -> Hash64 {
        let mut p = Vec::with_capacity(4 * HASH64_SIZE + 2);
        p.extend_from_slice(&self.version.to_le_bytes());
        for h in [&self.model_id, &self.tokenizer_hash, &self.quantization_manifest_hash, &self.shape_table_hash] {
            p.extend_from_slice(h.as_byte_slice());
        }
        blake2b_512_keyed(MIL_PALW_PROFILE_DOMAIN, &p)
    }

    /// `runtime_class_id` = keyed hash binding the model profile to the exact serving stack (runtime
    /// image ‖ kernel graph ‖ operation table ‖ GPU-arch class ‖ TP/PP topology ‖ determinism flags).
    /// Exact-match weight is granted **only within one class** (invariant I-9). A CPU vs GPU vs SKU
    /// difference (different `gpu_arch_class`/`kernel_graph_hash`) is a distinct class.
    pub fn runtime_class_id(&self) -> Hash64 {
        let profile = self.model_profile_id();
        let mut p = Vec::with_capacity(5 * HASH64_SIZE + 16);
        p.extend_from_slice(profile.as_byte_slice());
        for h in [&self.runtime_image_hash, &self.kernel_graph_hash, &self.operation_table_hash] {
            p.extend_from_slice(h.as_byte_slice());
        }
        p.extend_from_slice(&self.gpu_arch_class.to_le_bytes());
        p.extend_from_slice(&self.tensor_parallel_degree.to_le_bytes());
        p.extend_from_slice(&self.pipeline_parallel_degree.to_le_bytes());
        p.push(self.deterministic_reduction as u8);
        p.push(self.batch_invariant as u8);
        p.push(self.speculative_decode as u8);
        p.push(self.sampling.greedy as u8);
        p.extend_from_slice(&self.sampling.temperature_milli.to_le_bytes());
        blake2b_512_keyed(MIL_PALW_RUNTIME_CLASS_DOMAIN, &p)
    }

    /// The v0.2 determinism gate (design §6.2 / §30.2): greedy, batch-invariant, deterministic
    /// reduction, and **no** speculative decoding. A profile that fails this must not be registered
    /// as an exact-match Replica class (the runtime should fail startup, §27.1).
    pub fn is_deterministic_admissible(&self) -> bool {
        self.sampling.greedy
            && self.sampling.temperature_milli == 0
            && self.deterministic_reduction
            && self.batch_invariant
            && !self.speculative_decode
    }
}

// =============================================================================================
// Exact-match commitment helpers (the fields the k=2 match compares — design §7.4/§7.5).
// =============================================================================================

/// `shape_id`-binding hash of a fixed tensor shape descriptor (design §6.3). The wire `shape_id` is a
/// small index into the pinned shape table; this binds that index to the exact shape bytes.
pub fn shape_commitment(shape_descriptor: &[u8]) -> Hash64 {
    blake2b_512_keyed(MIL_PALW_SHAPE_DOMAIN, shape_descriptor)
}

/// `job_set_commitment` over the packed micro-batch descriptor (design §8/§21.4).
pub fn job_set_commitment(job_set_descriptor: &[u8]) -> Hash64 {
    blake2b_512_keyed(MIL_PALW_JOBSET_DOMAIN, job_set_descriptor)
}

/// `output_commitment = Hash64_k(output, salt ‖ output_token_ids)` (design §7.4). The salt (derived
/// from the job secret) defeats a known-question dictionary attack (§19.3); it is fixed-width so the
/// preimage is unambiguous.
pub fn output_commitment(salt: &[u8; 32], output_token_ids: &[u32]) -> Hash64 {
    let mut p = Vec::with_capacity(32 + output_token_ids.len() * 4);
    p.extend_from_slice(salt);
    for t in output_token_ids {
        p.extend_from_slice(&t.to_le_bytes());
    }
    blake2b_512_keyed(MIL_PALW_OUTPUT_DOMAIN, &p)
}

/// `canonical_gemm_trace_root` — commitment over the primary GPU GEMM trace (design §7.2/§7.3). Here
/// a keyed hash over the already-serialized canonical trace; the real trace is a Merkle root computed
/// by the provider backend (`misaka-mil-provider`). Binding it (not just the output) is what makes a
/// match evidence of the *computation*, not merely of equal answers.
pub fn gemm_trace_root(canonical_trace: &[u8]) -> Hash64 {
    blake2b_512_keyed(MIL_PALW_GEMM_TRACE_DOMAIN, canonical_trace)
}

/// `operation_schedule_commitment` over the deterministic operation schedule (design §7.2).
pub fn operation_schedule_commitment(schedule: &[u8]) -> Hash64 {
    blake2b_512_keyed(MIL_PALW_OP_SCHEDULE_DOMAIN, schedule)
}

/// The eight exact-match fields two providers must agree on to mint a candidate leaf (design §7.5,
/// §8.1). Equality of a full `ReplicaMatchKey` is the leaf-minting predicate; the k=2 backend
/// produces one per provider and mints only if `a == b`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplicaMatchKey {
    pub job_set_commitment: Hash64,
    pub model_profile_id: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_id: u16,
    pub output_commitment: Hash64,
    pub canonical_gemm_trace_root: Hash64,
    pub operation_schedule_commitment: Hash64,
    pub quantum_count: u16,
}

impl ReplicaMatchKey {
    /// True iff the two providers' keys are byte-identical across all eight fields (design §7.5). This is
    /// the **within-class** rule: it can only hold for two replicas of the *same* `runtime_class_id`.
    #[inline]
    pub fn exact_match(&self, other: &ReplicaMatchKey) -> bool {
        self == other
    }

    /// ADR-0039 §19.8 — **cross-vendor diverse-replica** k=2 match (token granularity). Accepts a k=2 pair
    /// of two providers that share `job_set_commitment` + `model_profile_id` + `shape_id` + `quantum_count`
    /// and agree on the token `output_commitment`, but carry **different** `runtime_class_id`
    /// (hardware/vendor diversity is *required*, e.g. one Apple-Metal class + one NVIDIA-CUDA class).
    ///
    /// `operation_schedule_commitment` is not compared. The only
    /// compute-binding field in diverse mode is `output_commitment` (a salted hash of the argmax token
    /// ids); both `canonical_gemm_trace_root` and `operation_schedule_commitment` are excluded.
    ///
    /// Why not the raw compute path: under the canonical backend, cross-vendor raw fp32 logits (hence
    /// `canonical_gemm_trace_root`) diverge by ≤1 ULP at long generation, while the argmax token sequence
    /// (hence `output_commitment`) stays bit-identical far longer (measured, canonical-compute §19.5b). So a
    /// cross-vendor pair can only match at token granularity. The argmax-absorbs-approximate-compute weakness
    /// is backstopped by canary rerun + bond slashing + caps (§19.6); the diversity requirement *adds*
    /// anti-collusion strength — a defect/backdoor must reproduce the same argmax INDEPENDENTLY on two
    /// vendors' hardware, strictly harder than agreement on two identical same-vendor boxes.
    ///
    /// This is an ADDITIONAL, explicitly-enabled mode; [`Self::exact_match`] (within-class) is unchanged.
    #[inline]
    pub fn diverse_replica_match(&self, other: &ReplicaMatchKey) -> bool {
        self.job_set_commitment == other.job_set_commitment
            && self.model_profile_id == other.model_profile_id
            && self.shape_id == other.shape_id
            && self.quantum_count == other.quantum_count
            && self.output_commitment == other.output_commitment
            // Diversity is REQUIRED, not merely allowed: the two replicas must be different classes.
            && self.runtime_class_id != other.runtime_class_id
        // NOT compared: `canonical_gemm_trace_root` AND `operation_schedule_commitment` — both are
        // class-dependent (raw fp32 kernels / tile schedule) and diverge cross-vendor by design; the only
        // cross-vendor invariant is the token `output_commitment` (the argmax answer).
    }
}

/// ADR-0039 §6.4 — the FULL deterministic output of one verifiable inference run: the answer tokens
/// plus the compute-path commitments (canonical GEMM trace root, operation schedule, counters). The
/// eight-field [`ReplicaMatchKey`] is a *projection* of this via [`Self::match_key`]: the k=2 dispatch
/// compares keys, but the differential-determinism harness (K0) compares the FULL output so it can
/// localize WHICH field first diverges between two providers that should have matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeterministicInferenceOutputV1 {
    /// One output token-id stream per sequence in the job set (usually one).
    pub output_token_ids: Vec<Vec<u32>>,
    pub output_commitment: Hash64,
    pub canonical_gemm_trace_root: Hash64,
    pub operation_schedule_commitment: Hash64,
    pub operation_counters: PalwOperationCountersV1,
    pub shape_id: u16,
    pub quantum_count: u16,
}

impl DeterministicInferenceOutputV1 {
    /// Project to the exact-match key. The three profile/job-derived fields come from `profile` +
    /// `job_set_descriptor`; the other five are carried verbatim from this output. Two providers of the
    /// same class on the same job mint a leaf iff their projected keys are equal (design §7.5).
    pub fn match_key(&self, profile: &PalwRuntimeProfileV1, job_set_descriptor: &[u8]) -> ReplicaMatchKey {
        ReplicaMatchKey {
            job_set_commitment: job_set_commitment(job_set_descriptor),
            model_profile_id: profile.model_profile_id(),
            runtime_class_id: profile.runtime_class_id(),
            shape_id: self.shape_id,
            output_commitment: self.output_commitment,
            canonical_gemm_trace_root: self.canonical_gemm_trace_root,
            operation_schedule_commitment: self.operation_schedule_commitment,
            quantum_count: self.quantum_count,
        }
    }
}

// =============================================================================================
// §7 — real GEMM as a work source. The canonical operation id + execution challenge + trace-chain
// step that a deterministic runtime absorbs (the REAL GEMM execution needs a GPU; this is the pure
// accounting/commitment layer both the CPU reference and the future CUDA backend share).
// =============================================================================================

/// ADR-0039 §7.2 — the canonical id of one GEMM/attention/router operation in the fixed operation
/// graph of a runtime profile. Serialized little-endian in field order; a provider cannot inflate work
/// by claiming an un-selected MoE expert because only the *selected* expert + canonical schedule enter
/// the trace.
///
/// Canonical Compute v1 §17.5 fix 1 (opaque operation-id): this 13-byte field schema is **owned by
/// `compute_spec_version` / the model spec, NOT by consensus**. It lives entirely in this off-chain crate
/// (`misaka-mil-core`); consensus has NO dependency on it and never parses these fields. The op-id reaches
/// consensus only after being irreversibly folded (via [`trace_chain_step`]) into the single opaque
/// `canonical_gemm_trace_root: Hash64`, which the validator treats as a model-opaque commitment. A future
/// architecture that drops `expert_index` (SSM) or restructures the schedule changes THIS encoding under a
/// new `compute_spec_version` — a spec release + rollout — with zero change to any consensus rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwOperationIdV1 {
    pub layer: u16,
    /// 0 = prefill, 1 = decode.
    pub token_phase: u8,
    pub microbatch_index: u16,
    pub op_index: u32,
    pub expert_index: u16,
    pub tile_schedule_id: u16,
}

impl PalwOperationIdV1 {
    /// Canonical little-endian serialization (fixed 13 bytes), the exact preimage bytes the trace
    /// step absorbs.
    pub fn to_canonical_bytes(&self) -> [u8; 13] {
        let mut b = [0u8; 13];
        b[0..2].copy_from_slice(&self.layer.to_le_bytes());
        b[2] = self.token_phase;
        b[3..5].copy_from_slice(&self.microbatch_index.to_le_bytes());
        b[5..9].copy_from_slice(&self.op_index.to_le_bytes());
        b[9..11].copy_from_slice(&self.expert_index.to_le_bytes());
        b[11..13].copy_from_slice(&self.tile_schedule_id.to_le_bytes());
        b
    }

    /// A stable hash of the op id (for indexing / equality over the wire).
    pub fn hash(&self) -> Hash64 {
        blake2b_512_keyed(MIL_PALW_OP_ID_DOMAIN, &self.to_canonical_bytes())
    }
}

/// ADR-0039 §7.3 — the per-job execution challenge, derived from the previously-finalized DNS beacon,
/// the blinded job capability, and the runtime profile, so a provider cannot pre-grind the trace:
/// `H(prev_dns_beacon ‖ blinded_job_capability ‖ model_profile_id ‖ shape_id)`.
pub fn execution_challenge(
    prev_dns_beacon: &Hash64,
    blinded_job_capability: &Hash64,
    model_profile_id: &Hash64,
    shape_id: u16,
) -> Hash64 {
    let mut p = Vec::with_capacity(3 * HASH64_SIZE + 2);
    p.extend_from_slice(prev_dns_beacon.as_byte_slice());
    p.extend_from_slice(blinded_job_capability.as_byte_slice());
    p.extend_from_slice(model_profile_id.as_byte_slice());
    p.extend_from_slice(&shape_id.to_le_bytes());
    blake2b_512_keyed(MIL_PALW_EXEC_CHALLENGE_DOMAIN, &p)
}

/// ADR-0039 §7.3 — the trace-chain seed `t_0 = H(challenge ‖ runtime_profile_id ‖ job_set_commitment)`.
pub fn trace_chain_init(challenge: &Hash64, runtime_profile_id: &Hash64, job_set_commitment: &Hash64) -> Hash64 {
    let mut p = Vec::with_capacity(3 * HASH64_SIZE);
    p.extend_from_slice(challenge.as_byte_slice());
    p.extend_from_slice(runtime_profile_id.as_byte_slice());
    p.extend_from_slice(job_set_commitment.as_byte_slice());
    blake2b_512_keyed(MIL_PALW_TRACE_STEP_DOMAIN, &p)
}

/// ADR-0039 §7.3 — one trace-chain step: `t_(i+1) = H(t_i ‖ op_id ‖ input_commit ‖ acc_checksum ‖
/// output_commit ‖ selected_expert_ids ‖ overflow_flags)`. Folding every step yields
/// `canonical_gemm_trace_root = t_final`. `selected_expert_ids` are length-prefixed so the preimage is
/// unambiguous; `overflow_flags` records integer-accumulator saturations (a divergence breaks the
/// k=2 exact match).
pub fn trace_chain_step(
    t_prev: &Hash64,
    op_id: &PalwOperationIdV1,
    input_tensor_commitment: &Hash64,
    integer_accumulator_checksum: u64,
    output_tensor_commitment: &Hash64,
    selected_expert_ids: &[u16],
    overflow_flags: u32,
) -> Hash64 {
    let mut p = Vec::with_capacity(3 * HASH64_SIZE + 13 + 8 + 8 + selected_expert_ids.len() * 2 + 4);
    p.extend_from_slice(t_prev.as_byte_slice());
    p.extend_from_slice(&op_id.to_canonical_bytes());
    p.extend_from_slice(input_tensor_commitment.as_byte_slice());
    p.extend_from_slice(&integer_accumulator_checksum.to_le_bytes());
    p.extend_from_slice(output_tensor_commitment.as_byte_slice());
    p.extend_from_slice(&(selected_expert_ids.len() as u64).to_le_bytes());
    for e in selected_expert_ids {
        p.extend_from_slice(&e.to_le_bytes());
    }
    p.extend_from_slice(&overflow_flags.to_le_bytes());
    blake2b_512_keyed(MIL_PALW_TRACE_STEP_DOMAIN, &p)
}

// =============================================================================================
// §21 — model/runtime registration, work-unit packing counters, and the anti-LCU-gaming guards.
// Auditing metadata only: a provider cannot self-report more quantum than its registered profile.
// =============================================================================================

/// ADR-0039 §21.4 — the per-batch operation counters used to audit real-vs-padded work. Consensus
/// credit is the fixed per-tier quantum, NOT these self-reported numbers; they only bound padding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PalwOperationCountersV1 {
    pub real_prefill_tokens: u32,
    pub padded_prefill_tokens: u32,
    pub real_decode_tokens: u32,
    pub padded_decode_tokens: u32,
    pub selected_expert_ops: u64,
    pub canonical_mac_units: u128,
    pub canonical_memory_units: u128,
}

impl PalwOperationCountersV1 {
    /// The padded fraction of all tokens in basis points, `padded / (real + padded)` (0 if no tokens).
    pub fn padded_ratio_bps(&self) -> u16 {
        let real = self.real_prefill_tokens as u128 + self.real_decode_tokens as u128;
        let padded = self.padded_prefill_tokens as u128 + self.padded_decode_tokens as u128;
        let total = real + padded;
        if total == 0 {
            return 0;
        }
        ((padded * 10_000) / total) as u16
    }

    /// ADR-0039 §21.4 — reject a batch whose padding exceeds `max_padded_bps` (a leaf mostly of dummy
    /// padding earns no certificate). `max_padded_bps` is a network param.
    pub fn padding_within_limit(&self, max_padded_bps: u16) -> bool {
        self.padded_ratio_bps() <= max_padded_bps
    }
}

/// ADR-0039 §21.3 — the per-epoch, per-shape leaf ceiling: a shape's certified leaf count must stay at
/// or below its quota so a provider cannot optimize only for the cheapest shape (empty prompt / short
/// decode). Returns true iff `certified_leaves <= quota`.
#[inline]
pub fn shape_quota_ok(certified_leaves: u32, quota: u32) -> bool {
    certified_leaves <= quota
}

// =============================================================================================
// §8.3 — anonymous k=2 delivery: the blinded job capability. The public value is a random capability
// nullifier only, NOT tied to the requester address; full issuance/payment unlinkability needs a
// separate shielded settlement (§8.3 caveat), so this is the on-wire blinding, not the full privacy.
// =============================================================================================

/// ADR-0039 §8.3 — the blinded job capability delivered to both providers: a domain-separated
/// commitment over a random `capability_nullifier` and the fixed job `shape_id`, carrying NO link to
/// the requester. Both providers of a k=2 pair receive the SAME capability so their traces bind to the
/// same job; the capability feeds the §7.3 execution challenge as `blinded_job_capability`.
pub fn blinded_job_capability(capability_nullifier: &Hash64, shape_id: u16) -> Hash64 {
    let mut p = Vec::with_capacity(HASH64_SIZE + 2);
    p.extend_from_slice(capability_nullifier.as_byte_slice());
    p.extend_from_slice(&shape_id.to_le_bytes());
    blake2b_512_keyed(MIL_PALW_JOB_CAPABILITY_DOMAIN, &p)
}

// =============================================================================================
// Tests.
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::MIL_PROTOCOL_VERSION;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn profile(tier: PalwTier, arch: u32) -> PalwRuntimeProfileV1 {
        PalwRuntimeProfileV1 {
            version: MIL_PROTOCOL_VERSION,
            tier,
            model_id: tier.model_id(),
            tokenizer_hash: h(1),
            quantization_manifest_hash: h(2),
            runtime_image_hash: h(3),
            kernel_graph_hash: h(4),
            operation_table_hash: h(5),
            shape_table_hash: h(6),
            gpu_arch_class: arch,
            tensor_parallel_degree: 1,
            pipeline_parallel_degree: 1,
            deterministic_reduction: true,
            batch_invariant: true,
            speculative_decode: false,
            sampling: PalwSamplingParams::greedy(),
        }
    }

    #[test]
    fn tiers_are_distinct_and_pinned() {
        assert_eq!(PalwTier::Standard.project_name(), b"MISAKA-QW4-PALW-v1");
        assert_eq!(PalwTier::Quality.project_name(), b"MISAKA-QW35A-PALW-v1");
        assert_eq!(PalwTier::Quality.source_model(), "huihui_ai/Qwen3.6-abliterated:35b-Claude-4.7");
        assert_eq!(PalwTier::Standard.ram_floor_gib(), 8);
        assert_eq!(PalwTier::Quality.ram_floor_gib(), 24);
        assert_ne!(PalwTier::Standard.model_id(), PalwTier::Quality.model_id());
    }

    #[test]
    fn cross_tier_never_shares_profile_or_class() {
        let std_p = profile(PalwTier::Standard, 100);
        let qual_p = profile(PalwTier::Quality, 100);
        // different tier ⇒ different model_id ⇒ different model_profile_id ⇒ different runtime_class_id.
        assert_ne!(std_p.model_profile_id(), qual_p.model_profile_id());
        assert_ne!(std_p.runtime_class_id(), qual_p.runtime_class_id());
    }

    #[test]
    fn arch_class_separates_runtime_class_but_not_model_profile() {
        let a = profile(PalwTier::Standard, 100); // e.g. a GPU arch
        let b = profile(PalwTier::Standard, 200); // e.g. a different arch / CPU
        // same weights ⇒ same model profile ...
        assert_eq!(a.model_profile_id(), b.model_profile_id());
        // ... but a different arch class is a DISTINCT runtime class (I-9: no cross-class exact match).
        assert_ne!(a.runtime_class_id(), b.runtime_class_id());
    }

    #[test]
    fn determinism_gate() {
        let mut p = profile(PalwTier::Quality, 1);
        assert!(p.is_deterministic_admissible());
        p.speculative_decode = true;
        assert!(!p.is_deterministic_admissible());
        let mut p2 = profile(PalwTier::Quality, 1);
        p2.sampling = PalwSamplingParams { greedy: false, temperature_milli: 700 };
        assert!(!p2.is_deterministic_admissible());
        let mut p3 = profile(PalwTier::Quality, 1);
        p3.batch_invariant = false;
        assert!(!p3.is_deterministic_admissible());
    }

    #[test]
    fn commitment_helpers_are_domain_separated_and_salted() {
        // distinct domains → distinct digests for the same bytes.
        let x = b"same-bytes";
        assert_ne!(shape_commitment(x), job_set_commitment(x));
        assert_ne!(gemm_trace_root(x), operation_schedule_commitment(x));
        assert_ne!(shape_commitment(x), gemm_trace_root(x));

        // output_commitment is salted: same tokens, different salt ⇒ different commitment.
        let toks = [1u32, 2, 3, 4];
        assert_ne!(output_commitment(&[0u8; 32], &toks), output_commitment(&[1u8; 32], &toks));
        assert_eq!(output_commitment(&[7u8; 32], &toks), output_commitment(&[7u8; 32], &toks));
        // sensitive to the token stream.
        assert_ne!(output_commitment(&[7u8; 32], &toks), output_commitment(&[7u8; 32], &[1, 2, 3, 5]));
    }

    #[test]
    fn exact_match_key_is_all_or_nothing() {
        let p = profile(PalwTier::Standard, 100);
        let base = ReplicaMatchKey {
            job_set_commitment: job_set_commitment(b"js"),
            model_profile_id: p.model_profile_id(),
            runtime_class_id: p.runtime_class_id(),
            shape_id: 3,
            output_commitment: output_commitment(&[9u8; 32], &[1, 2, 3]),
            canonical_gemm_trace_root: gemm_trace_root(b"trace"),
            operation_schedule_commitment: operation_schedule_commitment(b"sched"),
            quantum_count: 2,
        };
        let same = base;
        assert!(base.exact_match(&same));
        // any single-field disagreement fails the match.
        let mut diff_trace = base;
        diff_trace.canonical_gemm_trace_root = gemm_trace_root(b"trace2");
        assert!(!base.exact_match(&diff_trace));
        let mut diff_shape = base;
        diff_shape.shape_id = 4;
        assert!(!base.exact_match(&diff_shape));
    }

    #[test]
    fn diverse_replica_match_is_cross_vendor_token_granularity() {
        // §19.8: a k=2 pair of the SAME model on DIFFERENT hardware classes (Apple vs NVIDIA) — same tier
        // ⇒ same model_profile_id, but distinct gpu_arch_class ⇒ distinct runtime_class_id.
        let apple = profile(PalwTier::Standard, 100);
        let nvidia = profile(PalwTier::Standard, 200);
        assert_ne!(apple.runtime_class_id(), nvidia.runtime_class_id(), "different hardware ⇒ different class");
        // Both agree on the TOKEN output + schedule; their RAW compute traces differ (cross-vendor ≤1 ULP).
        let k_apple = ReplicaMatchKey {
            job_set_commitment: job_set_commitment(b"js"),
            model_profile_id: apple.model_profile_id(),
            runtime_class_id: apple.runtime_class_id(),
            shape_id: 3,
            output_commitment: output_commitment(&[9u8; 32], &[1, 2, 3]),
            canonical_gemm_trace_root: gemm_trace_root(b"apple-trace"),
            operation_schedule_commitment: operation_schedule_commitment(b"sched"),
            quantum_count: 2,
        };
        // The NVIDIA leaf differs in class AND in BOTH class-dependent fields (raw trace + op schedule);
        // only the token output_commitment is shared.
        let k_nvidia = ReplicaMatchKey {
            runtime_class_id: nvidia.runtime_class_id(),
            canonical_gemm_trace_root: gemm_trace_root(b"nvidia-trace"),
            operation_schedule_commitment: operation_schedule_commitment(b"nvidia-sched"),
            ..k_apple
        };
        // Within-class exact-match FAILS (different class + trace + schedule) — cross-vendor is a distinct class.
        assert!(!k_apple.exact_match(&k_nvidia));
        // Cross-vendor diverse-replica match SUCCEEDS at token granularity (raw trace + op schedule differ).
        assert!(k_apple.diverse_replica_match(&k_nvidia));
        // Diversity is REQUIRED: two SAME-class replicas do NOT diverse-match (even with equal tokens).
        let k_apple2 = ReplicaMatchKey { canonical_gemm_trace_root: gemm_trace_root(b"apple2"), ..k_apple };
        assert!(!k_apple.diverse_replica_match(&k_apple2), "same runtime_class_id is not a diverse pair");
        // Token divergence still fails, cross-vendor or not.
        let k_nvidia_wrong = ReplicaMatchKey { output_commitment: output_commitment(&[9u8; 32], &[1, 2, 9]), ..k_nvidia };
        assert!(!k_apple.diverse_replica_match(&k_nvidia_wrong), "different token output_commitment ⇒ no match");
    }

    #[test]
    fn runtime_profile_borsh_roundtrip() {
        let p = profile(PalwTier::Quality, 42);
        let back = PalwRuntimeProfileV1::try_from_slice(&borsh::to_vec(&p).unwrap()).unwrap();
        assert_eq!(p, back);
    }

    /// §7.2/§7.3: the op id serializes canonically (13 bytes), the execution challenge binds the
    /// beacon/profile, and the trace chain is order-sensitive and deterministic.
    #[test]
    fn palw_operation_id_and_trace_chain() {
        let op =
            PalwOperationIdV1 { layer: 5, token_phase: 1, microbatch_index: 2, op_index: 9, expert_index: 3, tile_schedule_id: 7 };
        assert_eq!(op.to_canonical_bytes().len(), 13);
        // a single field change perturbs the canonical bytes and the hash.
        let mut op2 = op;
        op2.expert_index = 4;
        assert_ne!(op.to_canonical_bytes(), op2.to_canonical_bytes());
        assert_ne!(op.hash(), op2.hash());

        // the challenge depends on the beacon (no pre-grinding across epochs).
        let c1 = execution_challenge(&h(1), &h(2), &h(3), 6);
        let c2 = execution_challenge(&h(9), &h(2), &h(3), 6);
        assert_ne!(c1, c2);

        // trace chain: deterministic and order-sensitive.
        let t0 = trace_chain_init(&c1, &h(4), &job_set_commitment(b"js"));
        let a = trace_chain_step(&t0, &op, &h(5), 111, &h(6), &[1, 2], 0);
        let a_again = trace_chain_step(&t0, &op, &h(5), 111, &h(6), &[1, 2], 0);
        assert_eq!(a, a_again, "deterministic");
        let b = trace_chain_step(&t0, &op2, &h(5), 111, &h(6), &[1, 2], 0);
        assert_ne!(a, b, "op id enters the trace");
        // folding two steps in a different order gives a different root.
        let ab = trace_chain_step(&a, &op2, &h(7), 222, &h(8), &[], 1);
        let ba = trace_chain_step(&b, &op, &h(7), 222, &h(8), &[], 1);
        assert_ne!(ab, ba);
    }

    /// §8.3: the blinded job capability is deterministic, binds the shape, and hides the requester
    /// (its only public input is a random capability nullifier + the shape).
    #[test]
    fn palw_blinded_job_capability() {
        let cap = blinded_job_capability(&h(7), 3);
        assert_eq!(cap, blinded_job_capability(&h(7), 3), "deterministic ⇒ both k=2 providers agree");
        assert_ne!(cap, blinded_job_capability(&h(8), 3), "different nullifier");
        assert_ne!(cap, blinded_job_capability(&h(7), 4), "binds the shape");
    }

    /// §21.4/§21.3: padded-ratio + shape-quota guards.
    #[test]
    fn palw_registration_lcu_guards() {
        let c = PalwOperationCountersV1 {
            real_prefill_tokens: 700,
            padded_prefill_tokens: 200,
            real_decode_tokens: 100,
            padded_decode_tokens: 0,
            ..Default::default()
        };
        // padded = 200 / 1000 = 2000 bps.
        assert_eq!(c.padded_ratio_bps(), 2000);
        assert!(c.padding_within_limit(2500));
        assert!(!c.padding_within_limit(1500));
        assert_eq!(PalwOperationCountersV1::default().padded_ratio_bps(), 0); // no tokens
        // shape quota.
        assert!(shape_quota_ok(10, 10));
        assert!(!shape_quota_ok(11, 10));
    }
}
