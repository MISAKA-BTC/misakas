//! Concrete A1 checked-terminal bridge for A5. This belongs to a node crate so
//! the consensus/accounting leaf never depends on the node executor.
//!
//! The terminal is retained as the actual non-deserializable A1 type. A saved
//! status/descriptor or a caller-provided coverage boolean cannot fill the map.
//! A4's current claim verdict and the rooted economics/dependencies still come
//! from the borrowed view; a local match is not a native-credit authorization.

use misaka_palw_positive_accounting::credit::{CheckedCoverage, CreditBinding, CreditDenial, RootedCreditFacts, RootedCreditView};
use misaka_palw_positive_contract::{
    identity::{ChainScope, ClaimId},
    messages::{AuthenticatedResult, CheckProfile, ClaimStatus, coverage_plan_id},
    ports::CreditUse,
    tir::{exact_derivation_id, greedy_selector_id},
};
use misaka_palw_tir_exec::node::{positive_check::CheckOutcome, positive_contract::CheckedTerminal};
use std::collections::BTreeMap;

struct RecordedTerminal {
    binding: CreditBinding,
    terminal: CheckedTerminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalEffect {
    Recorded,
    AlreadyRecorded,
}

/// Current immutable economic/admission view plus A1 results locally obtained
/// under that view. There is no Deserialize implementation or public map setter.
pub struct TirCreditView<'a, S: RootedCreditView> {
    rooted: &'a S,
    terminals: BTreeMap<ClaimId, RecordedTerminal>,
    max_terminals: u64,
}

impl<'a, S: RootedCreditView> TirCreditView<'a, S> {
    pub fn new(rooted: &'a S, max_terminals: u64) -> Result<Self, CreditDenial> {
        if max_terminals == 0 {
            return Err(CreditDenial::Budget);
        }
        Ok(Self { rooted, terminals: BTreeMap::new(), max_terminals })
    }

    /// Result authentication and A1 completion have separate provenance. The
    /// current admitted row must bind both to the exact new signed result. A4
    /// retains ownership of the claim transition; this method writes no verdict.
    pub fn record_checked(
        &mut self,
        authenticated: &AuthenticatedResult<'_>,
        terminal: CheckedTerminal,
    ) -> Result<TerminalEffect, CreditDenial> {
        let result = authenticated.commit();
        let claim = result.claim_id();
        let binding = CreditBinding {
            scope: result.scope,
            claim,
            case: result.case_id(),
            result: result.id(),
            output: result.output,
            dependencies: result.dependencies,
        };
        let facts = self.rooted.claim(claim)?;
        let plan = terminal.descriptor();
        let canonical_plan = coverage_plan_id(result, CheckProfile::ExactStreamV1, exact_derivation_id(), plan.mandatory_relations)
            .map_err(|_| CreditDenial::IncompleteCoverage(claim))?;
        self.rooted.validate_scope(&binding)?;
        if facts.binding != binding
            || terminal.result() != binding.result
            || plan.result != binding.result
            || plan.scope != binding.scope
            || plan.plan != facts.plan
            || plan.plan != canonical_plan
            || result.selector != greedy_selector_id()
            || plan.mandatory_relations != facts.mandatory_relations
        {
            return Err(CreditDenial::Binding(claim));
        }
        if plan.profile != CheckProfile::ExactStreamV1 || plan.mandatory_relations == 0 {
            return Err(CreditDenial::IncompleteCoverage(claim));
        }
        if let Some(previous) = self.terminals.get(&claim) {
            // Replaying or replacing a witness cannot create another source of
            // credit. A contradictory local terminal must be investigated.
            return if previous.binding == binding
                && previous.terminal.descriptor() == plan
                && previous.terminal.outcome() == terminal.outcome()
            {
                Ok(TerminalEffect::AlreadyRecorded)
            } else {
                Err(CreditDenial::Binding(claim))
            };
        }
        if self.terminals.len() as u64 >= self.max_terminals {
            return Err(CreditDenial::Budget);
        }
        self.terminals.insert(claim, RecordedTerminal { binding, terminal });
        Ok(TerminalEffect::Recorded)
    }

    pub fn recorded_terminals(&self) -> u64 {
        self.terminals.len() as u64
    }
}

impl<S: RootedCreditView> RootedCreditView for TirCreditView<'_, S> {
    fn scope(&self) -> ChainScope {
        self.rooted.scope()
    }

    fn claim(&self, claim: ClaimId) -> Result<RootedCreditFacts, CreditDenial> {
        self.rooted.claim(claim)
    }

    fn validate_scope(&self, binding: &CreditBinding) -> Result<(), CreditDenial> {
        self.rooted.validate_scope(binding)
    }

    fn authorize_entitlement(&self, binding: &CreditBinding, usage: CreditUse) -> Result<(), CreditDenial> {
        self.rooted.authorize_entitlement(binding, usage)
    }

    fn checked_coverage(&self, claim: ClaimId) -> Result<CheckedCoverage, CreditDenial> {
        let checked = self.terminals.get(&claim).ok_or(CreditDenial::MissingLocalVerification(claim))?;
        let facts = self.rooted.claim(claim)?;
        self.rooted.validate_scope(&checked.binding)?;
        if checked.binding != facts.binding {
            return Err(CreditDenial::Binding(claim));
        }
        if checked.terminal.outcome() != CheckOutcome::Matched {
            return Err(CreditDenial::PositiveNotPassed(claim));
        }
        let descriptor = checked.terminal.descriptor();
        Ok(CheckedCoverage {
            binding: checked.binding,
            plan: descriptor.plan,
            mandatory_relations: descriptor.mandatory_relations,
            // A1::finish only returns Matched after the complete derived plan.
            checked_relations: descriptor.mandatory_relations,
            status: ClaimStatus::PositivePassed,
        })
    }

    fn checked_dependencies(&self, claim: ClaimId) -> Result<&[CreditBinding], CreditDenial> {
        self.rooted.checked_dependencies(claim)
    }
}

#[cfg(test)]
#[path = "tir_tests.rs"]
mod tests;
