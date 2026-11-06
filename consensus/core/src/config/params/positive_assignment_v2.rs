//! RFC-0045 selection. Dormant on every released preset. A finite policy
//! identity does not provide native service/funding/certification readiness.
use super::{ForkActivation, Params};
use crate::{Hash, Hash64, palw_mode_v2::PalwModeV2Error};
pub use misaka_palw_positive_contract::assignment::policy::AssignmentPolicyV2;
use misaka_palw_positive_contract::{identity::Digest, native::ruleset_id};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositiveAssignmentFenceV2 {
    pub activation: ForkActivation,
    pub policy: AssignmentPolicyV2,
    /// Complete finite native price/physical preimages. None is a probe only,
    /// never an executable funding or capacity promise.
    pub native_preimages: Option<AssignmentNativePreimagesV2>,
    /// Explicit capacity preimage for new source/prover commitments.
    pub service_capacity: Option<AssignmentServiceCapacityV2>,
}
/// Shared capacity for outstanding source and prover duties. Units match
/// the native signed consents. Commitments remain occupied while their
/// authoritative Assignment or joined funding row is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentServiceCapacityV2 {
    pub source_work: u64,
    pub source_bytes: u64,
    pub source_retained: u64,
    pub prover_work: u64,
    pub prover_bytes: u64,
    pub prover_retained: u64,
}

impl AssignmentServiceCapacityV2 {
    pub(crate) fn dimensions_v2(self) -> [u64; 6] {
        [self.source_work, self.source_bytes, self.source_retained, self.prover_work, self.prover_bytes, self.prover_retained]
    }

    pub(crate) fn validate_v2(self) -> Result<(), PalwModeV2Error> {
        if self.dimensions_v2().into_iter().any(|n| n == 0 || n == u64::MAX) {
            return Err(PalwModeV2Error::Invalid("assignment2 service capacity must be finite and nonzero"));
        }
        Ok(())
    }
}

/// Immutable, versioned native funding inputs. Actual derived usage, leases,
/// source/Case/P owners and current coins remain separately required.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AssignmentNativePreimagesV2 {
    pub assignment_fees: crate::palw_positive_assignment_funds_v2::AssignmentFeeScheduleV2,
    pub physical: crate::palw_positive_assignment_funds_v2::AssignmentPhysicalProfileV2,
    pub source_policy: crate::palw_positive_assignment_resources_v2::AssignmentSourcePolicyV2,
    pub case_fees: crate::palw_positive_assignment_resources_v2::AssignmentCaseFeeScheduleV2,
    pub cancellation: crate::palw_positive_assignment_resources_v2::AssignmentCancellationPolicyV2,
}
impl AssignmentNativePreimagesV2 {
    pub fn root(&self) -> Hash64 {
        Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka/positive/assignment-native-preimages/v2/r1")
                .hash(&borsh::to_vec(self).expect("fixed native preimages"))
                .as_bytes()
                .try_into()
                .expect("64-byte digest"),
        )
    }
    pub fn validate(&self, policy: &AssignmentPolicyV2) -> Result<(), PalwModeV2Error> {
        self.assignment_fees.validate().map_err(|_| PalwModeV2Error::Invalid("assignment fee preimage is invalid"))?;
        self.physical.validate().map_err(|_| PalwModeV2Error::Invalid("assignment physical preimage is invalid"))?;
        self.source_policy.validate().map_err(|_| PalwModeV2Error::Invalid("assignment source policy preimage is invalid"))?;
        self.case_fees.validate().map_err(|_| PalwModeV2Error::Invalid("assignment C fee preimage is invalid"))?;
        self.cancellation.validate().map_err(|_| PalwModeV2Error::Invalid("assignment cancellation preimage is invalid"))?;
        if self.source_policy.id() != policy.source_policy
            || self.case_fees.id() != policy.case_fee_schedule
            || self.cancellation.id() != policy.cancellation_policy
            || self.assignment_fees.id() != policy.assignment_fee_schedule
            || self.assignment_fees.reserve_id() != policy.reserve_schedule
            || self.physical.id() != policy.physical_service_profile
        {
            return Err(PalwModeV2Error::Invalid("assignment native preimages differ from adopted policy"));
        }
        Ok(())
    }
}
impl PositiveAssignmentFenceV2 {
    /// Use the unchanged r1 recipe. The actual fired-rule resolver below
    /// commits the complete Assignment policy through its fork input.
    pub fn ruleset_id(&self, fired_fork: Hash) -> Hash64 {
        let mut fired = [0; 64];
        fired[..32].copy_from_slice(&fired_fork.as_bytes());
        Hash64::from_bytes(ruleset_id(Digest(fired), self.policy.positive_policy).0)
    }
}
/// Read-only rule derivation, not accepted-prefix/native admission authority.
/// This is deliberately not the advertised ForkIdV1 (height list only), nor
/// the genesis-only handshake identity. Future companion policies are removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveAssignmentRuleScopeV2 {
    fired_rules: Hash,
    ruleset: Hash64,
    positive_policy: Digest,
    assignment_policy: Digest,
}
impl PositiveAssignmentRuleScopeV2 {
    pub(crate) fn from_retained_data(fired: Digest, policy: &AssignmentPolicyV2) -> Option<Self> {
        let fired_rules = Hash::from_bytes(fired.0[..32].try_into().ok()?);
        if fired.0[32..].iter().any(|byte| *byte != 0) {
            return None;
        }
        Some(Self {
            fired_rules,
            ruleset: Hash64::from_bytes(ruleset_id(fired, policy.positive_policy).0),
            positive_policy: policy.positive_policy,
            assignment_policy: policy.id(),
        })
    }
    pub fn fired_rules(&self) -> Hash {
        self.fired_rules
    }
    pub fn fired_rules_digest(&self) -> Digest {
        let mut bytes = [0; 64];
        bytes[..32].copy_from_slice(&self.fired_rules.as_bytes());
        Digest(bytes)
    }
    pub fn ruleset(&self) -> Hash64 {
        self.ruleset
    }
    pub fn positive_policy(&self) -> Digest {
        self.positive_policy
    }
    pub fn assignment_policy(&self) -> Digest {
        self.assignment_policy
    }
}

impl Params {
    /// Resolve from the actual native admission clock. All already-fired
    /// heights and their full companion policies participate; scheduled
    /// future heights/policies do not. Persist this original scope for later
    /// Case/restore rather than recomputing it under a future fork.
    pub fn palw_positive_assignment_rule_scope_v2_at(&self, daa: u64) -> Option<PositiveAssignmentRuleScopeV2> {
        let fence = self.palw_positive_assignment_v2_at(daa)?;
        let mut fired = self.clone();
        fired.for_each_fence(&mut |at| {
            if *at > daa {
                *at = u64::MAX;
            }
        });
        fired.normalize_values_a_scheduled_fence_drags_with_it();
        let fired_rules = fired.consensus_params_id();
        Some(PositiveAssignmentRuleScopeV2 {
            fired_rules,
            ruleset: fence.ruleset_id(fired_rules),
            positive_policy: fence.policy.positive_policy,
            assignment_policy: fence.policy.id(),
        })
    }

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
        if let Some(preimages) = &fence.native_preimages {
            preimages.validate(&fence.policy)?;
        }
        if let Some(capacity) = fence.service_capacity {
            capacity.validate_v2()?;
        }
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
