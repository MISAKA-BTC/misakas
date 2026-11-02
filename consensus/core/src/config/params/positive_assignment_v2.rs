//! RFC-0045 selection. Dormant on every released preset. A finite policy
//! identity does not provide native service/funding/certification readiness.
use super::{ForkActivation, Params};
use crate::{Hash, Hash64, palw_mode_v2::PalwModeV2Error};
pub use misaka_palw_positive_contract::assignment::policy::AssignmentPolicyV2;
use misaka_palw_positive_contract::{assignment::policy::assignment_ruleset_id, identity::Digest};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositiveAssignmentFenceV2 {
    pub activation: ForkActivation,
    pub policy: AssignmentPolicyV2,
}
impl PositiveAssignmentFenceV2 {
    pub fn ruleset_id(&self, fired_fork: Hash) -> Hash64 {
        let mut fired = [0; 64];
        fired[..32].copy_from_slice(&fired_fork.as_bytes());
        Hash64::from_bytes(assignment_ruleset_id(Digest(fired), self.policy.positive_policy, self.policy.id()).0)
    }
}
impl Params {
    /// Current intake selection only. Existing assignments retain their own
    /// accepted full policy preimage/clock; this accessor must not rewrite it.
    pub fn palw_positive_assignment_v2_at(&self, daa: u64) -> Option<&PositiveAssignmentFenceV2> {
        let fence = self.palw_positive_assignment_v2.as_ref()?;
        let positive = self.palw_positive_court_v2_at(daa)?;
        (fence.activation != ForkActivation::never()
            && fence.activation.is_active(daa)
            && fence.policy.validate_shape().is_ok()
            && fence.policy.positive_policy.0 == positive.policy.root().as_bytes())
        .then_some(fence)
    }
    pub fn validate_palw_positive_assignment_v2(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_positive_assignment_v2.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
            return Ok(());
        };
        fence.policy.validate_shape().map_err(|_| PalwModeV2Error::Invalid("assignment2 unsupported/non-finite policy"))?;
        let Some(positive) = self.palw_positive_court_v2.as_ref() else {
            return Err(PalwModeV2Error::Invalid("assignment2 requires the exact Positive policy"));
        };
        if positive.activation == ForkActivation::never()
            || positive.activation.daa_score() > fence.activation.daa_score()
            || fence.policy.positive_policy.0 != positive.policy.root().as_bytes()
        {
            return Err(PalwModeV2Error::Invalid("assignment2 Positive scope/activation mismatch"));
        }
        Err(PalwModeV2Error::Invalid("assignment2 native funding/roles/capacity/history/P readiness incomplete; no public activation"))
    }
}
