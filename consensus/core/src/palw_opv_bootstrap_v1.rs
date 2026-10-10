//! **The OPV ↔ PALW Work Beacon startup cycle, closed** (`docs/design/palw/opv-beacon-bootstrap.md`) — dormant behind
//! `palw_probabilistic_constraints_v1` and `palw_panel_free_v1` (refused at every real height); no `Params` field, no tag, no table.
//!
//! ```text
//! sampled conformance ⇐ beacon ⇐ k Finals of OPV-ELIGIBLE classes ⇐ eligibility ⇐ conformance          (from genesis: ∅ forever)
//! complete check (this module): every input and every weight checked IN THE FOLD — no seed, no beacon  (the base case)
//! ```
//!
//! Four parts, each the single implementation the fold, the readers and the tests share:
//!
//! 1. **The complete check** — which classes qualify ([`palw_complete_check_domain_v1`]: stateless, an enumerable input domain, an
//!    artifact small enough to carry whole, a bounded forward work), what the registrant posts ([`CompleteCheckPostV1`]: the whole
//!    inventory and the result roots, sized to ONE carrier), the reference it is judged against ([`complete_check_reference_v1`]) and
//!    the judgement ([`judge_complete_check_v1`]: the leaves re-rooted to the V2 artifact root, the kernel commitments recomputed —
//!    binding equality PROVEN — and every input run by the chain's own interpreter).
//! 2. **Derived OPV eligibility** ([`PalwKernelRouteStateV1::opv_eligibility_v1`], E1–E7) and the eligible set a conformance
//!    commitment freezes as its beacon's sources ([`PalwKernelRouteStateV1::opv_eligible_set_v1`]).
//! 3. **The effective bits** a passed attempt delivers ([`attempt_effective_bits_v1`]): `Complete` for a complete check; for a sampled
//!    one, the scope's families under the policy's repetitions and retries and the stated grinding budget.
//! 4. **The dependency graph** as data ([`PALW_OPV_DEPENDENCIES_V1`]) and its least fixed point from genesis ([`reachable_v1`]).
//!
//! Beside them: the G14-for-rewards gate ([`palw_reward_gate_v1`]) and the ADR-0176 hook every reward channel names its per-bond budget
//! through ([`PalwRewardChannelV1`], [`PalwBondBudgetHookV1`]). **No condition here asks whether anyone can acquire a model**
//! (ADR-0177; design §14.1): a binding stands or falls by proofs, never by its bytes being served.

use std::collections::{BTreeMap, BTreeSet};

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::hash::Digest;
use misaka_palw_challenge::soundness::{
    EffectiveBitsV1, EffectiveSoundnessInputV1, RelationSoundnessV1, effective_false_accept_bits_v1, sampled_family_millibits_v1,
};
use misaka_palw_challenge::{OnboardingStateV1, PostCommitChallengePolicyV1};
use misaka_palw_kernel::VerificationPlanV1;
use misaka_palw_kernel::descriptor::KernelStandingV1;
use misaka_palw_kernel::ledger::{KernelLedgerV1, carrier_fit_v1, single_class_binding_v1};
use misaka_palw_kernel::mode::{VerificationModeV1, class_id_for_mode_v1};
use misaka_palw_kernel::opv::OpvPolicyV1;
use misaka_palw_kernel::public::program_root_v1;
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::program::{INPUT_POS, Ref};
use misaka_palw_tir::{MapParams, Tensor, TirProgramV1};

use crate::Hash64;
use crate::constants::SOMPI_PER_KASPA;
use crate::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1};
use crate::palw_conformance_evidence_v1::{reference_leaf_result_v1, tool_root, values_digest_v1};
use crate::palw_kernel_route_v1::PalwKernelRouteStateV1;
use crate::palw_onboarding_v1::{
    ArtifactBindingStateV1, ConformanceAttemptRowV1, KernelBindingRowV1, PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1,
};
use crate::palw_tir_artifact_v1::{
    palw_tir_inventory_leaf_count_v1, palw_tir_param_instances_v1, palw_tir_tensor_bytes_v1, palw_tir_visit_inventory_rows_v1,
};

// =================================================================================================================================
// I. The complete check
// =================================================================================================================================

/// The most inputs (`token_bound × positions`) a complete check enumerates.
pub const PALW_COMPLETE_CHECK_MAX_INPUTS_V1: u64 = 1_024;
/// The most inventory leaves a complete check carries.
pub const PALW_COMPLETE_CHECK_MAX_LEAVES_V1: u32 = 1_024;
/// The most artifact bytes a complete check carries (the post must ride ONE carrier — no chunk lane, so no lane can be captured).
pub const PALW_COMPLETE_CHECK_MAX_ARTIFACT_BYTES_V1: u64 = 64 * 1024;
/// The most bytes a [`CompleteCheckPostV1`] may encode to (one carrier, with room for the wrapper and the signature).
pub const PALW_COMPLETE_CHECK_MAX_POST_BYTES_V1: usize = 90_000;
/// The most fold work (§8 node costs, plus one unit per artifact byte hashed) one complete check may cost. Charged to the block's
/// adjudication budget as court work BEFORE the post is read.
pub const PALW_COMPLETE_CHECK_MAX_WORK_V1: u64 = 1 << 26;
/// The most complete checks one chain block judges (the rest wait for the next block, uncharged). With the work cap this keeps at
/// most `2 × 2^26 = 2^27` of the block's `2^30` court work for complete checks, so prosecutions always have room.
pub const PALW_COMPLETE_CHECKS_PER_BLOCK_V1: u32 = 2;
/// The non-refundable fee a judged complete check burns from the registrant's bond (INTERIM): with the carrier fee and the counted
/// attempt (≤ `retry_limit + 1` per class, each behind a class registration and a binding reservation) it pays for the CPU.
pub const PALW_COMPLETE_CHECK_FEE_SOMPI_V1: u64 = SOMPI_PER_KASPA;
/// A complete-check commitment with no judged post by `committed + this` is a default (withheld, counted). INTERIM.
pub const PALW_COMPLETE_CHECK_DEADLINE_DAA_V1: u64 = 60;

pub const DOMAIN_COMPLETE_INPUTS_V1: &[u8] = b"misaka.palw.complete-check.input-results.v1";
pub const DOMAIN_COMPLETE_LEAVES_V1: &[u8] = b"misaka.palw.complete-check.leaf-results.v1";
pub const DOMAIN_COMPLETE_POST_V1: &[u8] = b"misaka.palw.complete-check.post.v1";

/// **The network's complete-check onboarding policy** (no randomness: `k`, the delay, the window and `D` are zero), the other of the
/// network's two onboarding policies. Its target is the effective floor (a complete check meets every floor) and its retry limit the
/// sampled policy's (three counted attempts).
pub fn palw_onboarding_complete_check_policy_v1() -> PostCommitChallengePolicyV1 {
    misaka_palw_challenge::complete_check_policy_v1(
        crate::palw_panel_free_v1::PALW_OPV_MIN_EFFECTIVE_BITS_V1,
        crate::palw_conformance_evidence_v1::palw_onboarding_challenge_policy_v1().retry_limit,
    )
}

/// **What a complete check of a class enumerates and costs.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompleteCheckDomainV1 {
    pub token_bound: u32,
    /// 1 when no node reads the position; the plan's `max_positions` otherwise.
    pub positions: u32,
    /// `token_bound × positions`.
    pub inputs: u64,
    pub leaves: u32,
    pub artifact_bytes: u64,
    /// The fold's work: `inputs × per-position forward work + artifact bytes`.
    pub work: u64,
}

/// **Whether a class's whole behaviour can be checked completely, and what that costs** — a pure function of the program the chain
/// holds and the plan's position bound (no list, no registrant flag). A program with state refers to its prefix, so its inputs are
/// `Σ T^l` and it never qualifies; a stateless one's output at a position is a function of `(token, position)` — of the token alone
/// when no node reads the position — so every job's whole output is determined by the enumerated inputs.
pub fn palw_complete_check_domain_v1(program: &TirProgramV1, max_positions: u32) -> Result<CompleteCheckDomainV1, String> {
    if !program.states.is_empty()
        || program.blocks.iter().flat_map(|b| &b.nodes).any(|n| n.inputs.iter().any(|r| matches!(r, Ref::State(_))))
    {
        return Err("the program carries state: its output depends on the whole prefix, so its inputs are never enumerable".into());
    }
    let reads_pos =
        program.blocks.iter().flat_map(|b| &b.nodes).any(|n| n.inputs.iter().any(|r| matches!(r, Ref::Input(i) if *i == INPUT_POS)));
    let positions = if reads_pos { max_positions.max(1) } else { 1 };
    let inputs = program.token_bound as u64 * positions as u64;
    if inputs == 0 || inputs > PALW_COMPLETE_CHECK_MAX_INPUTS_V1 {
        return Err(format!(
            "{inputs} inputs (token bound × positions), past the complete check's {PALW_COMPLETE_CHECK_MAX_INPUTS_V1}"
        ));
    }
    let leaves = palw_tir_inventory_leaf_count_v1(program).map_err(|e| e.to_string())?;
    if leaves > PALW_COMPLETE_CHECK_MAX_LEAVES_V1 {
        return Err(format!("{leaves} artifact leaves, past the complete check's {PALW_COMPLETE_CHECK_MAX_LEAVES_V1}"));
    }
    let instances = palw_tir_param_instances_v1(program);
    let artifact_bytes: u64 = instances
        .iter()
        .enumerate()
        .map(|(j, inst)| palw_tir_tensor_bytes_v1(program, j as u16).saturating_mul(inst.len() as u64))
        .fold(0u64, u64::saturating_add);
    if artifact_bytes > PALW_COMPLETE_CHECK_MAX_ARTIFACT_BYTES_V1 {
        return Err(format!("{artifact_bytes} artifact bytes, past the complete check's {PALW_COMPLETE_CHECK_MAX_ARTIFACT_BYTES_V1}"));
    }
    let mut per_position: u64 = 0;
    for (block, _layer) in program.occurrences() {
        let b = block as usize;
        for n in 0..program.blocks.get(b).map(|blk| blk.nodes.len()).unwrap_or(0) {
            let c = misaka_palw_tir::admit::node_cost(program, b, n, 1);
            per_position = per_position.saturating_add(c.macs).saturating_add(c.elementwise).saturating_add(c.transcendentals);
        }
    }
    let work = inputs.saturating_mul(per_position.max(1)).saturating_add(artifact_bytes);
    if work > PALW_COMPLETE_CHECK_MAX_WORK_V1 {
        return Err(format!("a complete check costs {work} work units, past the fold's {PALW_COMPLETE_CHECK_MAX_WORK_V1}"));
    }
    Ok(CompleteCheckDomainV1 { token_bound: program.token_bound, positions, inputs, leaves, artifact_bytes, work })
}

/// One implementation role's result roots: over every input (`(token, position, logits digest, greedy next)`, canonical order)
/// and over every leaf (its decoded-values digest, inventory order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompleteRootsV1 {
    pub reference: Digest,
    pub independent: Digest,
    pub backend: Digest,
}

/// **Tag 109 `PostComplete`**: the whole inventory and the three implementations' result roots of an attempt committed under the
/// complete-check policy.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompleteCheckPostV1 {
    pub version: u16,
    /// The attempt's statement root: a post is about exactly one attempt.
    pub commitment_root: Digest,
    /// Every leaf of the V2 inventory, in inventory order.
    pub operands: Vec<PalwArtifactOperandV1>,
    pub inputs: CompleteRootsV1,
    pub leaves: CompleteRootsV1,
}

impl CompleteCheckPostV1 {
    pub fn id(&self) -> Digest {
        tool_root(DOMAIN_COMPLETE_POST_V1, self)
    }
}

/// What the chain's own reference computes for a complete check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompleteReferenceV1 {
    /// The root over the carried leaves (to equal the class's registered `artifact_root`).
    pub artifact_root: Hash64,
    /// `ParamCommitmentsV1::root` of the tensors the leaves assemble to (to equal the binding's kernel root).
    pub kernel_param_root: Digest,
    pub input_root: Digest,
    pub leaf_root: Digest,
}

/// A complete check that is about this attempt and is not a pass: the code names the first thing that failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompleteFailV1 {
    pub code: &'static str,
    pub detail: String,
}

fn fail(code: &'static str, detail: impl Into<String>) -> CompleteFailV1 {
    CompleteFailV1 { code, detail: detail.into() }
}

/// **The reference of a complete check**: the carried leaves at exactly the coordinates the program's inventory puts them (count,
/// tensor, layer, offset, length — cheap, first), their root, the tensors they assemble to and those tensors' kernel commitments, every
/// leaf's reference decoding, and every input run by the chain's own interpreter. Every path is a `Result` on any bytes.
pub fn complete_check_reference_v1(
    program: &TirProgramV1,
    domain: &CompleteCheckDomainV1,
    operands: &[PalwArtifactOperandV1],
) -> Result<CompleteReferenceV1, CompleteFailV1> {
    // ---- coordinates (O(leaves)) ----
    let mut rows = Vec::with_capacity(domain.leaves as usize);
    palw_tir_visit_inventory_rows_v1(program, &mut |row| rows.push(row)).map_err(|e| fail("INVENTORY", e.to_string()))?;
    if operands.len() != rows.len() {
        return Err(fail("INVENTORY_SHAPE", format!("{} leaves carried, the inventory has {}", operands.len(), rows.len())));
    }
    for (i, (op, row)) in operands.iter().zip(&rows).enumerate() {
        let name = program.params.get(row.param as usize).map(|p| p.name.as_str()).unwrap_or("");
        if op.tensor_name != name || op.layer != row.layer || op.row_start != row.row_start || op.bytes.len() != row.len as usize {
            return Err(fail("INVENTORY_SHAPE", format!("leaf {i} is not at the inventory's coordinates")));
        }
    }
    let leaves: Vec<Hash64> = operands.iter().map(artifact_leaf_v1).collect();
    let artifact_root = artifact_root_v1(&leaves).ok_or_else(|| fail("INVENTORY_SHAPE", "an empty inventory"))?;
    // ---- tensors ----
    let mut bytes_of: BTreeMap<(u16, Option<u16>), Vec<u8>> = BTreeMap::new();
    for (op, row) in operands.iter().zip(&rows) {
        bytes_of.entry((row.param, row.layer)).or_default().extend_from_slice(&op.bytes);
    }
    let mut params = MapParams::default();
    for ((j, layer), bytes) in bytes_of {
        let decl = &program.params[j as usize];
        let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
        let t = Tensor::from_le_bytes(decl.dtype, &shape, &bytes).map_err(|e| fail("TENSOR", e.to_string()))?;
        params.tensors.insert((j, layer), t);
    }
    let kernel_param_root = ParamCommitmentsV1::of(&params).root();
    // ---- leaves ----
    let mut leaf_results = Vec::with_capacity(operands.len());
    for (op, row) in operands.iter().zip(&rows) {
        leaf_results.push(reference_leaf_result_v1(program, row.param, &op.bytes).map_err(|e| fail("LEAF_DECODE", e))?);
    }
    // ---- inputs ----
    let post_occurrence = program.occurrences().len().saturating_sub(1);
    let mut input_results: Vec<(u32, u32, [u8; 32], u32)> = Vec::with_capacity(domain.inputs as usize);
    for t in 0..domain.token_bound {
        let stream = vec![t; domain.positions as usize];
        let trace = misaka_palw_kernel::trace::trace_v1(program, &params, &stream).map_err(|e| fail("FORWARD", e.to_string()))?;
        for p in 0..domain.positions {
            let logits = trace
                .values
                .get(p as usize)
                .and_then(|occ| occ.get(post_occurrence))
                .and_then(|nodes| nodes.get(program.logits as usize))
                .ok_or_else(|| fail("FORWARD", "the trace has no logits at a position"))?;
            let next = misaka_palw_kernel::job::DecodeRuleV1::Greedy.select(logits).ok_or_else(|| fail("FORWARD", "empty logits"))?;
            input_results.push((t, p, values_digest_v1(&logits.data), next));
        }
    }
    Ok(CompleteReferenceV1 {
        artifact_root,
        kernel_param_root,
        input_root: tool_root(DOMAIN_COMPLETE_INPUTS_V1, &input_results),
        leaf_root: tool_root(DOMAIN_COMPLETE_LEAVES_V1, &leaf_results),
    })
}

/// The verdict of a complete check bound to its attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompleteVerdictV1 {
    Pass,
    Fail(CompleteFailV1),
}

/// **Judge a complete check** (after its binding to the attempt was checked and its work charged): the leaves must root to the V2
/// class's registered artifact root (the whole artifact is public, on chain), assemble to the tensors the kernel binding committed
/// to (binding equality proven), and every implementation's input and leaf roots must equal the chain's own.
pub fn judge_complete_check_v1(
    program: &TirProgramV1,
    domain: &CompleteCheckDomainV1,
    artifact_root: Hash64,
    kernel_param_root: Hash64,
    post: &CompleteCheckPostV1,
) -> CompleteVerdictV1 {
    let r = match complete_check_reference_v1(program, domain, &post.operands) {
        Ok(r) => r,
        Err(f) => return CompleteVerdictV1::Fail(f),
    };
    if r.artifact_root != artifact_root {
        return CompleteVerdictV1::Fail(fail("ARTIFACT_ROOT", "the leaves do not root to the class's registered artifact root"));
    }
    if r.kernel_param_root != kernel_param_root.as_bytes() {
        return CompleteVerdictV1::Fail(fail(
            "BINDING_NOT_EQUAL",
            "the artifact's tensors are not the ones the kernel binding committed",
        ));
    }
    for (what, roots, want) in [("input", &post.inputs, r.input_root), ("leaf", &post.leaves, r.leaf_root)] {
        for (role, got) in [("reference", roots.reference), ("independent", roots.independent), ("backend", roots.backend)] {
            if got != want {
                return CompleteVerdictV1::Fail(fail("RESULTS_DIFFER", format!("the {role} implementation's {what} results differ")));
            }
        }
    }
    CompleteVerdictV1::Pass
}

/// What an honest registrant posts when every implementation agrees with the reference (tests and the SDK's builder).
pub fn complete_check_post_v1(
    program: &TirProgramV1,
    domain: &CompleteCheckDomainV1,
    commitment_root: Digest,
    operands: Vec<PalwArtifactOperandV1>,
) -> Result<CompleteCheckPostV1, CompleteFailV1> {
    let r = complete_check_reference_v1(program, domain, &operands)?;
    let all = |d: Digest| CompleteRootsV1 { reference: d, independent: d, backend: d };
    Ok(CompleteCheckPostV1 { version: 1, commitment_root, operands, inputs: all(r.input_root), leaves: all(r.leaf_root) })
}

// =================================================================================================================================
// II. Effective bits of a passed attempt
// =================================================================================================================================

/// **The grinding choices per beacon the interim sampled policy is accounted with: unbounded (`2^128`).** Under the current source
/// rule a LAST contributor — anyone who can post one job on an eligible class and produce its claim — grinds the job's free nonce
/// offline before committing, so its choices are its offline work, not a count the chain caps (`docs/design/palw/opv-beacon-bootstrap.md`
/// §6.2). Until a sealed-source beacon bounds it, a sampled scope must out-bit the adversary's whole hash budget.
pub fn palw_onboarding_grinding_choices_v1() -> u128 {
    u128::MAX
}

/// **The effective bits a passed attempt delivers**: `Complete` under the complete-check policy; under the sampled policy, the
/// committed scope's families (per repetition) under the policy's repetitions and retries, the stated grinding budget, one beacon and
/// one statement. No posted material (it cannot pass without it) is 0 bits.
pub fn attempt_effective_bits_v1(
    route: &PalwKernelRouteStateV1,
    v2_class: &Hash64,
    attempt: &ConformanceAttemptRowV1,
) -> EffectiveBitsV1 {
    if attempt.is_complete_check() {
        return EffectiveBitsV1::Complete;
    }
    let Some(post) = route.conformance_evidence_post_v1(v2_class) else { return EffectiveBitsV1::Bits(0) };
    let policy = attempt.policy();
    let mut relations = Vec::new();
    if post.scope.vectors_per_repetition > 0 {
        relations.push(RelationSoundnessV1::MilliBits(sampled_family_millibits_v1(
            post.scope.vectors_per_repetition as u64,
            post.scope.vector_fault_ppm,
        )));
    }
    if post.scope.leaves_per_repetition > 0 {
        relations.push(RelationSoundnessV1::MilliBits(sampled_family_millibits_v1(
            post.scope.leaves_per_repetition as u64,
            post.scope.leaf_fault_ppm,
        )));
    }
    // v3 (sealed sources): the adversary's only post-reveal move is a counted veto, so G = F = 1 — and the beacon's own failure,
    // `ε_src` (every honest seal censored out of the seal window), is a second term (§6.3). v2: the last contributor, G = 2^128.
    let sealed = attempt.is_sealed_source();
    let grinding =
        if sealed { misaka_palw_challenge::sealed_beacon_grinding_choices_v3(1) } else { palw_onboarding_grinding_choices_v1() };
    let algorithmic = effective_false_accept_bits_v1(&EffectiveSoundnessInputV1 {
        relations,
        repetition_count: policy.repetition_count,
        retry_limit: policy.retry_limit,
        grinding_choices_per_beacon: grinding,
        beacons_per_attempt: 1,
        adaptive_queries: 1,
    })
    .unwrap_or(EffectiveBitsV1::Bits(0));
    if !sealed {
        return algorithmic;
    }
    use crate::palw_conformance_evidence_v1::{
        PALW_ONBOARDING_SEALED_ADVERSARY_NEG_LOG2_MILLIBITS_V1 as RHO, PALW_ONBOARDING_SEALED_MERGE_DELAY_DAA_V1 as DELTA,
        PALW_ONBOARDING_SEALED_PARTICIPATION_BITS_V1 as PARTICIPATION,
    };
    use misaka_palw_challenge::combine_failure_bits_v1 as union;
    // ε_src = censorship of every honest seal ∪ no honest PARTY sealing at all (bonds are not parties, F-C4R4-01): both charged.
    let censorship = misaka_palw_challenge::sealed_source_censorship_bits_v3(policy.beacon_window_slots, DELTA, 1, RHO);
    let src = union(EffectiveBitsV1::Bits(censorship.min(u16::MAX as u64) as u16), EffectiveBitsV1::Bits(PARTICIPATION));
    union(algorithmic, src)
}

// =================================================================================================================================
// III. Derived OPV eligibility
// =================================================================================================================================

/// The network-side view the predicate reads (from `Params::palw_panel_free_v1` through the fold's extras).
#[derive(Clone, Copy, Debug)]
pub struct OpvEligibilityViewV1<'a> {
    pub policy: &'a OpvPolicyV1,
    pub denied: &'a [Hash64],
    pub min_effective_bits: u16,
    /// GAP-70's switch (the Lead, 2026-10-10): whether a SAMPLED conformance may satisfy E6. `false` on every network
    /// (`PalwPanelFreeFenceV1::validate_value` refuses `true` until a digest court exists); a drill sets it explicitly.
    pub sampled_gates_reward: bool,
    /// TEST SEAM (empty outside `cfg(test)` of the processor): ids a pre-derivation mechanics test treats as eligible.
    pub test_eligible: &'a [Hash64],
}

impl<'a> OpvEligibilityViewV1<'a> {
    pub fn of(extras: &'a crate::palw_kernel_route_v1::PalwKernelOpvExtrasV1) -> Self {
        Self {
            policy: &extras.policy,
            denied: &extras.denied_classes,
            min_effective_bits: extras.min_effective_bits,
            sampled_gates_reward: extras.sampled_conformance_gates_reward,
            test_eligible: &extras.test_eligible,
        }
    }
}

/// What the predicate knows of an OPV class: its mode-bound id, its legacy (Panel-licensed) sibling's id, and the roots both share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpvClassFactsV1 {
    pub opv_id: Digest,
    pub legacy_id: Digest,
    pub descriptor: Digest,
    pub program_root: Digest,
    pub plan_root: Digest,
    pub param_root: Digest,
}

impl OpvClassFactsV1 {
    /// From the fields of a registration (tag 13) — the class need not exist yet.
    pub fn of_registration(descriptor: Digest, program_bytes: &[u8], plan: &VerificationPlanV1, pc: &ParamCommitmentsV1) -> Self {
        let legacy_id = single_class_binding_v1(descriptor, program_bytes, plan, pc).class_binding_id();
        Self {
            opv_id: class_id_for_mode_v1(&legacy_id, VerificationModeV1::OptimisticPublicVerification),
            legacy_id,
            descriptor,
            program_root: program_root_v1(program_bytes),
            plan_root: plan.root(),
            param_root: pc.root(),
        }
    }

    /// From a registered single-program class of either mode (`None`: not a registered single-program class).
    pub fn of_registered(ledger: &KernelLedgerV1, class: &Digest) -> Option<Self> {
        let row = ledger.classes.get(class)?;
        Some(Self::of_registration(row.descriptor.digest(), &row.program_bytes, &row.plan, &row.param_commitments))
    }
}

/// **Why a class is not OPV-eligible** — each reason is exactly one predecessor of `OpvEligible` in [`PALW_OPV_DEPENDENCIES_V1`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpvIneligibleV1 {
    /// E7: the network's deny-list names it.
    Denied,
    /// E1: its kernel descriptor is not Active.
    KernelNotActive,
    /// No V2 class is kernel-bound to it or to its legacy sibling: no conformance path exists for it.
    NotOnboarded,
    /// E2: the bound V2 class's record is not past CONFORMANCE_PASSED and the public-prosecution step.
    ConformanceNotPassed,
    /// E2: the passed statement names another program, plan, kernel or artifact (another class needs its own conformance).
    ConformanceOfAnotherStatement,
    /// E3: G14 does not fully hold — the bound kernel class does not stand in the route, its PLAN is not publicly prosecutable over
    /// its whole context now, or (a V2 class) it serves a task/context past the one G14 was proven for.
    NotG14Complete(String),
    /// E4: the artifact binding does not stand — refuted (a proof of inequality), missing, still Pending, or of other commitments. An
    /// IDENTITY fact (ADR-0175/0177): never "the bytes could not be obtained" — maturity is a clock, not evidence of availability.
    BindingNotStanding,
    /// E5: its prosecution bounds do not fit the OPV carriers, the block's court, or out-cost the claim's gain.
    ResourceUnbounded(String),
    /// E6: the conformance policy is not the network's, not valid, or its effective bits are below the floor.
    PolicyNotVerified(String),
    /// A pipeline OPV class: no onboarding path exists for pipelines (GAP-B4).
    PipelineNotOnboardable,
    /// RFC-0004 Part II: a `Retrieval` class (or a `Composite` with a retrieval stage) — no conformance statement exists for a snapshot
    /// yet (the snapshot binding of GAP-52), so none is eligible.
    SnapshotNotOnboardable,
}

impl OpvIneligibleV1 {
    /// The predecessor of `OpvEligible` this reason is the absence of.
    pub fn predecessor(&self) -> PalwOpvNodeV1 {
        use PalwOpvNodeV1 as N;
        match self {
            Self::Denied => N::NotDenied,
            Self::KernelNotActive => N::KernelActive,
            Self::NotOnboarded | Self::ConformanceNotPassed | Self::ConformanceOfAnotherStatement | Self::NotG14Complete(_) => {
                N::G14Eligible
            }
            Self::PipelineNotOnboardable | Self::SnapshotNotOnboardable => N::G14Eligible,
            Self::BindingNotStanding => N::ArtifactMatured,
            Self::ResourceUnbounded(_) => N::BoundsFit,
            Self::PolicyNotVerified(_) => N::PolicyVerified,
        }
    }

    /// One of each (for the graph test).
    pub fn all() -> Vec<Self> {
        vec![
            Self::Denied,
            Self::KernelNotActive,
            Self::NotOnboarded,
            Self::ConformanceNotPassed,
            Self::ConformanceOfAnotherStatement,
            Self::NotG14Complete(String::new()),
            Self::BindingNotStanding,
            Self::ResourceUnbounded(String::new()),
            Self::PolicyNotVerified(String::new()),
            Self::PipelineNotOnboardable,
            Self::SnapshotNotOnboardable,
        ]
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Denied => "DENIED",
            Self::KernelNotActive => "KERNEL_NOT_ACTIVE",
            Self::NotOnboarded => "NOT_ONBOARDED",
            Self::ConformanceNotPassed => "CONFORMANCE_NOT_PASSED",
            Self::ConformanceOfAnotherStatement => "CONFORMANCE_OF_ANOTHER_STATEMENT",
            Self::NotG14Complete(_) => "NOT_G14_COMPLETE",
            Self::BindingNotStanding => "BINDING_NOT_STANDING",
            Self::ResourceUnbounded(_) => "RESOURCE_UNBOUNDED",
            Self::PolicyNotVerified(_) => "POLICY_NOT_VERIFIED",
            Self::PipelineNotOnboardable => "PIPELINE_NOT_ONBOARDABLE",
            Self::SnapshotNotOnboardable => "SNAPSHOT_NOT_ONBOARDABLE",
        }
    }
}

/// Why a class IS eligible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpvEligibleV1 {
    /// Derived from chain state through the conformance of this V2 class.
    Derived { v2_class: Hash64 },
    /// TEST SEAM only (`OpvEligibilityViewV1::test_eligible`).
    TestHook,
}

impl PalwKernelRouteStateV1 {
    /// Every kernel binding (106) `(V2 class, row)`, in key order.
    pub fn kernel_bindings_v1(&self) -> Vec<(Hash64, KernelBindingRowV1)> {
        self.aux
            .range((PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1, Vec::new())..(PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1 + 1, Vec::new()))
            .filter_map(|((_, key), row)| Some((borsh::from_slice(key).ok()?, borsh::from_slice(row).ok()?)))
            .collect()
    }

    /// **Derived OPV eligibility of a single-program class at `daa`** (`docs/design/palw/opv-beacon-bootstrap.md` §5, E1–E7). `ledger`
    /// is this route's ledger at the block (the caller has it loaded).
    pub fn opv_eligibility_v1(
        &self,
        ledger: &KernelLedgerV1,
        facts: &OpvClassFactsV1,
        daa: u64,
        view: &OpvEligibilityViewV1<'_>,
    ) -> Result<OpvEligibleV1, OpvIneligibleV1> {
        let id = Hash64::from_bytes(facts.opv_id);
        if view.denied.contains(&id) {
            return Err(OpvIneligibleV1::Denied);
        }
        if view.test_eligible.contains(&id) {
            return Ok(OpvEligibleV1::TestHook);
        }
        if ledger.schedule.standing_at(&facts.descriptor, daa) != KernelStandingV1::Active {
            return Err(OpvIneligibleV1::KernelNotActive);
        }
        let candidates: Vec<(Hash64, KernelBindingRowV1)> = self
            .kernel_bindings_v1()
            .into_iter()
            .filter(|(_, b)| b.kernel_class.as_bytes() == facts.opv_id || b.kernel_class.as_bytes() == facts.legacy_id)
            .collect();
        let mut first_refusal = None;
        for (v2_class, binding) in candidates {
            match self.opv_eligibility_through_v1(ledger, facts, daa, view, &v2_class, &binding) {
                Ok(()) => return Ok(OpvEligibleV1::Derived { v2_class }),
                Err(why) => {
                    first_refusal.get_or_insert(why);
                }
            }
        }
        Err(first_refusal.unwrap_or(OpvIneligibleV1::NotOnboarded))
    }

    fn opv_eligibility_through_v1(
        &self,
        ledger: &KernelLedgerV1,
        facts: &OpvClassFactsV1,
        daa: u64,
        view: &OpvEligibilityViewV1<'_>,
        v2_class: &Hash64,
        binding: &KernelBindingRowV1,
    ) -> Result<(), OpvIneligibleV1> {
        use OpvIneligibleV1 as I;
        // E2: conformance passed, and passed about THIS program, plan, kernel and artifact.
        let attempt = self.conformance_attempt_v1(v2_class).ok_or(I::ConformanceNotPassed)?;
        if !matches!(attempt.record.state, OnboardingStateV1::G14Eligible | OnboardingStateV1::ActiveRewardable) {
            return Err(I::ConformanceNotPassed);
        }
        let c = &attempt.commitment;
        let statement_on_chain = self
            .conformance_v1(v2_class, &Hash64::from_bytes(c.artifact_root))
            .is_some_and(|row| row.statement_root.as_bytes() == c.statement_root());
        if c.program_root != facts.program_root
            || c.verification_plan_root != facts.plan_root
            || c.kernel_descriptor_id != facts.descriptor
            || binding.plan_root.as_bytes() != facts.plan_root
            || !statement_on_chain
        {
            return Err(I::ConformanceOfAnotherStatement);
        }
        // E3: the bound kernel class stands (registered only after PUBLIC_PROSECUTION_COMPLETE).
        if self.kernel_class_record_v1(&binding.kernel_class).is_none() {
            return Err(I::NotG14Complete("the bound kernel class does not stand in the route".into()));
        }
        // E3, plan and task/context (the user's ruling of 2026-10-09: OPV only for a class, plan AND task/context for which G14 fully
        // holds): PUBLIC_PROSECUTION_COMPLETE re-derived NOW for the bound class's plan — over its whole context (`plan.max_positions`:
        // every position a job of the class can touch; a job past it is refused at posting), under the public kernel-route profile
        // and the route's prosecution policy — and equal to the bounds the class registered with. The kernel route's task is its one
        // decode rule (greedy), which the plan's courts adjudicate.
        let row = ledger
            .classes
            .get(&binding.kernel_class.as_bytes())
            .or_else(|| ledger.conformance_classes.get(&binding.kernel_class.as_bytes()).map(|(_, row)| row))
            .ok_or_else(|| I::NotG14Complete("the bound class is not in the ledger".into()))?;
        // The class's own gate (K2-TIR-v4 for segmented descriptors, v1 otherwise), with the program the bounds are priced from.
        let proven = misaka_palw_kernel::gate::class_prosecution_bounds_v1(
            &row.descriptor,
            &row.plan,
            &row.program,
            &misaka_palw_kernel::public::ProfileMaterialV1::kernel_route(true),
            &ledger.policy.prosecution,
        )
        .map_err(|gaps| I::NotG14Complete(format!("the plan is not publicly prosecutable over its context: {gaps:?}")))?;
        if proven != row.bounds {
            return Err(I::NotG14Complete("the plan's prosecution bounds are not the ones the class registered with".into()));
        }
        // E4: the binding STANDS — the V2 artifact bound to exactly these commitments, past its first refutation window, unrefuted.
        // Identity only (ADR-0177): the row's clock and its refutation by proof; never whether the bytes are being served.
        if binding.kernel_param_root.as_bytes() != facts.param_root
            || !self
                .artifact_binding_v1(v2_class, &binding.kernel_param_root)
                .is_some_and(|row| matches!(row.state_at(daa), ArtifactBindingStateV1::Matured | ArtifactBindingStateV1::Final))
        {
            return Err(I::BindingNotStanding);
        }
        // E5: bounded resources and deadlines under the OPV terms (the bound class has the same program and plan, so its bounds).
        let bounds = ledger
            .classes
            .get(&binding.kernel_class.as_bytes())
            .or_else(|| ledger.conformance_classes.get(&binding.kernel_class.as_bytes()).map(|(_, row)| row))
            .map(|row| row.bounds)
            .ok_or_else(|| I::ResourceUnbounded("the bound class's bounds are not in the ledger".into()))?;
        let p = view.policy;
        carrier_fit_v1(&bounds, p.carrier.filing_cap as usize, p.carrier.response_cap as usize, p.carrier.commit_cap as usize)
            .map_err(I::ResourceUnbounded)?;
        if bounds.max_court_work > ledger.policy.max_court_work_per_block {
            return Err(I::ResourceUnbounded("its worst court does not fit one block's court budget".into()));
        }
        if p.censorship_cost(&ledger.policy, bounds.max_court_work) <= p.max_gain_per_claim(&ledger.policy) {
            return Err(I::ResourceUnbounded("its prosecution could be censored for less than a claim gains".into()));
        }
        // E6: the network's policy, valid, with effective bits at the floor.
        let policy = attempt.policy();
        if c.challenge_policy_id != policy.id() || binding.challenge_policy_id.as_bytes() != policy.id() {
            return Err(I::PolicyNotVerified("not one of the network's two onboarding policies".into()));
        }
        policy.validate().map_err(|e| I::PolicyNotVerified(e.to_string()))?;
        // GAP-70 (the Lead, 2026-10-10): for the release only the COMPLETE check gates rewards. A sampled conformance (v2 or v3) is a
        // non-reward signal until a court exists for a vector's logits and commit digests (OB-P0 GAP 2), a refutation path for a new
        // class (GAP-B6) and the self-reported implementation results (GAP-B7) are closed — whatever its effective bits.
        if !attempt.is_complete_check() && !view.sampled_gates_reward {
            return Err(I::PolicyNotVerified(
                "a sampled conformance gates no reward until a digest court exists (GAP-70): only the complete check does".into(),
            ));
        }
        let effective = attempt_effective_bits_v1(self, v2_class, &attempt);
        if !effective.meets(view.min_effective_bits) {
            return Err(I::PolicyNotVerified(format!("{effective:?} effective bits, below the floor of {}", view.min_effective_bits)));
        }
        Ok(())
    }

    /// **E1–E7 for a V2 class, through its OWN kernel binding** (G14-for-rewards): the bound kernel class's facts, the deny-list under
    /// either mode's id, the kernel Active, then [`Self::opv_eligibility_v1`]'s E2–E6 through exactly this binding (never another
    /// V2 class's conformance of the same kernel class).
    pub fn v2_class_reward_eligibility_v1(
        &self,
        ledger: &KernelLedgerV1,
        v2_class: &Hash64,
        daa: u64,
        view: &OpvEligibilityViewV1<'_>,
    ) -> Result<(), OpvIneligibleV1> {
        let binding = self.kernel_binding_v1(v2_class).ok_or(OpvIneligibleV1::NotOnboarded)?;
        let facts = OpvClassFactsV1::of_registered(ledger, &binding.kernel_class.as_bytes())
            .ok_or_else(|| OpvIneligibleV1::NotG14Complete("the bound kernel class is not registered".into()))?;
        if [facts.opv_id, facts.legacy_id].iter().any(|id| view.denied.contains(&Hash64::from_bytes(*id))) {
            return Err(OpvIneligibleV1::Denied);
        }
        if ledger.schedule.standing_at(&facts.descriptor, daa) != KernelStandingV1::Active {
            return Err(OpvIneligibleV1::KernelNotActive);
        }
        self.opv_eligibility_through_v1(ledger, &facts, daa, view, v2_class, &binding)
    }

    /// **Eligibility of an OPV class by id** (registered single-program or pipeline): a pipeline class is eligible only through the
    /// test seam (no onboarding path).
    /// **A typed-root class's derived OPV eligibility** (RFC-0004 Part II; lane G14C GAP-50, closing OPVB's GAP-B16 for the kinds
    /// that have a path). A typed class adds no relation of its own that a conformance would sample (its memory edges, retrieval
    /// orderings and composite edges are exact and checked at inclusion or by their own exact courts), so its eligibility is derived
    /// from the classes whose computation it runs:
    ///
    /// * `Weights` — the identical single-program registration's (E1–E7 through its own kernel binding);
    /// * `Memory` — its rule program's single-program registration over the class's weights (the base weights AND `M0`: the
    ///   parameter commitments the artifact binding covers), E1–E7 through that registration's kernel binding;
    /// * `Composite` — every stage's component, each eligible in its own right (a model stage: a registered single-program OPV class;
    ///   a retrieval stage: never, below);
    /// * `Retrieval` — never: no conformance statement exists for a snapshot yet ([`OpvIneligibleV1::SnapshotNotOnboardable`]).
    ///
    /// The typed class's own id is checked against the deny-list first (and the test seam, which only names), so a typed class can be
    /// denied without denying the model it reuses.
    pub fn opv_spec_eligibility_v1(
        &self,
        ledger: &KernelLedgerV1,
        spec: &misaka_palw_kernel::spec::ComputationSpecV1,
        daa: u64,
        view: &OpvEligibilityViewV1<'_>,
    ) -> Result<OpvEligibleV1, OpvIneligibleV1> {
        use misaka_palw_kernel::spec::{SpecClassKindV1, SpecShapeV1};
        let class = spec.class_id().map_err(OpvIneligibleV1::NotG14Complete)?;
        let id = Hash64::from_bytes(class);
        if view.denied.contains(&id) {
            return Err(OpvIneligibleV1::Denied);
        }
        if view.test_eligible.contains(&id) {
            return Ok(OpvEligibleV1::TestHook);
        }
        let of_weights = |w: &misaka_palw_kernel::spec::WeightsRootV1| {
            OpvClassFactsV1::of_registration(w.descriptor, &w.program_bytes, &w.plan, &w.param_commitments)
        };
        match spec.shape().map_err(OpvIneligibleV1::NotG14Complete)? {
            SpecShapeV1::Weights(w) | SpecShapeV1::Memory(w, _) => self.opv_eligibility_v1(ledger, &of_weights(w), daa, view),
            SpecShapeV1::Retrieval(_) => Err(OpvIneligibleV1::SnapshotNotOnboardable),
            SpecShapeV1::Composite(c) => {
                let mut eligible = Err(OpvIneligibleV1::NotG14Complete("a composite with no stage".into()));
                for stage in &c.stages {
                    if ledger.classes.contains_key(&stage.component) {
                        eligible = Ok(self.opv_class_eligibility_v1(ledger, &stage.component, daa, view)?);
                    } else if let Some(row) = ledger.typed.classes.get(&stage.component) {
                        match &row.kind {
                            SpecClassKindV1::Retrieval { .. } => return Err(OpvIneligibleV1::SnapshotNotOnboardable),
                            _ => {
                                return Err(OpvIneligibleV1::NotG14Complete(
                                    "a composite stage that is neither a model nor a tool".into(),
                                ));
                            }
                        }
                    } else {
                        return Err(OpvIneligibleV1::NotOnboarded);
                    }
                }
                eligible
            }
        }
    }

    pub fn opv_class_eligibility_v1(
        &self,
        ledger: &KernelLedgerV1,
        class: &Digest,
        daa: u64,
        view: &OpvEligibilityViewV1<'_>,
    ) -> Result<OpvEligibleV1, OpvIneligibleV1> {
        match OpvClassFactsV1::of_registered(ledger, class) {
            Some(facts) if facts.opv_id == *class => self.opv_eligibility_v1(ledger, &facts, daa, view),
            Some(_) => Err(OpvIneligibleV1::NotOnboarded), // a legacy class is never "OPV-eligible" under its own id
            None => {
                let id = Hash64::from_bytes(*class);
                if view.denied.contains(&id) {
                    Err(OpvIneligibleV1::Denied)
                } else if view.test_eligible.contains(&id) {
                    Ok(OpvEligibleV1::TestHook)
                } else {
                    Err(OpvIneligibleV1::PipelineNotOnboardable)
                }
            }
        }
    }

    /// **The OPV-eligible class ids at `daa`** (registered or not yet): the OPV id of every kernel-bound class (its own id under OPV,
    /// or its legacy class's OPV sibling) that is eligible, and the test seam's registered ids. Sorted, unique. What a conformance
    /// commitment freezes as its beacon's possible sources.
    pub fn opv_eligible_set_v1(&self, ledger: &KernelLedgerV1, daa: u64, view: &OpvEligibilityViewV1<'_>) -> Vec<Hash64> {
        let mut candidates: BTreeSet<Digest> = BTreeSet::new();
        for (_, binding) in self.kernel_bindings_v1() {
            if let Some(facts) = OpvClassFactsV1::of_registered(ledger, &binding.kernel_class.as_bytes()) {
                candidates.insert(facts.opv_id);
            }
        }
        // The test seam's ids count only where this ledger holds them (the seam is process-wide in a test binary).
        for id in view.test_eligible {
            if ledger.classes.contains_key(&id.as_bytes()) || ledger.pipeline_classes.contains_key(&id.as_bytes()) {
                candidates.insert(id.as_bytes());
            }
        }
        let mut out: Vec<Hash64> = candidates
            .into_iter()
            .filter(|id| {
                let facts = OpvClassFactsV1::of_registered(ledger, id).or_else(|| {
                    // Not registered under OPV yet: its facts are its legacy sibling's (same program, plan and commitments).
                    self.kernel_bindings_v1().into_iter().find_map(|(_, b)| {
                        OpvClassFactsV1::of_registered(ledger, &b.kernel_class.as_bytes()).filter(|f| f.opv_id == *id)
                    })
                });
                match facts {
                    Some(f) if f.opv_id == *id => self.opv_eligibility_v1(ledger, &f, daa, view).is_ok(),
                    _ => self.opv_class_eligibility_v1(ledger, id, daa, view).is_ok(),
                }
            })
            .map(Hash64::from_bytes)
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The ids of the candidate under every mode: a V2 class's bound kernel class and that class's sibling under the other mode
    /// (never a source of the candidate's own beacon).
    pub fn candidate_kernel_ids_v1(&self, ledger: &KernelLedgerV1, binding: &KernelBindingRowV1) -> Vec<Hash64> {
        let mut out = vec![binding.kernel_class];
        if let Some(f) = OpvClassFactsV1::of_registered(ledger, &binding.kernel_class.as_bytes()) {
            out.push(Hash64::from_bytes(f.opv_id));
            out.push(Hash64::from_bytes(f.legacy_id));
        }
        out.sort();
        out.dedup();
        out
    }

    /// **The complete checks judged at `daa`** (the per-block cap counts them from the attempt rows: a judged check always leaves
    /// its evidence on the attempt, pass or fail, and a complete-check class cannot re-commit in the block that closed its attempt —
    /// `apply_conformance_committed_v1` — so no judgement of this block is overwritten before it is counted).
    pub fn complete_checks_judged_at_v1(&self, daa: u64) -> u32 {
        use crate::palw_onboarding_v1::PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1 as T;
        self.aux
            .range((T, Vec::new())..(T + 1, Vec::new()))
            .filter_map(|(_, row)| borsh::from_slice::<ConformanceAttemptRowV1>(row).ok())
            .filter(|a| a.is_complete_check() && a.evidence.is_some_and(|e| e.posted_daa == daa))
            .count() as u32
    }
}

// =================================================================================================================================
// V. G14-for-rewards (`docs/PRINCIPLES.md` §6)
// =================================================================================================================================

/// **Whether a V2 class may take claims at a block, and on which footing.** Past the fence, a class that never passed the
/// onboarding/G14 path is `Refused` on every lane. A class that passed is `Onboarded`, and that grants its V2-root claims nothing
/// beyond the old rules (GAP-81). A CLAIM's reward channel is its verification route ([`PalwRewardChannelV1`]), never this verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwRewardGateV1 {
    /// The gate is not armed at this block (no `palw_panel_free_v1` in force): the pre-gate rules, byte for byte.
    Unarmed,
    /// Not gated: the base class (the bonded BASE-0 fallback, never useful-computation reward), or a class registered BEFORE the
    /// fence (`LEGACY_PANEL_ROUTE`): it keeps earning through the OLD Panel route, whose verification stays in force in full — no
    /// G14 bypass applies to it, and its OPV rewards (kernel-route OPV claims) still require E1–E7 (`opv_gate_v1`).
    Exempt(&'static str),
    /// The class passed the onboarding/G14 path: the onboarding gate's identity part is `Ready` and E1–E7 hold through its own
    /// kernel binding. **That opens no new reward for its V2-root claims** (the Lead's GAP-81 decision, 2026-10-10: the gate is per
    /// CLAIM verification route). Its V2 claims carry V2 step-tree roots, which the kernel route cannot convict, so they ride the
    /// legacy channel under the old rules in full: registry lifecycle, Panel room, verify deadline, seating, the bond-share split and
    /// the share it registered with. Only its kernel-route claims earn the new rewards, each gated by `opv_gate_v1`
    /// ([`PalwRewardChannelV1`]).
    Onboarded,
    /// Registered, perhaps even Active, but earning nothing: `code` names the first unmet condition.
    Refused { code: &'static str, why: String },
}

/// **G14-for-rewards** (the Lead's 2026-10-09 scope; `docs/PRINCIPLES.md` §6). From the fence's activation on, a class earns reward or
/// consensus work weight only through the onboarding/G14 path:
///
/// 1. unarmed (`reward_gate` is `None`) → [`PalwRewardGateV1::Unarmed`];
/// 2. the base class → exempt (the BASE-0 fallback);
/// 3. a class registered before the activation → exempt on the OLD Panel route, in full (the user's ruling of 2026-10-09: past
///    rights are protected; the NEW reward channel — the kernel route's claims — always requires the new gate; the two never mix);
/// 4. the onboarding gate's IDENTITY part (`onboarding_identity_gate_v1`) must be `Ready`: the artifact binding Final, the kernel
///    class standing (registered only after PUBLIC_PROSECUTION_COMPLETE), the conformance record at G14_ELIGIBLE or
///    ACTIVE_REWARDABLE for this artifact. **Never DA16's artifact-lapse hold** (ADR-0177: no reward condition may depend on a model
///    being served; design §14.1);
/// 5. E1–E7 through its own kernel binding ([`PalwKernelRouteStateV1::v2_class_reward_eligibility_v1`]): the kernel Active, the
///    conformance about this statement, the binding standing, bounded prosecution (carriers, court budget, censorship cost above the
///    claim's gain), a verified policy at the effective-bits floor, not denied.
///
/// The seven conditions of §6 map onto 4–5: coverage (the kernel class's plan, PUBLIC_PROSECUTION_COMPLETE), approved soundness and
/// grinding resistance (E6's effective bits — check math and challenge manipulation only; detection is conditional on a verifier
/// holding the model, A-ACQ, design §14.2), public authenticated material (E4's standing binding), one-verifier localization and objective adjudication (E3, E5's
/// court budget), collectable collateral above the gain (E5), honest verifiers' resources (E5's carriers and the OPV budgets), and
/// dispute/DA/Final/reorg consistency (the route's bounded deadlines; reorg consistency is the fork-choice fence's, ordered by
/// validation). Cost: the ledger is rebuilt from the rows per call (GAP-B8).
pub fn palw_reward_gate_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    base_class: &Hash64,
    route_extras: Option<&crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1>,
    class_id: &Hash64,
    daa: u64,
) -> PalwRewardGateV1 {
    use crate::palw_onboarding_v1::PalwOnboardingGateV1 as G;
    let refused = |code: &'static str, why: String| PalwRewardGateV1::Refused { code, why };
    let Some(opv) = route_extras.and_then(|e| e.opv.as_ref()) else { return PalwRewardGateV1::Unarmed };
    let Some(terms) = opv.reward_gate else { return PalwRewardGateV1::Unarmed };
    if class_id == base_class {
        return PalwRewardGateV1::Exempt("BASE_FLOOR");
    }
    let Some(class) = state.class(class_id) else { return refused("NOT_REGISTERED", "no such V2 class".into()) };
    if class.registered_daa < terms.fence_activation_daa {
        return PalwRewardGateV1::Exempt("LEGACY_PANEL_ROUTE");
    }
    let route = state.kernel_route();
    let Some(route) = route else {
        return refused("NOT_ONBOARDED", "no kernel route state: the class never began onboarding".into());
    };
    match route.onboarding_identity_gate_v1(class_id, &class.artifact_root, daa) {
        G::NotKernelBound => {
            return refused("NOT_ONBOARDED", "the class has no artifact or kernel binding: it never began onboarding".into());
        }
        G::Held { code, why } => return refused(code, why.to_string()),
        G::Ready => {}
    }
    let ledger = match route.ledger() {
        Ok(l) => l,
        Err(e) => return refused("ROUTE_ROWS", e),
    };
    if let Err(why) = route.v2_class_reward_eligibility_v1(&ledger, class_id, daa, &OpvEligibilityViewV1::of(opv)) {
        return refused(why.code(), format!("{why:?}"));
    }
    // Task/context: every job the V2 class takes (its TIR layout's `max_context`) must lie inside the context G14 was proven for (the
    // bound kernel plan's `max_positions`).
    let proven_context = route
        .kernel_binding_v1(class_id)
        .and_then(|b| route.kernel_class_record_v1(&b.kernel_class))
        .map(|record| record.plan.max_positions);
    match (state.tir_class_v1(class_id).map(|tir| tir.facts.max_context), proven_context) {
        (Some(served), Some(proven)) if served <= proven => PalwRewardGateV1::Onboarded,
        (Some(served), Some(proven)) => refused(
            "NOT_G14_COMPLETE",
            format!("the class serves contexts of {served} positions; G14 was proven for {proven} (the bound plan)"),
        ),
        _ => refused("NOT_G14_COMPLETE", "the class's IR record or its bound kernel class is missing".into()),
    }
}

// =================================================================================================================================
// VI. The reward channels and the per-bond budget they draw from (ADR-0176; design §14.3)
// =================================================================================================================================

/// **ADR-0176's four per-bond ceilings** over ONE common DAA window `W`: the same bond, window and `rho` give the same ceilings
/// whether the producer computes honestly or forges fast. Raising `rho` raises `Q` only, never `B`, `R` or `F`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwBondBudgetLegV1 {
    /// `Q`: claims (an attempt, a kernel-route claim, a beacon source — each is one).
    Claims,
    /// `B`: reward-bearing blocks.
    RewardBlocks,
    /// `R`: rewards attributed to the bond, issuance or escrow alike (a user-paid `FinalReward` counts).
    Rewards,
    /// `F`: Final weight.
    FinalWeight,
}

/// **When a channel touches its bond's budget** (ADR-0176 D3): the maximum of every leg is reserved at acceptance; each later event
/// re-checks that reservation and consumes from it, never more than was reserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwBudgetEventV1 {
    /// Accepted at DAA `d`: every leg's maximum reserved, held until `reuse_not_before = d + W` whatever happens next (a Final, void,
    /// default, conviction, `BEACON_VETOED` or retry releases nothing early).
    Accept,
    /// A reward-bearing block derived from it.
    RewardBlock,
    /// A reward paid out.
    Payout,
    /// It reached Final (or matured).
    Final,
}

/// One leg a channel draws: reserved at [`PalwBudgetEventV1::Accept`], consumed at `consumed_at`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBudgetDrawV1 {
    pub leg: PalwBondBudgetLegV1,
    pub consumed_at: PalwBudgetEventV1,
}

/// **The two reward channels — a CLAIM's channel is its verification route** (the Lead's GAP-81 decision, 2026-10-10; the user's
/// ruling of 2026-10-09: the channels never mix). Only a claim whose work is verified on a G14-complete route — the kernel route —
/// earns the NEW rewards. A V2-root claim rides the legacy channel under the old rules in full, whatever class it belongs to and
/// whatever [`PalwRewardGateV1`] says of that class. Both channels draw from the producer bond's ONE ADR-0176 budget, so earning
/// through both never doubles a ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwRewardChannelV1 {
    /// The OLD channel: every V2-root claim (Attempt, Free Prompt, Evaluation) — a pre-fence class's on the Panel route, and an
    /// onboarded post-fence class's too: its V2 roots are not convictable by the kernel route, so they never earn the new rewards.
    LegacyPanelRoute,
    /// The NEW channel: a kernel-route claim, gated per claim (an OPV class's claim commits only while E1–E7 hold, `opv_gate_v1`) —
    /// its `FinalReward` out of the job's escrow (G14-R4's user-pays GAP-5). No reward block. `F` is reserved at 0 today (a kernel
    /// Final carries no consensus weight; the policy's `work_credit_per_claim` is only a gain bound), so any future weight is
    /// reserved at acceptance like every other leg.
    KernelRoute,
}

impl PalwRewardChannelV1 {
    pub const ALL: [Self; 2] = [Self::LegacyPanelRoute, Self::KernelRoute];

    /// **The budget a channel draws from**, leg by leg (design §14.3's table as data).
    pub fn draws(self) -> &'static [PalwBudgetDrawV1] {
        use PalwBondBudgetLegV1 as L;
        use PalwBudgetEventV1 as E;
        const V2_WORK: &[PalwBudgetDrawV1] = &[
            PalwBudgetDrawV1 { leg: L::Claims, consumed_at: E::Accept },
            PalwBudgetDrawV1 { leg: L::RewardBlocks, consumed_at: E::RewardBlock },
            PalwBudgetDrawV1 { leg: L::Rewards, consumed_at: E::Payout },
            PalwBudgetDrawV1 { leg: L::FinalWeight, consumed_at: E::Final },
        ];
        const KERNEL_OPV: &[PalwBudgetDrawV1] = &[
            PalwBudgetDrawV1 { leg: L::Claims, consumed_at: E::Accept },
            PalwBudgetDrawV1 { leg: L::Rewards, consumed_at: E::Final },
            PalwBudgetDrawV1 { leg: L::FinalWeight, consumed_at: E::Final },
        ];
        match self {
            Self::LegacyPanelRoute => V2_WORK,
            Self::KernelRoute => KERNEL_OPV,
        }
    }
}

/// **What a reward door asks the engine to reserve at acceptance.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBudgetRequestV1 {
    pub channel: PalwRewardChannelV1,
    /// The producer bond: its ONE budget, whatever the channel.
    pub bond: crate::palw_state_v2::PalwBondKeyV2,
    /// The attempt or claim the reservation is for (the engine's key: one reservation per subject).
    pub subject: Hash64,
    /// The acceptance DAA `d`.
    pub accepted_daa: u64,
    /// The maximum per leg, in each leg's unit — `[Q (claims), B (blocks), R (sompi), F (weight)]`; a leg the channel does not draw is 0.
    pub max: [u128; 4],
}

/// Why the engine refused a reservation or a consumption (nothing was reserved or consumed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBudgetRefusalV1 {
    pub leg: PalwBondBudgetLegV1,
    pub why: String,
}

/// **The hook BUDGET's engine implements** (`palw_bond_budget_v1`, dormant; ADR-0176). Every reward door — a kernel-route claim's
/// admission and Final (the new channel), and a V2-root claim's admission, reward blocks, payouts and Final (the legacy channel) —
/// calls it with the draw it is about to make. **This branch wires no implementation**: until the engine exists, every reward,
/// consensus-weight and Panel=0 fence stays refused ("rewards not budgeted", `Params::validate_palw_panel_free_v1`).
pub trait PalwBondBudgetHookV1 {
    /// Reserve `request.max` on every leg of `request.bond`'s budget over the window containing `request.accepted_daa`, or refuse with
    /// nothing reserved when any leg lacks headroom. The reservation's capacity returns no earlier than `accepted_daa + W`.
    fn reserve(&mut self, request: &PalwBudgetRequestV1) -> Result<(), PalwBudgetRefusalV1>;

    /// Re-check, then consume `amount` of `leg` from `subject`'s reservation at `event`; refused (nothing consumed) above what remains
    /// reserved, or where the network or model budget (ADR-0177's allocation) no longer covers it.
    fn consume(
        &mut self,
        bond: &crate::palw_state_v2::PalwBondKeyV2,
        subject: &Hash64,
        event: PalwBudgetEventV1,
        leg: PalwBondBudgetLegV1,
        amount: u128,
    ) -> Result<(), PalwBudgetRefusalV1>;

    /// `reuse_not_before = d + W` of `subject`'s reservation (`None`: no such reservation).
    fn reuse_not_before(&self, bond: &crate::palw_state_v2::PalwBondKeyV2, subject: &Hash64) -> Option<u64>;
}

// =================================================================================================================================
// IV. The dependency graph
// =================================================================================================================================

/// A state or object of the onboarding → OPV → beacon chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwOpvNodeV1 {
    Genesis,
    KernelActive,
    V2Registered,
    ArtifactMatured,
    ArtifactFinal,
    KernelClassStands,
    KernelBound,
    CompleteQualified,
    CommittedSampled,
    CommittedComplete,
    EligibleSourcesNonEmpty,
    BeaconLocked,
    SampledEvidence,
    CompleteEvidence,
    ConformancePassed,
    G14Eligible,
    V2Active,
    /// The V2 class passed the onboarding/G14 path ([`palw_reward_gate_v1`]: `Onboarded`). It may take V2-root claims under the OLD
    /// rules and never earns a new reward through them (GAP-81).
    V2Onboarded,
    BoundsFit,
    PolicyVerified,
    NotDenied,
    OpvEligible,
    OpvClassRegistered,
    OpvClaim,
    OpvFinal,
    ApprovedPanelScheme,
    PanelAssignmentBeacon,
    V3PanelBound,
    WorkSliceBeacon,
}

/// One derivation: `node` holds once every `requires` holds (several rows of one node are alternatives).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwOpvDependencyV1 {
    pub node: PalwOpvNodeV1,
    pub requires: &'static [PalwOpvNodeV1],
    /// The gate function that enforces it.
    pub enforced_by: &'static str,
}

const fn dep(node: PalwOpvNodeV1, requires: &'static [PalwOpvNodeV1], enforced_by: &'static str) -> PalwOpvDependencyV1 {
    PalwOpvDependencyV1 { node, requires, enforced_by }
}

use PalwOpvNodeV1 as N;

/// **The graph, as the code enforces it** (`docs/design/palw/opv-beacon-bootstrap.md` §3). `ApprovedPanelScheme` has no row: the
/// release approves no Panel beacon scheme (`approved_panel_beacon_policies_v1() = []`), so RFC-0010 V3 binds nothing yet.
pub const PALW_OPV_DEPENDENCIES_V1: &[PalwOpvDependencyV1] = &[
    dep(N::KernelActive, &[N::Genesis], "KernelScheduleV1::standing_at (the route's template schedule)"),
    dep(N::V2Registered, &[N::Genesis], "tag 108 SignedRegistrationV1 -> ClassRegisteredTirV1"),
    dep(N::ArtifactMatured, &[N::V2Registered], "apply_artifact_bound_v1 + onboarding_attested_roots_v1"),
    dep(N::ArtifactFinal, &[N::ArtifactMatured], "ArtifactBindingRowV1::state_at"),
    dep(
        N::KernelClassStands,
        &[N::ArtifactMatured, N::KernelActive],
        "kernel register_class: attested artifact, public_prosecution_complete_v1, court budget, carrier fit",
    ),
    dep(N::KernelBound, &[N::V2Registered, N::KernelClassStands], "apply_kernel_bound_v1"),
    dep(N::CompleteQualified, &[N::KernelBound], "palw_complete_check_domain_v1 (checked at 106 for the complete-check policy)"),
    dep(N::CommittedSampled, &[N::KernelBound], "apply_conformance_committed_v1 (the sampled policy)"),
    dep(N::CommittedComplete, &[N::KernelBound, N::CompleteQualified], "apply_conformance_committed_v1 (the complete-check policy)"),
    dep(
        N::EligibleSourcesNonEmpty,
        &[N::OpvFinal],
        "opv_eligible_set_v1 frozen at 107: OPV Finals of OTHER classes eligible at the commitment",
    ),
    dep(N::BeaconLocked, &[N::CommittedSampled, N::EligibleSourcesNonEmpty], "collect_attributed_work_beacon_v1"),
    dep(N::SampledEvidence, &[N::BeaconLocked], "judge_posted_evidence_v1"),
    dep(N::CompleteEvidence, &[N::CommittedComplete], "judge_complete_check_v1 (no seed, no beacon)"),
    dep(N::ConformancePassed, &[N::SampledEvidence], "tick_conformance_v1: the window closes unrefuted, the beacon unchanged"),
    dep(N::ConformancePassed, &[N::CompleteEvidence], "apply_complete_check_v1: a pass at once"),
    dep(N::G14Eligible, &[N::ConformancePassed, N::KernelClassStands], "OnboardingStepV1::PublicProsecutionGate"),
    dep(N::V2Active, &[N::G14Eligible, N::ArtifactFinal], "onboarding_gate_v1 / activate_due_classes"),
    dep(
        N::V2Onboarded,
        &[N::V2Active, N::KernelActive, N::ArtifactMatured, N::BoundsFit, N::PolicyVerified, N::NotDenied],
        "palw_reward_gate_v1 Onboarded: identity gate Ready and E1-E7 through its binding; its V2-root claims keep the OLD rules (GAP-81)",
    ),
    dep(N::BoundsFit, &[N::KernelClassStands], "opv_eligibility_v1 E5"),
    dep(
        N::PolicyVerified,
        &[N::CompleteEvidence],
        "opv_eligibility_v1 E6: the complete check (GAP-70: a sampled conformance gates no reward) at the effective-bits floor",
    ),
    dep(N::NotDenied, &[N::Genesis], "opv_eligibility_v1 E7: the fence's denied_classes"),
    dep(
        N::OpvEligible,
        &[N::KernelActive, N::G14Eligible, N::ArtifactMatured, N::BoundsFit, N::PolicyVerified, N::NotDenied],
        "opv_eligibility_v1",
    ),
    dep(N::OpvClassRegistered, &[N::OpvEligible], "apply_kernel_route_object_v1: admission at tag 13 + kernel register_class"),
    dep(N::OpvClaim, &[N::OpvClassRegistered, N::OpvEligible], "apply_kernel_route_object_v1: the gate before CommitClaim"),
    dep(N::OpvFinal, &[N::OpvClaim], "kernel tick: the window ends with no accepted dispute"),
    dep(N::PanelAssignmentBeacon, &[N::OpvFinal, N::ApprovedPanelScheme], "verify_panel_beacon_v1"),
    dep(N::V3PanelBound, &[N::PanelAssignmentBeacon], "palw_panel_v3_fold_v1"),
    dep(N::WorkSliceBeacon, &[N::OpvFinal], "RFC-0008 §6 (not on the integration tree)"),
];

/// **The least fixed point from `Genesis`**: every node some derivation reaches using only nodes already reached, with the rows of
/// `disabled` nodes removed. A node in a cycle with no derivation outside it is never reached — which is what a startup cycle is.
pub fn reachable_v1(rows: &[PalwOpvDependencyV1], disabled: &[PalwOpvNodeV1]) -> BTreeSet<PalwOpvNodeV1> {
    let mut reached: BTreeSet<PalwOpvNodeV1> = [N::Genesis].into_iter().collect();
    loop {
        let before = reached.len();
        for row in rows {
            if !disabled.contains(&row.node) && !reached.contains(&row.node) && row.requires.iter().all(|r| reached.contains(r)) {
                reached.insert(row.node);
            }
        }
        if reached.len() == before {
            return reached;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The verdict, as a check of the graph.** With the complete check, every OPV state is reachable from genesis; remove the
    /// complete-check derivation and `OpvEligible` — and with it every beacon — is unreachable: the startup cycle. The Panel
    /// assignment beacon is unreachable either way (no approved scheme).
    #[test]
    fn the_graph_reaches_opv_from_genesis_only_through_the_complete_check() {
        let all = reachable_v1(PALW_OPV_DEPENDENCIES_V1, &[]);
        for n in [N::OpvEligible, N::OpvFinal, N::BeaconLocked, N::SampledEvidence, N::V2Active, N::V2Onboarded, N::WorkSliceBeacon] {
            assert!(all.contains(&n), "{n:?} is reachable with the bootstrap");
        }
        for n in [N::ApprovedPanelScheme, N::PanelAssignmentBeacon, N::V3PanelBound] {
            assert!(!all.contains(&n), "{n:?} needs an approved Panel beacon scheme (none)");
        }
        let without = reachable_v1(PALW_OPV_DEPENDENCIES_V1, &[N::CompleteEvidence]);
        for n in [
            N::ConformancePassed,
            N::G14Eligible,
            N::OpvEligible,
            N::OpvFinal,
            N::EligibleSourcesNonEmpty,
            N::BeaconLocked,
            N::V2Onboarded,
        ] {
            assert!(!without.contains(&n), "{n:?} must be unreachable without the bootstrap: the cycle");
        }
        assert!(without.contains(&N::CommittedSampled), "a sampled commitment is still made — and waits forever (BEACON_UNAVAILABLE)");
    }

    /// **A cycle introduced is caught**: make the complete check need the beacon (a bootstrap whose activation needs randomness) and
    /// `OpvEligible` falls out of the fixed point.
    #[test]
    fn a_bootstrap_that_needs_the_beacon_closes_the_cycle_and_the_walk_says_so() {
        let mut rows: Vec<PalwOpvDependencyV1> = PALW_OPV_DEPENDENCIES_V1.to_vec();
        for r in rows.iter_mut().filter(|r| r.node == N::CompleteEvidence) {
            r.requires = &[N::CommittedComplete, N::BeaconLocked];
        }
        assert!(!reachable_v1(&rows, &[]).contains(&N::OpvEligible));
    }

    /// Every refusal reason of the predicate is the absence of a predecessor of `OpvEligible`, and every predecessor has a reason.
    #[test]
    fn every_eligibility_reason_is_an_edge_of_the_graph_and_every_edge_a_reason() {
        let preds: BTreeSet<PalwOpvNodeV1> =
            PALW_OPV_DEPENDENCIES_V1.iter().filter(|r| r.node == N::OpvEligible).flat_map(|r| r.requires.iter().copied()).collect();
        let reasons: BTreeSet<PalwOpvNodeV1> = OpvIneligibleV1::all().iter().map(OpvIneligibleV1::predecessor).collect();
        assert_eq!(preds, reasons);
        let codes: BTreeSet<&str> = OpvIneligibleV1::all().iter().map(OpvIneligibleV1::code).collect();
        assert_eq!(codes.len(), OpvIneligibleV1::all().len(), "codes are distinct");
    }

    /// **ADR-0176: both reward channels name the per-bond budget they draw, reserved at acceptance.** The legacy channel (V2-root
    /// claims) draws all four legs; the new channel (kernel-route claims) draws `Q`, `R` and a reserved `F` and no reward block; each
    /// consumes `Q` at acceptance and `R` / `F` only later.
    #[test]
    fn every_reward_channel_names_the_bond_budget_it_draws_and_reserves_it_at_acceptance() {
        use PalwBondBudgetLegV1 as L;
        use PalwBudgetEventV1 as E;
        for c in PalwRewardChannelV1::ALL {
            let d = c.draws();
            assert!(d.contains(&PalwBudgetDrawV1 { leg: L::Claims, consumed_at: E::Accept }), "{c:?} draws Q at acceptance");
            assert!(d.iter().any(|x| x.leg == L::Rewards && x.consumed_at != E::Accept), "{c:?} draws R, consumed after acceptance");
            assert!(d.iter().any(|x| x.leg == L::FinalWeight && x.consumed_at == E::Final), "{c:?} reserves F, consumed at Final");
            let legs: BTreeSet<L> = d.iter().map(|x| x.leg).collect();
            assert_eq!(legs.len(), d.len(), "{c:?}: one draw per leg");
        }
        assert_eq!(PalwRewardChannelV1::LegacyPanelRoute.draws().len(), 4, "a V2-root claim draws every leg");
        assert!(PalwRewardChannelV1::KernelRoute.draws().iter().all(|x| x.leg != L::RewardBlocks), "a kernel claim makes no block");
    }

    /// **ADR-0177: no condition of eligibility, the beacon or the reward gate asks whether a model can be acquired.** No refusal
    /// code and no enforcing function of the graph names availability, a lease, a provider or a lapse; and the reward gate reads the
    /// onboarding gate's IDENTITY part, never `onboarding_gate_v1` (which still carries DA16's withdrawn artifact-lapse hold).
    #[test]
    fn no_condition_of_opv_eligibility_the_beacon_or_the_reward_gate_depends_on_acquiring_a_model() {
        const FORBIDDEN: [&str; 6] = ["AVAILAB", "LEASE", "PROVIDER", "LAPSE", "SEEDER", "DA_"];
        let texts: Vec<String> = OpvIneligibleV1::all()
            .iter()
            .map(|r| r.code().to_string())
            .chain(PALW_OPV_DEPENDENCIES_V1.iter().map(|r| r.enforced_by.to_uppercase()))
            .collect();
        for t in &texts {
            for f in FORBIDDEN {
                assert!(!t.contains(f), "{t:?} names {f}: an acquisition condition (ADR-0177)");
            }
        }
        let src = include_str!("palw_opv_bootstrap_v1.rs");
        let at = src.find("pub fn palw_reward_gate_v1(").expect("the gate");
        let body = &src[at..at + src[at..].find("\n}\n").expect("its end")];
        assert!(body.contains(".onboarding_identity_gate_v1("), "the gate reads the identity gate");
        assert!(!body.contains(".onboarding_gate_v1("), "the gate never reads DA16's lapse hold");
    }

    /// The network's complete-check policy is a valid complete check, has no beacon, and is a different id from the sampled one.
    #[test]
    fn the_complete_check_policy_validates_and_draws_no_beacon() {
        let c = palw_onboarding_complete_check_policy_v1();
        c.validate().unwrap();
        assert!(c.is_complete_check() && !c.needs_beacon());
        let s = crate::palw_conformance_evidence_v1::palw_onboarding_challenge_policy_v1();
        assert!(s.needs_beacon());
        assert_ne!(c.id(), s.id());
        assert_eq!(c.retry_limit, s.retry_limit);
    }
}
