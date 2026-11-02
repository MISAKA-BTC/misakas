//! RFC-0044's typed, dormant native Positive Court selection.
//! Finite values and profile digests are not funding, resource reservations,
//! checked coverage or public activation authority. All shipped presets are None.
use super::{ForkActivation, Params};
use crate::{Hash64, palw_mode_v2::PalwModeV2Error};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{messages::CheckProfile, tir};

/// One explicitly bounded private profile. Native A4/A5/A6 must resolve the
/// service/schedule preimages under THIS digest and reserve the actual complete
/// quote; supplying these IDs or a feasible shape does not admit a claim.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositiveCourtPolicyV2 {
    pub version: u16,
    pub revision: u16,
    pub profile: CheckProfile,
    pub derivation_id: Hash64,
    pub selector_id: Hash64,
    pub service_profile_id: Hash64,
    pub funding_schedule_id: Hash64,
    /// Entire private EvidenceFrame, including both framing headers.
    /// Native carrier/signature overhead is additionally charged by its quote.
    pub max_frame_bytes: u64,
    pub max_part_heap: u64,
    pub max_relations: u64,
    pub max_attempts: u32,
    pub max_all_attempt_bytes: u64,
    pub max_all_attempt_elements: u64,
    pub max_all_attempt_terms: u64,
    pub max_case_heap_bytes: u64,
    pub max_case_journal_bytes: u64,
    /// Native deterministic ServiceCost.work quote, including work outside the
    /// arithmetic counters. It is not milliseconds or inference reward credit.
    pub max_case_work: u64,
    pub max_retention_daa: u64,
}

impl PositiveCourtPolicyV2 {
    pub fn root(&self) -> Hash64 {
        Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka/positive/native-policy/v2/r1")
                .hash(&borsh::to_vec(self).expect("fixed positive policy serialization"))
                .as_bytes()
                .try_into()
                .expect("64"),
        )
    }
    pub fn validate_shape(&self) -> Result<(), PalwModeV2Error> {
        if self.version != 2
            || self.revision != 1
            || self.profile != CheckProfile::ExactStreamV1
            || self.derivation_id != Hash64::from_bytes(tir::exact_derivation_id().0)
            || self.selector_id != Hash64::from_bytes(tir::greedy_selector_id().0)
        {
            return Err(PalwModeV2Error::Invalid("positive2 unsupported contract/profile/derivation/selector"));
        }
        if self.service_profile_id == Hash64::default() || self.funding_schedule_id == Hash64::default() {
            return Err(PalwModeV2Error::Invalid("positive2 missing service/funding policy identity"));
        }
        let amounts = [
            self.max_frame_bytes,
            self.max_part_heap,
            self.max_relations,
            self.max_all_attempt_bytes,
            self.max_all_attempt_elements,
            self.max_all_attempt_terms,
            self.max_case_heap_bytes,
            self.max_case_journal_bytes,
            self.max_case_work,
            self.max_retention_daa,
        ];
        if amounts.iter().any(|x| *x == 0 || *x == u64::MAX)
            || self.max_attempts == 0
            || self.max_attempts == u32::MAX
            || tir::raw_part_budget(self.max_frame_bytes).is_err()
            || self.max_frame_bytes > u32::MAX as u64 + misaka_palw_positive_contract::wire::EVIDENCE_HEADER_BYTES as u64
            || self.max_part_heap > self.max_case_heap_bytes
            || self.max_frame_bytes > self.max_all_attempt_bytes
            || self.max_all_attempt_bytes > self.max_case_journal_bytes
            || self.max_relations.checked_mul(self.max_attempts as u64).is_none()
            || self.max_all_attempt_elements.checked_add(self.max_all_attempt_terms).is_none_or(|work| work > self.max_case_work)
        {
            return Err(PalwModeV2Error::Invalid("positive2 non-finite, overflowing or inconsistent resource profile"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositiveCourtFenceV2 {
    pub activation: ForkActivation,
    pub policy: PositiveCourtPolicyV2,
}
impl PositiveCourtFenceV2 {
    /// Schema/fork probing values, NOT adopted network parameters or an
    /// admission permit. The placeholder service/funding IDs have no native
    /// preimages here; public validation still refuses every real activation.
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self {
            activation,
            policy: PositiveCourtPolicyV2 {
                version: 2,
                revision: 1,
                profile: CheckProfile::ExactStreamV1,
                derivation_id: Hash64::from_bytes(tir::exact_derivation_id().0),
                selector_id: Hash64::from_bytes(tir::greedy_selector_id().0),
                service_profile_id: Hash64::from_u64_word(1),
                funding_schedule_id: Hash64::from_u64_word(2),
                max_frame_bytes: 32 << 10,
                max_part_heap: 1 << 20,
                max_relations: 100,
                max_attempts: 2,
                max_all_attempt_bytes: 1 << 20,
                max_all_attempt_elements: 1_000_000,
                max_all_attempt_terms: 1_000_000,
                max_case_heap_bytes: 2 << 20,
                max_case_journal_bytes: 2 << 20,
                max_case_work: 4_000_000,
                max_retention_daa: 1000,
            },
        }
    }
    /// Selection only. The native fold independently checks its full admission
    /// prerequisites and role/funding/capacity capabilities at this same clock.
    pub fn active_at(&self, daa: u64) -> bool {
        self.activation != ForkActivation::never() && self.activation.is_active(daa) && self.policy.validate_shape().is_ok()
    }
    /// A signed ruleset uses the already-fired fork, not the future schedule.
    pub fn ruleset_id(&self, fired_fork: crate::Hash) -> Hash64 {
        let mut fired = [0u8; 64];
        fired[..32].copy_from_slice(&fired_fork.as_bytes());
        Hash64::from_bytes(
            misaka_palw_positive_contract::native::ruleset_id(
                misaka_palw_positive_contract::identity::Digest(fired),
                misaka_palw_positive_contract::identity::Digest(self.policy.root().as_bytes()),
            )
            .0,
        )
    }
}

impl Params {
    pub fn palw_positive_court_v2_at(&self, daa: u64) -> Option<&PositiveCourtFenceV2> {
        self.palw_positive_court_v2.as_ref().filter(|f| f.active_at(daa))
    }
    /// This is NOT a public arming permit. Profile feasibility cannot replace
    /// an adopted native funding/carrier/recovery/Gate C readiness record.
    pub fn validate_palw_positive_court_v2(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_positive_court_v2.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
            return Ok(());
        };
        fence.policy.validate_shape()?;
        Err(PalwModeV2Error::Invalid(
            "positive2 native activation has no adopted funding/carrier/coverage/Gate C/recovery readiness record; remains dormant",
        ))
    }
}

// The public-boundary regression suite is registered by the dedicated
// tests/positive_court_fence_v2.rs target, without compiling the full unit suite.

#[cfg(test)]
mod private_visitor_tests {
    use super::*;
    #[test]
    fn positive2_private_fork_visitor_changes_only_activation() {
        let mut params = super::super::TESTNET_PARAMS;
        params.palw_positive_court_v2 = Some(PositiveCourtFenceV2::unapproved_probe_at(ForkActivation::new(1000)));
        let before = params.palw_positive_court_v2.as_ref().unwrap().policy.clone();
        params.for_each_fence(&mut |daa| *daa = 2000);
        assert_eq!(params.palw_positive_court_v2.as_ref().unwrap().activation, ForkActivation::new(2000));
        assert_eq!(params.palw_positive_court_v2.as_ref().unwrap().policy, before);
    }
}
