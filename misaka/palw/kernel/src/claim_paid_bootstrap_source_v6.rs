//! Explicit bootstrap-selected source namespace. Old paid source, enrollment
//! and assignment IDs keep their meanings. Hashes identify data; selected
//! approval still assumes honest frozen stake, participation and acquisition.
use crate::hash::{Digest, object_id};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::approved_source_v1::{
    ApprovedSourceCollectorPolicyV1, ApprovedSourceConsumerV1, ApprovedSourceStateV1,
};

impl crate::claim_paid_v2::PaidClaimClassV2 {
    /// Capability shape only, never approval of a class/model or a policy.
    pub fn supports_approved_bootstrap_source_v6(&self) -> bool {
        self.interactive_plan.is_none() && self.review_scope.is_none()
            && self.source_policy.interactive_mode == misaka_palw_challenge::InteractiveModeV1::NonInteractive
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PaidBootstrapSourcePolicyV6 {
    pub version: u16,
    pub network: Digest,
    pub bootstrap_policy: Digest,
    pub required_sources: u32,
    pub maximum_events: u32,
    pub anchor_delay: u64,
    pub window: u64,
    pub settlement_depth: u64,
    pub maximum_consumers: u32,
    pub source_liability_slack: u64,
}
impl PaidBootstrapSourcePolicyV6 {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 6 || [self.network, self.bootstrap_policy].contains(&[0; 64])
            || self.required_sources == 0 || self.required_sources > self.maximum_events
            || self.maximum_events > 65_536 || self.anchor_delay == 0 || self.window == 0
            || self.settlement_depth == 0 || self.maximum_consumers == 0 || self.maximum_consumers > 65_536
        { return Err("V6 bootstrap source policy is absent, unsupported or unbounded".into()); }
        Ok(())
    }
    pub fn id(&self) -> Digest { object_id(b"MISAKA/PALW/PAID-BOOTSTRAP-SOURCE-POLICY/V6", self) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PaidBootstrapSourceBindingV6 {
    pub version: u16,
    pub policy: Digest,
    pub contract: PaidBootstrapSourcePolicyV6,
    pub enrollment: Digest,
    pub original_context: Digest,
    pub checker_snapshot: Digest,
    pub liability_until: u64,
    pub collector: ApprovedSourceCollectorPolicyV1,
    pub consumer: ApprovedSourceConsumerV1,
}
impl PaidBootstrapSourceBindingV6 {
    pub fn id(&self) -> Digest { object_id(b"MISAKA/PALW/PAID-BOOTSTRAP-SOURCE-BINDING/V6", self) }
    pub fn seed(&self, source: &ApprovedSourceStateV1) -> Result<Digest, String> {
        self.contract.validate()?;
        let ApprovedSourceStateV1::Locked { context, root, output, sources, locked_at } = source else {
            return Err("V6 approved bootstrap source is not locked".into());
        };
        if self.version != 6 || *sources != self.collector.required_sources || *locked_at < self.consumer.commitment_position
            || [self.policy, self.enrollment, self.original_context, self.checker_snapshot, *context, *root, *output].contains(&[0; 64])
            || self.policy != self.contract.id() || self.collector.version != 1
            || self.collector.network != self.contract.network || self.collector.bootstrap_policy != self.contract.bootstrap_policy
            || self.collector.required_sources != self.contract.required_sources || self.collector.maximum_events != self.contract.maximum_events
            || self.collector.anchor_delay != self.contract.anchor_delay || self.collector.window != self.contract.window
            || self.collector.settlement_depth != self.contract.settlement_depth
            || *context != object_id(b"MISAKA/PALW/APPROVED-SOURCE-CONTEXT/V1", &(&self.collector, &self.consumer))
            || *output != misaka_palw_challenge::hash::h(b"MISAKA/PALW/APPROVED-SOURCE-OUTPUT/V1", root)
            || [self.collector.bootstrap_epoch, self.collector.source_profile, self.collector.checker_scope,
                self.consumer.commitment, self.consumer.target_profile, self.consumer.lineage].contains(&[0; 64])
            || self.collector.source_profile == self.consumer.target_profile
        { return Err("V6 approved source lock/binding differs".into()); }
        Ok(object_id(b"MISAKA/PALW/PAID-BOOTSTRAP-CHALLENGE-SEED/V6", &(self, source)))
    }
}

#[cfg(test)]
mod source_controls {
    use super::*;
    fn binding() -> PaidBootstrapSourceBindingV6 {
        // Data/codec controls only: these synthetic fields are not a native
        // signature, useful-work acceptance or reviewed security policy.
        let contract = PaidBootstrapSourcePolicyV6 { version: 6, network: [1;64], bootstrap_policy: [2;64],
            required_sources: 1, maximum_events: 2, anchor_delay: 1, window: 10, settlement_depth: 1,
            maximum_consumers: 2, source_liability_slack: 1 };
        PaidBootstrapSourceBindingV6 { version: 6, policy: contract.id(), contract, enrollment: [3;64],
            original_context: [4;64], checker_snapshot: [5;64], liability_until: 100,
            collector: ApprovedSourceCollectorPolicyV1 { version: 1, network: contract.network, bootstrap_epoch: [6;64],
                bootstrap_policy: contract.bootstrap_policy, source_profile: [7;64], checker_scope: [8;64],
                required_sources: 1, maximum_events: 2, anchor_delay: 1, window: 10, settlement_depth: 1 },
            consumer: ApprovedSourceConsumerV1 { commitment: [9;64], commitment_position: 20, target_profile: [10;64], lineage: [11;64] } }
    }
    fn locked(binding: &PaidBootstrapSourceBindingV6) -> ApprovedSourceStateV1 {
        let root = [12;64];
        ApprovedSourceStateV1::Locked { context: object_id(b"MISAKA/PALW/APPROVED-SOURCE-CONTEXT/V1", &(&binding.collector,&binding.consumer)),
            root, output: misaka_palw_challenge::hash::h(b"MISAKA/PALW/APPROVED-SOURCE-OUTPUT/V1", &root), sources: 1, locked_at: 25 }
    }
    #[test]
    fn pending_missing_lineage_and_substituted_context_cannot_mint_the_new_seed() {
        let good = binding(); let source = locked(&good); let seed = good.seed(&source).unwrap();
        assert_ne!(seed, object_id(b"MISAKA/PALW/PAID-CLAIM-ASSIGNMENT/V2", &good.enrollment));
        assert!(good.seed(&ApprovedSourceStateV1::Pending { found: 0, required: 1 }).is_err());
        assert!(good.seed(&ApprovedSourceStateV1::Unavailable { found: 0, required: 1 }).is_err());
        let mut wrong = good; wrong.consumer.lineage = [0;64]; assert!(wrong.seed(&source).is_err());
        let mut wrong = good; wrong.collector.bootstrap_epoch = [13;64]; assert!(wrong.seed(&source).is_err());
        let mut wrong = good; wrong.contract.required_sources = 2; assert!(wrong.seed(&source).is_err());
        let mut wrong = good; wrong.enrollment = [14;64]; assert_ne!(wrong.seed(&source).unwrap(), seed);
        let bytes = borsh::to_vec(&good).unwrap();
        assert_eq!(borsh::from_slice::<PaidBootstrapSourceBindingV6>(&bytes).unwrap(), good);
    }
    #[test]
    fn policies_do_not_gain_a_numeric_or_dormant_default() {
        let mut contract = binding().contract; contract.validate().unwrap();
        contract.required_sources = 0; assert!(contract.validate().is_err());
        contract.required_sources = 1; contract.maximum_events = 65_537; assert!(contract.validate().is_err());
        contract.maximum_events = 2; contract.network = [0;64]; assert!(contract.validate().is_err());
    }
    #[test]
    fn explicit_new_wire_keeps_every_old_lock_discriminant() {
        use crate::{route::KernelRouteObjectV1, claim_paid_v2::PaidClaimObjectV2};
        let old = KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::Lock { enrollment: [3;64] } };
        let new = KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::LockApprovedBootstrapV6 { enrollment: [3;64] } };
        assert_eq!(old.encode()[2], 4); assert_eq!(new.encode()[2], 17);
        assert_ne!(old.encode(), new.encode());
    }
}

impl crate::claim_paid_v2::PaidClaimCheckAuthorityV2 {
    /// A new typed source path, never a fabricated legacy VerifiedWorkBeacon.
    /// Native admission independently reconstructs this binding/collector from
    /// own rows; SDK callers cannot make remote event bytes native authority.
    pub fn verify_approved_bootstrap_v6(
        class: &crate::claim_paid_v2::PaidClaimClassV2,
        enrollment: &crate::claim_paid_v2::PaidClaimEnrollmentV2,
        base: &misaka_palw_challenge::ChallengeSubjectV1,
        binding: &PaidBootstrapSourceBindingV6,
        source: &ApprovedSourceStateV1,
        now: u64,
        checker_snapshot: Digest,
    ) -> Result<Self, String> {
        let subject = crate::claim_paid_v2::paid_subject_with_checker_snapshot_v2(base, class, enrollment, checker_snapshot)?;
        if !class.supports_approved_bootstrap_source_v6()
            || !enrollment.lifetime.receipt_open(now) || binding.enrollment != enrollment.id()
            || binding.checker_snapshot != checker_snapshot || binding.liability_until != enrollment.lifetime.liability_until
            || binding.consumer.commitment != subject.commitment_root || binding.consumer.target_profile != class.base_class
            || binding.consumer.commitment_position < enrollment.lifetime.committed_daa
            || binding.consumer.commitment_position > enrollment.accepted_until_daa
        { return Err("V6 check authority changes the signed native statement/checker/lifetime".into()); }
        let seed = binding.seed(source)?;
        if matches!(source, ApprovedSourceStateV1::Locked { locked_at, .. } if *locked_at > now) {
            return Err("V6 approved source lock is from a future native position".into());
        }
        Self::from_approved_bootstrap_parts_v6(class, enrollment, binding.id(), seed)
    }
}
