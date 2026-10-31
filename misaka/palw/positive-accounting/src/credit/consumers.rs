//! New-claim consumer entry points over the single current credit predicate.
//! These methods do not modify legacy claims, issue coins, authorize a carrier,
//! or manufacture a missing native result binding. Existing native readers must
//! route their Positive-result arm here after A0 assigns that native binding.

use misaka_palw_positive_contract::ports::{CreditGate, CreditUse};

pub struct CreditConsumers<'a, G: CreditGate> {
    gate: &'a G,
}

impl<'a, G: CreditGate> CreditConsumers<'a, G> {
    pub fn new(gate: &'a G) -> Self {
        Self { gate }
    }

    /// Existing primary issuance/coinbase/model/PWU entitlement. Worker success
    /// payment cannot use or satisfy this purpose. Native readers must check
    /// the authorization's purpose at their final use, not accept another use.
    pub fn inference_reward(&self, claim: &G::RootedClaim) -> Result<G::Authorization, G::Denial> {
        self.gate.authorize(claim, CreditUse::Reward)
    }

    /// Existing contract-authorized worker success payment only. This does not
    /// mint primary issuance. Earned source-service fees and safe refunds use
    /// their actual receipt/liability/funding/once-only money predicates instead
    /// and must not stay locked merely because Positive/P credit is unavailable.
    pub fn worker_payment(&self, claim: &G::RootedClaim) -> Result<G::Authorization, G::Denial> {
        self.gate.authorize(claim, CreditUse::WorkerPayment)
    }

    /// The same entry is used at both mint and spend; an old mint flag does not
    /// authorize a spend after the current claim or one of its ancestors fails.
    pub fn work_receipt(&self, claim: &G::RootedClaim) -> Result<G::Authorization, G::Denial> {
        self.gate.authorize(claim, CreditUse::WorkReceipt)
    }

    /// Applies to live/provisional/safe weight and its re-derivation. The amount
    /// is the caller's canonical work value; a denial always contributes zero.
    pub fn dag_weight(&self, claim: &G::RootedClaim, canonical_weight: u128) -> u128 {
        if self.gate.authorize(claim, CreditUse::DagWeight).is_ok() { canonical_weight } else { 0 }
    }

    pub fn rule_e_capital(&self, claim: &G::RootedClaim, canonical_capital: u128) -> u128 {
        if self.gate.authorize(claim, CreditUse::RuleE).is_ok() { canonical_capital } else { 0 }
    }

    /// Safe frontier, settlement depth, settled anchor and EVM settlement share
    /// this decision. Their existing source/depth rules still apply afterwards.
    pub fn settlement(&self, claim: &G::RootedClaim) -> Result<G::Authorization, G::Denial> {
        self.gate.authorize(claim, CreditUse::Settlement)
    }

    pub fn prefix_reuse(&self, claim: &G::RootedClaim) -> Result<G::Authorization, G::Denial> {
        self.gate.authorize(claim, CreditUse::PrefixReuse)
    }

    pub fn dependent_claim(&self, claim: &G::RootedClaim) -> Result<G::Authorization, G::Denial> {
        self.gate.authorize(claim, CreditUse::Dependency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credit::{CreditDenial, UnavailableNativeCredit};
    use misaka_palw_positive_contract::identity::{ClaimId, Digest};

    #[test]
    fn unassigned_native_binding_cannot_reach_any_positive_consumer() {
        let consumers = CreditConsumers::new(&UnavailableNativeCredit);
        let claim = ClaimId(Digest([7; 64]));
        let denied = Err(CreditDenial::MissingNativeResultBinding);
        assert_eq!(consumers.inference_reward(&claim), denied);
        assert_eq!(consumers.worker_payment(&claim), denied);
        assert_eq!(consumers.work_receipt(&claim), denied);
        assert_eq!(consumers.dag_weight(&claim, u128::MAX), 0);
        assert_eq!(consumers.rule_e_capital(&claim, u128::MAX), 0);
        assert_eq!(consumers.settlement(&claim), denied);
        assert_eq!(consumers.prefix_reuse(&claim), denied);
        assert_eq!(consumers.dependent_claim(&claim), denied);
    }
}
