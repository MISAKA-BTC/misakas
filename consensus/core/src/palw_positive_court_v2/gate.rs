//! Positive R2 admission and the one native-credit barrier.
//!
//! This module does not retrofit a Panel trace onto an old Miner signature.
//! In particular, a new result's freely chosen `claim` field cannot borrow a
//! certification row belonging to an old claim. No native result binding exists
//! yet, so the barrier has deliberately no success/capability constructor.

use super::{ClaimStatusV2, ExactCaseV2, MinerResultV2, WitnessStatusV2};
use crate::Hash64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimFormV2 {
    LegacyTraceV1,
    MinerResultV2,
}

/// Resolved by an admission adapter from its ruleset. It is never a field of a
/// Miner result and must not be selected from a producer's optional feature bit.
/// This isolated prototype does not install it into a public network preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmissionScopeV2 {
    pub network: Hash64,
    pub branch: Hash64,
    pub ruleset: Hash64,
    /// Rooted registered class identity, including its artifact/weight root.
    pub registered_class_id: Hash64,
    pub required_form: ClaimFormV2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionDenialV2 {
    WrongClaimForm,
    WrongNetwork,
    WrongBranch,
    WrongRuleset,
    WrongClass,
}

impl AdmissionScopeV2 {
    /// A Positive-governed admission cannot downgrade to the old trace claim.
    /// Conversely, the new object cannot silently acquire old-wire semantics.
    pub fn check_form(&self, supplied: ClaimFormV2) -> Result<(), AdmissionDenialV2> {
        if supplied == self.required_form { Ok(()) } else { Err(AdmissionDenialV2::WrongClaimForm) }
    }

    pub fn check_result(&self, result: &MinerResultV2) -> Result<(), AdmissionDenialV2> {
        self.check_form(ClaimFormV2::MinerResultV2)?;
        if result.network != self.network {
            return Err(AdmissionDenialV2::WrongNetwork);
        }
        if result.branch != self.branch {
            return Err(AdmissionDenialV2::WrongBranch);
        }
        if result.ruleset != self.ruleset {
            return Err(AdmissionDenialV2::WrongRuleset);
        }
        if result.job.shape_profile_id != self.registered_class_id {
            return Err(AdmissionDenialV2::WrongClass);
        }
        Ok(())
    }
}

/// Enumerating the consumers here makes the remaining native integration
/// explicit. None receives a permissive default for a missing Positive row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeCreditConsumerV2 {
    InferenceReward,
    WorkReceipt,
    DagWeight,
    RuleEParticipation,
    Settlement,
    PrefixReuse,
    DependentClaim,
}
pub const NATIVE_CREDIT_CONSUMERS_V2: [NativeCreditConsumerV2; 7] = [
    NativeCreditConsumerV2::InferenceReward,
    NativeCreditConsumerV2::WorkReceipt,
    NativeCreditConsumerV2::DagWeight,
    NativeCreditConsumerV2::RuleEParticipation,
    NativeCreditConsumerV2::Settlement,
    NativeCreditConsumerV2::PrefixReuse,
    NativeCreditConsumerV2::DependentClaim,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeCreditDenialV2 {
    MissingPositiveCase,
    Admission(AdmissionDenialV2),
    DifferentCurrentBranch,
    DifferentCurrentRuleset,
    PositiveNotPassed,
    IncompleteCoverage,
    /// A rooted native row must bind the NEW signed result, role identities,
    /// ruleset, branch and output before any Gate C lookup is meaningful.
    /// A copied old claim ID, local `Passed`, or supplied boolean cannot do so.
    MissingNativeResultBinding,
}

/// Shared fail-closed barrier for private integrations. The return type cannot
/// be mistaken for a granted credit capability. The existing native reward,
/// DAG, receipt and settlement code has no new-claim wire arm to call yet;
/// adding one must consume a rooted result binding and preserve this barrier.
///
/// There is intentionally no Gate C boolean argument and no lookup by the
/// untrusted `MinerResultV2.claim`. After a real native binding is implemented,
/// every consumer must additionally read current Gate C, dependencies, pending
/// fraud, maturity/liability and carrier/funding readiness from rooted state.
pub fn positive_native_credit_v2(
    case: Option<&ExactCaseV2>,
    admission: &AdmissionScopeV2,
    current_branch: Hash64,
    current_ruleset: Hash64,
    _consumer: NativeCreditConsumerV2,
) -> Result<std::convert::Infallible, NativeCreditDenialV2> {
    let case = case.ok_or(NativeCreditDenialV2::MissingPositiveCase)?;
    admission.check_result(case.result()).map_err(NativeCreditDenialV2::Admission)?;
    if current_branch != admission.branch {
        return Err(NativeCreditDenialV2::DifferentCurrentBranch);
    }
    if current_ruleset != admission.ruleset {
        return Err(NativeCreditDenialV2::DifferentCurrentRuleset);
    }
    let (claim, witness) = case.statuses();
    if claim != ClaimStatusV2::Passed {
        return Err(NativeCreditDenialV2::PositiveNotPassed);
    }
    if witness != WitnessStatusV2::Complete || case.cursor() != case.witness().binding.step_leaf_count {
        return Err(NativeCreditDenialV2::IncompleteCoverage);
    }
    Err(NativeCreditDenialV2::MissingNativeResultBinding)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope(form: ClaimFormV2) -> AdmissionScopeV2 {
        AdmissionScopeV2 {
            network: Hash64::from_u64_word(1),
            branch: Hash64::from_u64_word(2),
            ruleset: Hash64::from_u64_word(3),
            registered_class_id: Hash64::from_u64_word(4),
            required_form: form,
        }
    }

    #[test]
    fn claim_form_is_enforced_by_admission_not_a_miner_fallback() {
        assert_eq!(scope(ClaimFormV2::MinerResultV2).check_form(ClaimFormV2::LegacyTraceV1), Err(AdmissionDenialV2::WrongClaimForm));
        assert_eq!(scope(ClaimFormV2::LegacyTraceV1).check_form(ClaimFormV2::MinerResultV2), Err(AdmissionDenialV2::WrongClaimForm));
        assert_eq!(scope(ClaimFormV2::LegacyTraceV1).check_form(ClaimFormV2::LegacyTraceV1), Ok(()));
        assert_eq!(scope(ClaimFormV2::MinerResultV2).check_form(ClaimFormV2::MinerResultV2), Ok(()));
    }

    #[test]
    fn absent_positive_completion_denies_every_credit_consumer() {
        let admission = scope(ClaimFormV2::MinerResultV2);
        for consumer in NATIVE_CREDIT_CONSUMERS_V2 {
            assert_eq!(
                positive_native_credit_v2(None, &admission, admission.branch, admission.ruleset, consumer),
                Err(NativeCreditDenialV2::MissingPositiveCase)
            );
        }
    }
}
