//! Shared bounded Positive prerequisites AND existing use-specific entitlement.
//! The view must be derived from the current executed state. Public DTOs and
//! saved `PositivePassed` flags cannot implement the missing native adapter.

use misaka_palw_positive_contract::{
    identity::{CaseId, ChainScope, ClaimId, Digest, PlanId, ResultId},
    messages::ClaimStatus,
    ports::{CreditGate, CreditUse},
};
use std::collections::BTreeMap;

pub mod consumers;

pub const ALL_CREDIT_USES: [CreditUse; 8] = [
    CreditUse::Reward,
    CreditUse::WorkReceipt,
    CreditUse::DagWeight,
    CreditUse::RuleE,
    CreditUse::Settlement,
    CreditUse::PrefixReuse,
    CreditUse::Dependency,
    CreditUse::WorkerPayment,
];

/// Native admission must bind the *new signed result*, not a legacy claim ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreditBinding {
    pub scope: ChainScope,
    pub claim: ClaimId,
    pub case: CaseId,
    pub result: ResultId,
    pub output: Digest,
    pub dependencies: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateCApproval {
    Pending,
    Rejected,
    /// Resolved from drawn-verifier approval in rooted state. The existing
    /// honest-fraction premise still applies; Positive does not replace it.
    Approved(CreditBinding),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FraudState {
    Clear,
    AcceptedPending,
    Refuted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EconomicReadiness {
    Held,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootedCreditFacts {
    pub binding: CreditBinding,
    /// A1-derived plan pinned at admission, not a Panel-selected subplan.
    pub plan: PlanId,
    pub mandatory_relations: u64,
    pub status: ClaimStatus,
    pub gate_c: GateCApproval,
    pub fraud: FraudState,
    pub maturity_and_liability: EconomicReadiness,
    pub carrier_and_funding: EconomicReadiness,
}

/// Returned only by the view's A1/A4 adapter after local verification/replay.
/// No deserializer is provided here. This is not a native authorization token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckedCoverage {
    pub binding: CreditBinding,
    pub plan: PlanId,
    pub mandatory_relations: u64,
    pub checked_relations: u64,
    pub status: ClaimStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreditDenial {
    MissingNativeResultBinding,
    MissingClaim(ClaimId),
    Binding(ClaimId),
    Scope(ClaimId),
    PositiveNotPassed(ClaimId),
    MissingLocalVerification(ClaimId),
    IncompleteCoverage(ClaimId),
    MissingDependencyBinding(ClaimId),
    NonCanonicalDependencies(ClaimId),
    DependencyCycle(ClaimId),
    GateC(ClaimId),
    Fraud(ClaimId),
    MaturityOrLiability(ClaimId),
    CarrierOrFunding(ClaimId),
    Entitlement { claim: ClaimId, usage: CreditUse },
    Budget,
}

/// Trusted, immutable current-state adapter. In particular, the dependency list
/// must be authenticated against the result's dependency commitment and resolve
/// the expected export/state identity, position, model, tokenizer and layout.
/// An empty list must be authenticated too. No method takes an approval boolean.
pub trait RootedCreditView {
    /// Default scope for private single-anchor views. A native view's branch is
    /// a fixed admission ancestry anchor, never its changing current tip.
    fn scope(&self) -> ChainScope;
    fn claim(&self, claim: ClaimId) -> Result<RootedCreditFacts, CreditDenial>;

    /// Validate this claim's own immutable scope against the CURRENT rooted
    /// view. Native implementations must resolve its admitted Result/Case and
    /// accepted origin, policy/ruleset and actual lineage membership. Ancestors
    /// may have distinct fixed anchors on that lineage. A supplied anchor list
    /// or boolean is not native authority. The default preserves the strict
    /// single-anchor private contract and rejects every scope mismatch.
    fn validate_scope(&self, binding: &CreditBinding) -> Result<(), CreditDenial> {
        if binding.scope == self.scope() { Ok(()) } else { Err(CreditDenial::Scope(binding.claim)) }
    }

    /// Resolve the EXISTING entitlement for this exact binding and use from
    /// the current view's actual origin, paid/mining contract and conditions.
    /// PositivePassed, drawn approval and funding readiness are prerequisites;
    /// they do not create reward, weight, receipt, capital or reuse rights.
    /// Ancestors are queried for Dependency, not the root's requested use.
    /// A missing adapter refuses every use. Private fixture All/sets cannot
    /// substitute for native origin/contract resolution.
    fn authorize_entitlement(&self, binding: &CreditBinding, usage: CreditUse) -> Result<(), CreditDenial> {
        Err(CreditDenial::Entitlement { claim: binding.claim, usage })
    }

    fn checked_coverage(&self, claim: ClaimId) -> Result<CheckedCoverage, CreditDenial> {
        Err(CreditDenial::MissingLocalVerification(claim))
    }

    /// Canonically sorted, unique parent bindings. Never return an empty slice
    /// as a fallback for an unresolved commitment or unavailable parent state.
    fn checked_dependencies(&self, claim: ClaimId) -> Result<&[CreditBinding], CreditDenial> {
        Err(CreditDenial::MissingDependencyBinding(claim))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreditLimits {
    pub max_claims: u64,
    pub max_edges: u64,
}

pub struct PositiveCreditGate<'a, V: RootedCreditView> {
    view: &'a V,
    limits: CreditLimits,
}

/// Borrows the current view, preventing mutation through its owner while used.
/// Consumers must authorize again after rollback, fraud admission or reorg.
/// It has no serializer, public constructor or authority over native money.
pub struct CreditAuthorization<'a, V: RootedCreditView> {
    _view: &'a V,
    binding: CreditBinding,
    usage: CreditUse,
}
impl<V: RootedCreditView> CreditAuthorization<'_, V> {
    pub fn binding(&self) -> CreditBinding {
        self.binding
    }
    pub fn usage(&self) -> CreditUse {
        self.usage
    }
}

impl<'a, V: RootedCreditView> PositiveCreditGate<'a, V> {
    pub fn new(view: &'a V, limits: CreditLimits) -> Result<Self, CreditDenial> {
        if limits.max_claims == 0 || limits.max_edges == 0 {
            return Err(CreditDenial::Budget);
        }
        Ok(Self { view, limits })
    }

    /// All sites share Positive prerequisites and the all-ancestor rule, while
    /// retaining the root's existing use-specific entitlement. Each ancestor
    /// needs its own existing Dependency entitlement. No child-ready flag
    /// survives a changed parent view or underlying contract entitlement.
    fn check(&self, root: ClaimId, usage: CreditUse) -> Result<CreditBinding, CreditDenial> {
        #[derive(Clone, Copy)]
        enum Visit {
            Enter(ClaimId, Option<CreditBinding>),
            Exit(ClaimId),
        }
        let mut stack = vec![Visit::Enter(root, None)];
        // false = visiting, true = completely checked. A diamond is legal.
        let mut seen = BTreeMap::<ClaimId, bool>::new();
        let mut edges = 0u64;
        let mut root_binding = None;
        while let Some(visit) = stack.pop() {
            let (id, expected) = match visit {
                Visit::Exit(id) => {
                    seen.insert(id, true);
                    continue;
                }
                Visit::Enter(id, expected) => (id, expected),
            };
            let facts = self.view.claim(id)?;
            if facts.binding.claim != id || expected.is_some_and(|binding| binding != facts.binding) {
                return Err(CreditDenial::Binding(id));
            }
            self.view.validate_scope(&facts.binding)?;
            match seen.get(&id) {
                Some(false) => return Err(CreditDenial::DependencyCycle(id)),
                Some(true) => continue,
                None => {}
            }
            if seen.len() as u64 >= self.limits.max_claims {
                return Err(CreditDenial::Budget);
            }
            if facts.status != ClaimStatus::PositivePassed {
                return Err(CreditDenial::PositiveNotPassed(id));
            }
            let coverage = self.view.checked_coverage(id)?;
            if coverage.binding != facts.binding || coverage.plan != facts.plan {
                return Err(CreditDenial::Binding(id));
            }
            if coverage.status != ClaimStatus::PositivePassed
                || coverage.mandatory_relations == 0
                || coverage.mandatory_relations != facts.mandatory_relations
                || coverage.checked_relations != coverage.mandatory_relations
            {
                return Err(CreditDenial::IncompleteCoverage(id));
            }
            if facts.gate_c != GateCApproval::Approved(facts.binding) {
                return Err(CreditDenial::GateC(id));
            }
            if facts.fraud != FraudState::Clear {
                return Err(CreditDenial::Fraud(id));
            }
            if facts.maturity_and_liability != EconomicReadiness::Ready {
                return Err(CreditDenial::MaturityOrLiability(id));
            }
            if facts.carrier_and_funding != EconomicReadiness::Ready {
                return Err(CreditDenial::CarrierOrFunding(id));
            }
            self.view.authorize_entitlement(&facts.binding, if id == root { usage } else { CreditUse::Dependency })?;
            let dependencies = self.view.checked_dependencies(id)?;
            edges = edges.checked_add(dependencies.len() as u64).ok_or(CreditDenial::Budget)?;
            if edges > self.limits.max_edges {
                return Err(CreditDenial::Budget);
            }
            if dependencies.windows(2).any(|pair| pair[0].claim >= pair[1].claim) {
                return Err(CreditDenial::NonCanonicalDependencies(id));
            }
            if id == root {
                root_binding = Some(facts.binding);
            }
            seen.insert(id, false);
            stack.push(Visit::Exit(id));
            for parent in dependencies.iter().rev() {
                stack.push(Visit::Enter(parent.claim, Some(*parent)));
            }
        }
        root_binding.ok_or(CreditDenial::MissingClaim(root))
    }

    pub fn credited_weight(&self, claim: ClaimId, weight: u128) -> u128 {
        if self.authorize(&claim, CreditUse::DagWeight).is_ok() { weight } else { 0 }
    }
}

impl<'a, V: RootedCreditView> CreditGate for PositiveCreditGate<'a, V> {
    type RootedClaim = ClaimId;
    type Authorization = CreditAuthorization<'a, V>;
    type Denial = CreditDenial;
    fn authorize(&self, claim: &ClaimId, usage: CreditUse) -> Result<Self::Authorization, CreditDenial> {
        // An exhaustive match makes new A0 consumers a deliberate integration.
        match usage {
            CreditUse::Reward
            | CreditUse::WorkReceipt
            | CreditUse::DagWeight
            | CreditUse::RuleE
            | CreditUse::Settlement
            | CreditUse::PrefixReuse
            | CreditUse::Dependency
            | CreditUse::WorkerPayment => {}
        }
        Ok(CreditAuthorization { _view: self.view, binding: self.check(*claim, usage)?, usage })
    }
}

/// The actual native adapter remains unassigned. Never borrow a legacy
/// certification using a Miner-chosen ID or promote a prototype local Pass.
pub struct UnavailableNativeCredit;
impl CreditGate for UnavailableNativeCredit {
    type RootedClaim = ClaimId;
    type Authorization = std::convert::Infallible;
    type Denial = CreditDenial;
    fn authorize(&self, _: &ClaimId, _: CreditUse) -> Result<Self::Authorization, CreditDenial> {
        Err(CreditDenial::MissingNativeResultBinding)
    }
}

#[cfg(test)]
mod tests;
