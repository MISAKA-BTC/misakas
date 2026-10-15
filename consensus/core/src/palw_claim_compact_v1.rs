//! **R7-RELATION round 3: the consensus gate of a compact relation contract.** Dormant: admission needs the ClaimV2 fence active
//! (public validation keeps `palw_claim_challenge_v2` at `never`) and an approved review tuple naming the contract's suite (the
//! shipped registry is empty). Nothing calls this from a block path yet; no wire id, aux table or state tag is allocated.

use crate::config::params::Params;
use misaka_palw_challenge::PostCommitChallengePolicyV1;
use misaka_palw_challenge::composition::AdversaryBudgetV1;
use misaka_palw_kernel::claim_compact_v1::{
    CompactContractSummaryV1, CompactPlanCoverageV1, CompactRelationContractV1, register_compact_relation_contract_v1,
};

/// Register a compact relation contract at `daa`: the ClaimV2 fence must be present and active, then the kernel's structural,
/// coverage, bound and approved-review checks run against the shipped (empty) registry. Today it always refuses.
pub fn palw_compact_relation_registration_v1(
    params: &Params,
    daa: u64,
    contract: &CompactRelationContractV1,
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    base_class: misaka_palw_kernel::hash::Digest,
    coverage: &CompactPlanCoverageV1,
    adversary: &AdversaryBudgetV1,
    policy: &PostCommitChallengePolicyV1,
) -> Result<CompactContractSummaryV1, String> {
    let fence = params.palw_claim_challenge_v2.as_ref().ok_or("compact relation contracts need the ClaimV2 fence, which is absent")?;
    if !fence.activation.is_active(daa) {
        return Err("the ClaimV2 fence is dormant: compact relation contracts are not admitted".into());
    }
    let ledger = route.ledger()?;
    let native = misaka_palw_kernel::claim_compact_v1::native_v2::compile_native_class_v2(&ledger, base_class, coverage.params)?;
    if native.target != contract.target
        || native.parts.len() != 1
        || native.parts[0].maximum_executions != 1
        || native.parts[0].coverage != *coverage
    {
        return Err("single-plan contract does not cover the whole actual native class; use the whole-class contract boundary".into());
    }
    let class=ledger.classes.get(&base_class).ok_or("single-plan registration requires the actual native Program/Segmented class; pipeline/typed whole-class registration is separate")?;
    register_compact_relation_contract_v1(
        contract,
        coverage,
        &class.program,
        &class.plan,
        adversary,
        &misaka_palw_challenge::shipped_registry_v2(),
        policy,
    )
}

/// Whole native class coverage from authenticated route rows. This is a
/// deterministic compiler/read API; it grants no receipt success or activation.
/// Unlike a caller supplied stage list, every registered pipeline/composite
/// component, memory step multiplier and exact retrieval/edge boundary is bound.
pub fn palw_native_compact_coverage_v2(
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    class: misaka_palw_kernel::hash::Digest,
    compile: misaka_palw_kernel::claim_compact_v1::CompactCompileParamsV1,
) -> Result<misaka_palw_kernel::claim_compact_v1::native_v2::NativeCompactCoverageV2, String> {
    let ledger = route.ledger()?;
    misaka_palw_kernel::claim_compact_v1::native_v2::compile_native_class_v2(&ledger, class, compile)
}

/// Whole-class reviewed preflight. This API checks native authority and the
/// compiled ClaimV2 fence, but is not an inclusion path or a receipt/Final grant.
pub fn palw_native_compact_registration_v2(
    params: &Params,
    daa: u64,
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    contract: &misaka_palw_kernel::claim_compact_v1::native_v2::NativeCompactContractV2,
    coverage: &misaka_palw_kernel::claim_compact_v1::native_v2::NativeCompactCoverageV2,
    budget: &AdversaryBudgetV1,
    policy: &PostCommitChallengePolicyV1,
) -> Result<misaka_palw_kernel::claim_compact_v1::native_v2::NativeCompactContractSummaryV2, String> {
    let fence = params.palw_claim_challenge_v2.as_ref().ok_or("ClaimV2 compiled fence absent")?;
    if !fence.activation.is_active(daa) {
        return Err("ClaimV2 compact registration is dormant".into());
    }
    let ledger = route.ledger()?;
    misaka_palw_kernel::claim_compact_v1::native_v2::register_native_compact_contract_v2(
        contract,
        &ledger,
        coverage,
        budget,
        &misaka_palw_challenge::shipped_registry_v2(),
        policy,
    )
}
