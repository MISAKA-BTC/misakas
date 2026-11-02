//! RFC-0046 metadata intake selection, separate from the original Assignment
//! contract. No published defaults, migration, custody grant or activation.
use super::{ForkActivation, Params};
use crate::{Hash, palw_mode_v2::PalwModeV2Error};
use misaka_palw_positive_contract::{
    assignment::funding_relay_policy::{FundingRelayFeeScheduleV2, FundingRelayPhysicalProfileV2, FundingRelayPolicyV2},
    identity::Digest,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositiveFundingRelayFenceV2 {
    pub activation: ForkActivation,
    pub policy: FundingRelayPolicyV2,
    pub fees: FundingRelayFeeScheduleV2,
    pub physical: FundingRelayPhysicalProfileV2,
}
impl PositiveFundingRelayFenceV2 {
    pub fn validate_preimages(&self) -> Result<(), PalwModeV2Error> {
        self.policy.validate().map_err(|_| PalwModeV2Error::Invalid("relay2 nonfinite policy"))?;
        self.fees.validate().map_err(|_| PalwModeV2Error::Invalid("relay2 nonfinite fees"))?;
        self.physical.validate().map_err(|_| PalwModeV2Error::Invalid("relay2 nonfinite physical profile"))?;
        if self.policy.metadata_fee_schedule != self.fees.id() || self.policy.physical_custody_profile != self.physical.id() {
            return Err(PalwModeV2Error::Invalid("relay2 fee/physical preimage identity mismatch"));
        }
        Ok(())
    }
}

/// Native Params-derived scope DATA. Does not establish actual R acceptance,
/// original Assignment profile, lineage, a live funding output or custody.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveFundingRelayRuleScopeV2 {
    fired_rules: Hash,
    policy: Digest,
    ruleset: Digest,
}
impl PositiveFundingRelayRuleScopeV2 {
    pub fn fired_rules(&self) -> Hash {
        self.fired_rules
    }
    pub fn policy(&self) -> Digest {
        self.policy
    }
    pub fn ruleset(&self) -> Digest {
        self.ruleset
    }
}
impl Params {
    /// Metadata intake only. Existing contracts must keep their original
    /// authenticated Cooperative/RequiresR profile across all later forks.
    pub fn palw_positive_funding_relay_v2_at(&self, daa: u64) -> Option<&PositiveFundingRelayFenceV2> {
        let fence = self.palw_positive_funding_relay_v2.as_ref()?;
        self.palw_positive_assignment_v2_at(daa)?;
        (fence.activation != ForkActivation::never() && fence.activation.is_active(daa) && fence.validate_preimages().is_ok())
            .then_some(fence)
    }
    pub fn palw_positive_funding_relay_rule_scope_v2_at(&self, daa: u64) -> Option<PositiveFundingRelayRuleScopeV2> {
        let fence = self.palw_positive_funding_relay_v2_at(daa)?;
        let policy = fence.policy.id();
        let mut fired = self.clone();
        fired.for_each_fence(&mut |at| {
            if *at > daa {
                *at = u64::MAX;
            }
        });
        fired.normalize_values_a_scheduled_fence_drags_with_it();
        let fired_rules = fired.consensus_params_id();
        let mut bytes = [0; 64];
        bytes[..32].copy_from_slice(&fired_rules.as_bytes());
        let ruleset = crate::palw_positive_relay_signature_v2::funding_relay_data_hash_v2(
            b"misaka/positive/funding-relay-ruleset/v2/r1",
            &(Digest(bytes), policy),
        );
        Some(PositiveFundingRelayRuleScopeV2 { fired_rules, policy, ruleset })
    }
    pub fn validate_palw_positive_funding_relay_v2(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_positive_funding_relay_v2.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
            return Ok(());
        };
        fence.validate_preimages()?;
        let assignment = self
            .palw_positive_assignment_v2
            .as_ref()
            .filter(|a| a.activation != ForkActivation::never() && a.activation.daa_score() <= fence.activation.daa_score())
            .ok_or(PalwModeV2Error::Invalid("relay2 requires Assignment selection at or before metadata intake"))?;
        let _original_assignment_policy = &assignment.policy;
        Err(PalwModeV2Error::Invalid("relay2 native fee/custody/binding/restore owners incomplete; no public activation"))
    }
}
