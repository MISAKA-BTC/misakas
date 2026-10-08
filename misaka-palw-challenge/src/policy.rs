//! **`PostCommitChallengePolicyV1`** (RFC-0007 §VI.2): how and when the challenges of an already fixed statement are derived.
//!
//! The policy names algorithms by id; this binary implements exactly one of each ([`ImplementedV1`]) and refuses every other id,
//! so an unknown sampler, source rule, hash suite or transcript transform is never a success. No numeric value (k, D, delay,
//! window, retries, repetitions) is chosen here: [`PostCommitChallengePolicyV1::validate`] checks only that each is present and
//! structurally sane. Which tuple (checker suite, challenge policy, soundness policy) a network approves is a release decision
//! ([`ApprovedTupleV1`]); the shipped registry is empty, so nothing is approved by default.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::{DOMAIN_CHALLENGE, DOMAIN_POLICY, Digest, named_id, object_id};

/// How interactive proofs take challenges (RFC-0007 §VI.6). Fixed before execution; never a per-prover fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum InteractiveModeV1 {
    /// Freivalds-style checks only: the whole statement is fixed, then one seed derives every repetition.
    NonInteractive = 0,
    /// Prover message `j` is committed on the canonical history before a NEW qualifying source window yields challenge `j`.
    StagedBeacon = 1,
    /// A transcript-bound Fiat–Shamir transform absorbing the statement, policy, source binding and every prior message and
    /// challenge. Needs its own random-oracle/QROM and grinding review (an external gate) before any approval.
    TranscriptBoundFiatShamir = 2,
}

/// The single challenge policy (RFC-0007 §VI.2, plus the beacon collection window the onboarding directive names).
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct PostCommitChallengePolicyV1 {
    pub version: u16,
    pub randomness_source_policy_id: Digest,
    /// Slots between the subject's accepted commitment position and the first position a source's work may be accepted at.
    pub anchor_delay_slots: u64,
    /// Distinct qualifying works the accumulator mixes.
    pub work_count_k: u32,
    /// Slots after the start `S` within which the `k`-th source must be accepted, or the beacon is unavailable.
    pub beacon_window_slots: u64,
    pub source_eligibility_policy_id: Digest,
    pub anchor_settlement_policy_id: Digest,
    /// PALW-native settlement depth (positions past a source's settlement) before the beacon locks.
    pub settlement_depth_d: u64,
    pub binding_schema_id: Digest,
    pub hash_suite_id: Digest,
    pub seed_hash_domain_id: Digest,
    pub sampling_algorithm_id: Digest,
    pub field_sampling_algorithm_id: Digest,
    pub field_policy_id: Digest,
    pub repetition_policy_id: Digest,
    pub repetition_count: u32,
    pub soundness_policy_id: Digest,
    pub security_bits: u16,
    pub grinding_budget_policy_id: Digest,
    pub retry_limit: u32,
    pub abort_policy_id: Digest,
    pub reorg_policy_id: Digest,
    pub interactive_mode: InteractiveModeV1,
    pub transcript_transform_id: Digest,
    pub resource_schedule_id: Digest,
    pub retention_policy_id: Digest,
}

/// **How the beacon picks its `k` sources** among the eligible works (the policy's `source_eligibility_policy_id`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceRuleV1 {
    /// The first `k` eligible works in canonical order.
    Plain,
    /// The first `k` eligible works whose producers are pairwise distinct, and whose consumers are pairwise distinct where known.
    Distinct,
    /// [`Self::Distinct`], and at most `⌈k/2⌉` from one source profile.
    DistinctClassCapped,
}

impl SourceRuleV1 {
    /// Whether the rule reads who stands behind each work (a caller must supply attributed works).
    pub fn needs_attribution(self) -> bool {
        !matches!(self, Self::Plain)
    }

    /// The most sources one profile may contribute to a beacon of `k`.
    pub fn per_profile_cap(self, k: u32) -> u32 {
        match self {
            Self::DistinctClassCapped => k.div_ceil(2),
            _ => k,
        }
    }
}

/// The algorithm ids this binary implements (one of each).
pub struct ImplementedV1;

impl ImplementedV1 {
    pub fn randomness_source() -> Digest {
        named_id("palw-work-beacon/v1")
    }
    /// **The sealed-source PALW Work Beacon v3** (`crate::sealed`): commit-reveal over bonded, salted claim seals, every qualifying
    /// seal of the window mixed, withholding a counted veto. Its source rule is the distinct one.
    pub fn randomness_sealed_source() -> Digest {
        named_id("palw-work-beacon/sealed-source/v3")
    }
    /// **No randomness: a complete check.** Every input of the subject's (small, finite) domain and every artifact leaf is checked,
    /// so nothing is sampled and no beacon exists (the OPV bootstrap, `docs/design/palw/opv-beacon-bootstrap.md` §4).
    pub fn randomness_none_complete() -> Digest {
        named_id("randomness/none-complete-check/v1")
    }
    /// The complete check's "sampler": the canonical enumeration of the whole domain (no seed is read).
    pub fn sampling_complete() -> Digest {
        named_id("sampler/complete-enumeration/v1")
    }
    /// The soundness of a complete enumeration: ε = 0 for every fault the enumerated domain can show.
    pub fn soundness_complete() -> Digest {
        named_id("soundness/complete-enumeration/v1")
    }
    /// No randomness, so nothing to grind.
    pub fn grinding_none() -> Digest {
        named_id("grinding/none-no-randomness/v1")
    }
    pub fn source_eligibility() -> Digest {
        named_id("source-eligibility/future-final-active-g14-da-useful-work/v1")
    }
    /// v1's rule, and the `k` sources come from **distinct producers**, and from distinct consumers (job posters / payers) where the
    /// consumer is known ([`crate::beacon::SourceAttributionV1`]). Needs attributed works.
    pub fn source_eligibility_distinct() -> Digest {
        named_id("source-eligibility/future-final-active-g14-da-useful-work/distinct-producer-consumer/v2")
    }
    /// The distinct rule, and at most `⌈k/2⌉` sources from one source profile (class): no single class owns the beacon.
    pub fn source_eligibility_distinct_class_capped() -> Digest {
        named_id("source-eligibility/future-final-active-g14-da-useful-work/distinct-producer-consumer-class-capped/v2")
    }
    pub fn anchor_settlement() -> Digest {
        named_id("anchor-settlement/palw-native-depth/v1")
    }
    pub fn binding_schema() -> Digest {
        named_id("binding-schema/challenge-subject-typed-absence/v1")
    }
    pub fn hash_suite() -> Digest {
        named_id("hash-suite/keyed-blake2b512-len-u64-borsh/v1")
    }
    pub fn seed_hash_domain() -> Digest {
        crate::hash::h(crate::hash::DOMAIN_ALGORITHM_ID, DOMAIN_CHALLENGE)
    }
    pub fn sampling() -> Digest {
        named_id("sampler/blake2b-counter-u64-rejection/v1")
    }
    pub fn field_sampling() -> Digest {
        named_id("field-sampler/m127-127bit-rejection/v1")
    }
    pub fn field_m127() -> Digest {
        named_id("field/gf-2^127-1/v1")
    }
    pub fn repetition_fixed() -> Digest {
        named_id("repetition/fixed-count/v1")
    }
    pub fn reorg_branch_relative() -> Digest {
        named_id("reorg/branch-relative-recompute-and-roll-back-dependents/v1")
    }
    pub fn abort_counted_retry() -> Digest {
        named_id("abort/counted-retry-ledger/v1")
    }
    pub fn transcript_none() -> Digest {
        named_id("transcript/none/v1")
    }
    pub fn transcript_staged_beacon() -> Digest {
        named_id("transcript/staged-palw-work-beacon/v1")
    }
    pub fn transcript_fiat_shamir() -> Digest {
        named_id("transcript/bound-fiat-shamir-blake2b/v1")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PolicyRefusalV1 {
    #[error("unknown policy version {0}")]
    Version(u16),
    #[error("{0}: an algorithm or policy id this binary does not implement")]
    UnknownId(&'static str),
    #[error("{0}: a required value is missing (zero)")]
    Missing(&'static str),
    #[error("the transcript transform does not match the interactive mode")]
    TranscriptMode,
    #[error("the (checker suite, challenge policy, soundness policy) tuple is not approved, or asks fewer repetitions")]
    NotApproved,
    #[error("a complete-check policy draws no randomness: {0}")]
    CompleteCheckShape(&'static str),
    #[error("a complete-check policy has no beacon to collect")]
    NoBeacon,
    #[error("another collector: {0}")]
    WrongCollector(&'static str),
    #[error("the policy's target of {target} bits is below the approval floor of {floor} effective bits")]
    TargetBelowFloor { target: u16, floor: u16 },
    #[error("the approved tuple delivers {effective:?} effective bits, below the policy's target of {target}")]
    BelowTarget { effective: crate::soundness::EffectiveBitsV1, target: u16 },
    #[error("the approved tuple's soundness statement: {0}")]
    Soundness(crate::soundness::SoundnessRefusalV1),
}

impl PostCommitChallengePolicyV1 {
    /// `challenge_policy_id`: the digest of the complete canonical descriptor.
    pub fn id(&self) -> Digest {
        object_id(DOMAIN_POLICY, self)
    }

    /// The beacon's source-selection rule the policy names (`None`: an id this binary does not implement).
    pub fn source_rule(&self) -> Option<SourceRuleV1> {
        let id = self.source_eligibility_policy_id;
        if id == ImplementedV1::source_eligibility() {
            Some(SourceRuleV1::Plain)
        } else if id == ImplementedV1::source_eligibility_distinct() {
            Some(SourceRuleV1::Distinct)
        } else if id == ImplementedV1::source_eligibility_distinct_class_capped() {
            Some(SourceRuleV1::DistinctClassCapped)
        } else {
            None
        }
    }

    /// Whether challenges of this policy are drawn from a PALW Work Beacon (`false`: a complete check, which draws nothing).
    pub fn needs_beacon(&self) -> bool {
        self.randomness_source_policy_id == ImplementedV1::randomness_source() || self.is_sealed_source()
    }

    /// Whether the beacon is the sealed-source v3 one ([`crate::sealed::collect_sealed_work_beacon_v3`]).
    pub fn is_sealed_source(&self) -> bool {
        self.randomness_source_policy_id == ImplementedV1::randomness_sealed_source()
    }

    /// Whether this is a complete-check policy (no randomness, the whole domain enumerated).
    pub fn is_complete_check(&self) -> bool {
        self.randomness_source_policy_id == ImplementedV1::randomness_none_complete()
    }

    /// Structural validity against this binary's implementations. Approval is separate ([`ApprovedTupleV1`]).
    ///
    /// Two shapes exist: a **sampled** policy (the PALW Work Beacon, a rejection sampler, every beacon number present) and a
    /// **complete-check** policy (no randomness: `k`, the delay, the window and `D` are zero, one repetition, the complete enumeration
    /// and its soundness, non-interactive). Any other randomness source is unknown.
    pub fn validate(&self) -> Result<(), PolicyRefusalV1> {
        use PolicyRefusalV1 as R;
        if self.version != 1 {
            return Err(R::Version(self.version));
        }
        if !self.needs_beacon() && !self.is_complete_check() {
            return Err(R::UnknownId("randomness_source_policy_id"));
        }
        if self.is_complete_check() {
            return self.validate_complete_check();
        }
        if self.source_rule().is_none() {
            return Err(R::UnknownId("source_eligibility_policy_id"));
        }
        // v3 mixes every qualifying seal, one per producer (and per known consumer): the distinct rule and no other.
        if self.is_sealed_source() && self.source_rule() != Some(SourceRuleV1::Distinct) {
            return Err(R::UnknownId("source_eligibility_policy_id"));
        }
        let randomness =
            if self.is_sealed_source() { ImplementedV1::randomness_sealed_source() } else { ImplementedV1::randomness_source() };
        let ids: [(&'static str, Digest, Digest); 10] = [
            ("randomness_source_policy_id", self.randomness_source_policy_id, randomness),
            ("anchor_settlement_policy_id", self.anchor_settlement_policy_id, ImplementedV1::anchor_settlement()),
            ("binding_schema_id", self.binding_schema_id, ImplementedV1::binding_schema()),
            ("hash_suite_id", self.hash_suite_id, ImplementedV1::hash_suite()),
            ("seed_hash_domain_id", self.seed_hash_domain_id, ImplementedV1::seed_hash_domain()),
            ("sampling_algorithm_id", self.sampling_algorithm_id, ImplementedV1::sampling()),
            ("field_sampling_algorithm_id", self.field_sampling_algorithm_id, ImplementedV1::field_sampling()),
            ("field_policy_id", self.field_policy_id, ImplementedV1::field_m127()),
            ("repetition_policy_id", self.repetition_policy_id, ImplementedV1::repetition_fixed()),
            ("reorg_policy_id", self.reorg_policy_id, ImplementedV1::reorg_branch_relative()),
        ];
        if let Some((name, _, _)) = ids.iter().find(|(_, got, want)| got != want) {
            return Err(R::UnknownId(name));
        }
        if self.abort_policy_id != ImplementedV1::abort_counted_retry() {
            return Err(R::UnknownId("abort_policy_id"));
        }
        let transcript = match self.interactive_mode {
            InteractiveModeV1::NonInteractive => ImplementedV1::transcript_none(),
            InteractiveModeV1::StagedBeacon => ImplementedV1::transcript_staged_beacon(),
            InteractiveModeV1::TranscriptBoundFiatShamir => ImplementedV1::transcript_fiat_shamir(),
        };
        if self.transcript_transform_id != transcript {
            return Err(R::TranscriptMode);
        }
        let zero = [0u8; 64];
        for (name, v) in [
            ("anchor_delay_slots", self.anchor_delay_slots),
            ("work_count_k", self.work_count_k as u64),
            ("beacon_window_slots", self.beacon_window_slots),
            ("settlement_depth_d", self.settlement_depth_d),
            ("repetition_count", self.repetition_count as u64),
            ("security_bits", self.security_bits as u64),
        ] {
            if v == 0 {
                return Err(R::Missing(name));
            }
        }
        for (name, d) in [
            ("soundness_policy_id", self.soundness_policy_id),
            ("grinding_budget_policy_id", self.grinding_budget_policy_id),
            ("resource_schedule_id", self.resource_schedule_id),
            ("retention_policy_id", self.retention_policy_id),
        ] {
            if d == zero {
                return Err(R::Missing(name));
            }
        }
        Ok(())
    }

    /// The complete-check shape: nothing sampled, nothing collected, nothing to grind.
    fn validate_complete_check(&self) -> Result<(), PolicyRefusalV1> {
        use PolicyRefusalV1 as R;
        let ids: [(&'static str, Digest, Digest); 7] = [
            ("hash_suite_id", self.hash_suite_id, ImplementedV1::hash_suite()),
            ("binding_schema_id", self.binding_schema_id, ImplementedV1::binding_schema()),
            ("sampling_algorithm_id", self.sampling_algorithm_id, ImplementedV1::sampling_complete()),
            ("repetition_policy_id", self.repetition_policy_id, ImplementedV1::repetition_fixed()),
            ("reorg_policy_id", self.reorg_policy_id, ImplementedV1::reorg_branch_relative()),
            ("abort_policy_id", self.abort_policy_id, ImplementedV1::abort_counted_retry()),
            ("transcript_transform_id", self.transcript_transform_id, ImplementedV1::transcript_none()),
        ];
        if let Some((name, _, _)) = ids.iter().find(|(_, got, want)| got != want) {
            return Err(R::UnknownId(name));
        }
        if self.interactive_mode != InteractiveModeV1::NonInteractive {
            return Err(R::CompleteCheckShape("a complete check is non-interactive"));
        }
        if self.work_count_k != 0 || self.anchor_delay_slots != 0 || self.beacon_window_slots != 0 || self.settlement_depth_d != 0 {
            return Err(R::CompleteCheckShape("k, the anchor delay, the beacon window and D are zero (no beacon is collected)"));
        }
        if self.repetition_count != 1 {
            return Err(R::CompleteCheckShape("one repetition (repeating an enumeration adds nothing)"));
        }
        if self.soundness_policy_id != ImplementedV1::soundness_complete() {
            return Err(R::CompleteCheckShape("the soundness of a complete enumeration"));
        }
        if self.grinding_budget_policy_id != ImplementedV1::grinding_none() {
            return Err(R::CompleteCheckShape("no randomness, no grinding budget"));
        }
        if self.security_bits == 0 {
            return Err(R::Missing("security_bits"));
        }
        let zero = [0u8; 64];
        for (name, d) in [("resource_schedule_id", self.resource_schedule_id), ("retention_policy_id", self.retention_policy_id)] {
            if d == zero {
                return Err(R::Missing(name));
            }
        }
        Ok(())
    }
}

/// One approved (checker suite, challenge policy, soundness policy) tuple with its minimum repetitions **and the reviewed soundness
/// statement the effective accounting reads**: each relation family's per-repetition soundness, the grinding choices per beacon its
/// grinding budget states, the beacons one attempt draws and the adaptive statements an adversary may commit against it. A release
/// supplies these after review; a policy outside the registry, asking fewer repetitions than approved, with a target below
/// [`crate::soundness::APPROVAL_MIN_TARGET_BITS_V1`], or whose EFFECTIVE bits fall below its own target, is refused.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ApprovedTupleV1 {
    pub checker_suite_id: Digest,
    pub challenge_policy_id: Digest,
    pub soundness_policy_id: Digest,
    pub min_repetitions: u32,
    pub relations: Vec<crate::soundness::RelationSoundnessV1>,
    pub grinding_choices_per_beacon: u128,
    pub beacons_per_attempt: u32,
    pub adaptive_queries: u128,
}

impl ApprovedTupleV1 {
    /// The tuple's statement under `policy`'s repetitions and retries.
    pub fn effective_input(&self, policy: &PostCommitChallengePolicyV1) -> crate::soundness::EffectiveSoundnessInputV1 {
        crate::soundness::EffectiveSoundnessInputV1 {
            relations: self.relations.clone(),
            repetition_count: policy.repetition_count,
            retry_limit: policy.retry_limit,
            grinding_choices_per_beacon: self.grinding_choices_per_beacon,
            beacons_per_attempt: self.beacons_per_attempt,
            adaptive_queries: self.adaptive_queries,
        }
    }
}

/// **The registry this release ships: empty.** Nothing is approved until an external review of the composition, the bootstrap and
/// the grinding attack tests (the user's ruling of 2026-10-08); a drill uses an unapproved policy and says so.
pub fn shipped_registry_v1() -> Vec<ApprovedTupleV1> {
    Vec::new()
}

/// Whether `policy` is valid AND approved for `checker_suite_id` under `registry`: a matching tuple with at most the policy's
/// repetitions, a target of at least [`crate::soundness::APPROVAL_MIN_TARGET_BITS_V1`], and an effective bound
/// ([`crate::soundness::effective_false_accept_bits_v1`] — retries, grinding, relations, adaptivity counted) at or above the target.
pub fn approved_v1(
    registry: &[ApprovedTupleV1],
    checker_suite_id: &Digest,
    policy: &PostCommitChallengePolicyV1,
) -> Result<(), PolicyRefusalV1> {
    use crate::soundness::{APPROVAL_MIN_TARGET_BITS_V1, effective_false_accept_bits_v1};
    policy.validate()?;
    let id = policy.id();
    let tuple = registry
        .iter()
        .find(|t| {
            t.checker_suite_id == *checker_suite_id
                && t.challenge_policy_id == id
                && t.soundness_policy_id == policy.soundness_policy_id
                && policy.repetition_count >= t.min_repetitions
        })
        .ok_or(PolicyRefusalV1::NotApproved)?;
    if policy.security_bits < APPROVAL_MIN_TARGET_BITS_V1 {
        return Err(PolicyRefusalV1::TargetBelowFloor { target: policy.security_bits, floor: APPROVAL_MIN_TARGET_BITS_V1 });
    }
    let effective = effective_false_accept_bits_v1(&tuple.effective_input(policy)).map_err(PolicyRefusalV1::Soundness)?;
    if !effective.meets(policy.security_bits) {
        return Err(PolicyRefusalV1::BelowTarget { effective, target: policy.security_bits });
    }
    Ok(())
}

/// [`reference_policy_v1`]'s numbers under the **sealed-source beacon v3** (the distinct rule; `window` is the seal window `W`, the
/// reveal window being the next `W`) — for tests and tools; NOT an approved or shipped value.
pub fn sealed_source_policy_v3(k: u32, delay: u64, window: u64, depth: u64, repetitions: u32) -> PostCommitChallengePolicyV1 {
    PostCommitChallengePolicyV1 {
        randomness_source_policy_id: ImplementedV1::randomness_sealed_source(),
        source_eligibility_policy_id: ImplementedV1::source_eligibility_distinct(),
        ..reference_policy_v1(k, delay, window, depth, repetitions)
    }
}

/// A structurally valid policy with the given numbers — for tests and tools; NOT an approved or shipped value.
pub fn reference_policy_v1(k: u32, delay: u64, window: u64, depth: u64, repetitions: u32) -> PostCommitChallengePolicyV1 {
    PostCommitChallengePolicyV1 {
        version: 1,
        randomness_source_policy_id: ImplementedV1::randomness_source(),
        anchor_delay_slots: delay,
        work_count_k: k,
        beacon_window_slots: window,
        source_eligibility_policy_id: ImplementedV1::source_eligibility(),
        anchor_settlement_policy_id: ImplementedV1::anchor_settlement(),
        settlement_depth_d: depth,
        binding_schema_id: ImplementedV1::binding_schema(),
        hash_suite_id: ImplementedV1::hash_suite(),
        seed_hash_domain_id: ImplementedV1::seed_hash_domain(),
        sampling_algorithm_id: ImplementedV1::sampling(),
        field_sampling_algorithm_id: ImplementedV1::field_sampling(),
        field_policy_id: ImplementedV1::field_m127(),
        repetition_policy_id: ImplementedV1::repetition_fixed(),
        repetition_count: repetitions,
        soundness_policy_id: named_id("soundness/unreviewed-test-only/v1"),
        security_bits: 40,
        grinding_budget_policy_id: named_id("grinding/unreviewed-test-only/v1"),
        retry_limit: 2,
        abort_policy_id: ImplementedV1::abort_counted_retry(),
        reorg_policy_id: ImplementedV1::reorg_branch_relative(),
        interactive_mode: InteractiveModeV1::NonInteractive,
        transcript_transform_id: ImplementedV1::transcript_none(),
        resource_schedule_id: named_id("resources/unreviewed-test-only/v1"),
        retention_policy_id: named_id("retention/unreviewed-test-only/v1"),
    }
}

/// **A complete-check policy** (no randomness) with the given target and retry limit — the shape
/// [`PostCommitChallengePolicyV1::validate`] accepts for a complete enumeration. For tests, tools and the network's onboarding
/// complete-check policy; NOT an approval.
pub fn complete_check_policy_v1(security_bits: u16, retry_limit: u32) -> PostCommitChallengePolicyV1 {
    PostCommitChallengePolicyV1 {
        version: 1,
        randomness_source_policy_id: ImplementedV1::randomness_none_complete(),
        anchor_delay_slots: 0,
        work_count_k: 0,
        beacon_window_slots: 0,
        source_eligibility_policy_id: ImplementedV1::source_eligibility(),
        anchor_settlement_policy_id: ImplementedV1::anchor_settlement(),
        settlement_depth_d: 0,
        binding_schema_id: ImplementedV1::binding_schema(),
        hash_suite_id: ImplementedV1::hash_suite(),
        seed_hash_domain_id: ImplementedV1::seed_hash_domain(),
        sampling_algorithm_id: ImplementedV1::sampling_complete(),
        field_sampling_algorithm_id: ImplementedV1::field_sampling(),
        field_policy_id: ImplementedV1::field_m127(),
        repetition_policy_id: ImplementedV1::repetition_fixed(),
        repetition_count: 1,
        soundness_policy_id: ImplementedV1::soundness_complete(),
        security_bits,
        grinding_budget_policy_id: ImplementedV1::grinding_none(),
        retry_limit,
        abort_policy_id: ImplementedV1::abort_counted_retry(),
        reorg_policy_id: ImplementedV1::reorg_branch_relative(),
        interactive_mode: InteractiveModeV1::NonInteractive,
        transcript_transform_id: ImplementedV1::transcript_none(),
        resource_schedule_id: named_id("resources/complete-check/v1"),
        retention_policy_id: named_id("retention/complete-check/v1"),
    }
}
