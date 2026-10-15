//! **R7-RELATION round 3: the consensus gate of a compact relation contract.** Dormant: admission needs the ClaimV2 fence active
//! (public validation keeps `palw_claim_challenge_v2` at `never`) and an approved review tuple naming the contract's suite (the
//! shipped registry is empty). Nothing calls this from a block path yet; no wire id, aux table or state tag is allocated.
//!
//! **Retired (PRINCIPLES §8, 2026-10-15).** A compact relation contract is a producer-side admission object. Both registration
//! entry points below refuse unconditionally: no new compact contract is generated, accepted or used as a credit authority.
//! Only the deterministic coverage read API ([`palw_native_compact_coverage_v2`]) remains, as an inventory of which relations a
//! verifier can check directly (Freivalds/linear evaluation over operands the verifier itself reads), beside RFC-0007 item 1's
//! per-class check inventory ([`palw_native_check_inventory_v2`]: exact / direct with its sparse-error bound / refused, per
//! relation, stage and step) and the public refutation routes ([`PALW_PUBLIC_REFUTATION_ROUTES_V2`]).

use crate::config::params::Params;
use misaka_palw_challenge::PostCommitChallengePolicyV1;
use misaka_palw_challenge::composition::AdversaryBudgetV1;
use misaka_palw_kernel::claim_compact_v1::{
    CompactContractSummaryV1, CompactPlanCoverageV1, CompactRelationContractV1, register_compact_relation_contract_v1,
};

/// PRINCIPLES §8: compact relation contracts (V1 single-plan, V2 whole-class, V4 native) are never admitted again.
pub const PALW_COMPACT_CONTRACTS_RETIRED_V1: bool = true;
/// The refusal every retired compact admission entry returns.
pub const PALW_COMPACT_CONTRACT_RETIRED_REASON_V1: &str =
    "compact relation contracts are retired (PRINCIPLES §8): no new compact contract, proof binding or compact class is admitted";

/// Register a compact relation contract at `daa`: the ClaimV2 fence must be present and active, then the kernel's structural,
/// coverage, bound and approved-review checks run against the shipped (empty) registry. Refuses unconditionally (§8).
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
    if PALW_COMPACT_CONTRACTS_RETIRED_V1 {
        return Err(PALW_COMPACT_CONTRACT_RETIRED_REASON_V1.into());
    }
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

/// **RFC-0007 item 1: the per-class check inventory** from authenticated route rows — for every relation of every stage (every
/// step of a memory stage), exact recomputation, a direct check with its stated single-element (sparse) miss bound, or a listed
/// refusal; every exact boundary; and the routing / history / gather routes with their derived court bounds. A read API: it
/// grants no receipt, credit, weight, Final or activation, and it is not a computation proof (PRINCIPLES §8).
pub fn palw_native_check_inventory_v2(
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    class: misaka_palw_kernel::hash::Digest,
    compile: misaka_palw_kernel::claim_compact_v1::CompactCompileParamsV1,
) -> Result<misaka_palw_kernel::claim_compact_v1::native_inventory_v2::NativeClassInventoryV2, String> {
    let ledger = route.ledger()?;
    misaka_palw_kernel::claim_compact_v1::native_inventory_v2::native_class_inventory_v2(&ledger, class, compile)
}

/// One public refutation route (RFC-0007 item 1) and the court it reaches for each kind of class, with its named measured cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPublicRefutationRouteV2 {
    pub route: misaka_palw_kernel::claim_compact_v1::native_inventory_v2::NativeRefutationRouteV2,
    /// Native (TIR) classes: where a refutation is filed; its derived bound is the authenticated plan's worst court filing.
    pub native_court: &'static str,
    /// UNKNOWN until measured (`None`).
    pub native_measured_us: Option<u64>,
    /// Legacy PALW shape profiles: the carrier and terminal; bounded by the court close ceiling and the terminal's typed reservation.
    pub profile_court: &'static str,
    /// UNKNOWN until measured (`None`).
    pub profile_measured_us: Option<u64>,
}

/// **The three routes and their courts.** The profile side of routing and gather is served only under a class's immutable router
/// binding (aux 139) past `palw_memory_routing_court_v2` — dormant on every shipped preset; a Kimi card as declared is not
/// bindable and stays unadjudicable. No route chooses a router, and no route is a credit path.
pub const PALW_PUBLIC_REFUTATION_ROUTES_V2: [PalwPublicRefutationRouteV2; 3] = {
    use misaka_palw_kernel::claim_compact_v1::native_inventory_v2 as inv;
    [
        PalwPublicRefutationRouteV2 {
            route: inv::NativeRefutationRouteV2::Routing,
            native_court: "the plan's court for TopK/compare/select/reduce-max (element or instance recompute)",
            native_measured_us: inv::NATIVE_ROUTE_MEASURED_US_ROUTING_V2,
            profile_court: "object tag 244 RoutedStep: the step court's routing appendix under the bound router row",
            profile_measured_us: crate::palw_memory_routing_v2::PALW_ROUTE_COST_ROUTING_US_V2,
        },
        PalwPublicRefutationRouteV2 {
            route: inv::NativeRefutationRouteV2::History,
            native_court: "the plan's court for state write / history append, state boundaries and memory step continuity",
            native_measured_us: inv::NATIVE_ROUTE_MEASURED_US_HISTORY_V2,
            profile_court: "object tag 243 anchored GDN step; tag 244 GdnTransition / GdnConvWindow; the step court's KV anchors",
            profile_measured_us: crate::palw_memory_routing_v2::PALW_ROUTE_COST_HISTORY_US_V2,
        },
        PalwPublicRefutationRouteV2 {
            route: inv::NativeRefutationRouteV2::Gather,
            native_court: "the plan's court for index gather; exact snapshot retrieval re-execution",
            native_measured_us: inv::NATIVE_ROUTE_MEASURED_US_GATHER_V2,
            profile_court: "object tag 244 RoutedStep: the routed expert reader recomputed under its bound router",
            profile_measured_us: crate::palw_memory_routing_v2::PALW_ROUTE_COST_GATHER_US_V2,
        },
    ]
};

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
    if PALW_COMPACT_CONTRACTS_RETIRED_V1 {
        return Err(PALW_COMPACT_CONTRACT_RETIRED_REASON_V1.into());
    }
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
