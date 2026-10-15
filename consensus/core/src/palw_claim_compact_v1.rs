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
    coverage: &CompactPlanCoverageV1,
    adversary: &AdversaryBudgetV1,
    policy: &PostCommitChallengePolicyV1,
) -> Result<CompactContractSummaryV1, String> {
    let fence = params.palw_claim_challenge_v2.as_ref().ok_or("compact relation contracts need the ClaimV2 fence, which is absent")?;
    if !fence.activation.is_active(daa) {
        return Err("the ClaimV2 fence is dormant: compact relation contracts are not admitted".into());
    }
    register_compact_relation_contract_v1(contract, coverage, adversary, &misaka_palw_challenge::shipped_registry_v2(), policy)
}
