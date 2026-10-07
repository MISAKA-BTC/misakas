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

/// The algorithm ids this binary implements (one of each).
pub struct ImplementedV1;

impl ImplementedV1 {
    pub fn randomness_source() -> Digest {
        named_id("palw-work-beacon/v1")
    }
    pub fn source_eligibility() -> Digest {
        named_id("source-eligibility/future-final-active-g14-da-useful-work/v1")
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
}

impl PostCommitChallengePolicyV1 {
    /// `challenge_policy_id`: the digest of the complete canonical descriptor.
    pub fn id(&self) -> Digest {
        object_id(DOMAIN_POLICY, self)
    }

    /// Structural validity against this binary's implementations. Approval is separate ([`ApprovedTupleV1`]).
    pub fn validate(&self) -> Result<(), PolicyRefusalV1> {
        use PolicyRefusalV1 as R;
        if self.version != 1 {
            return Err(R::Version(self.version));
        }
        let ids: [(&'static str, Digest, Digest); 11] = [
            ("randomness_source_policy_id", self.randomness_source_policy_id, ImplementedV1::randomness_source()),
            ("source_eligibility_policy_id", self.source_eligibility_policy_id, ImplementedV1::source_eligibility()),
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
}

/// One approved (checker suite, challenge policy, soundness policy) tuple with its minimum repetitions. A release supplies these
/// after review; a policy outside the registry, or asking fewer repetitions than approved, is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ApprovedTupleV1 {
    pub checker_suite_id: Digest,
    pub challenge_policy_id: Digest,
    pub soundness_policy_id: Digest,
    pub min_repetitions: u32,
}

/// Whether `policy` is valid AND approved for `checker_suite_id` under `registry`.
pub fn approved_v1(
    registry: &[ApprovedTupleV1],
    checker_suite_id: &Digest,
    policy: &PostCommitChallengePolicyV1,
) -> Result<(), PolicyRefusalV1> {
    policy.validate()?;
    let id = policy.id();
    registry
        .iter()
        .find(|t| {
            t.checker_suite_id == *checker_suite_id
                && t.challenge_policy_id == id
                && t.soundness_policy_id == policy.soundness_policy_id
                && policy.repetition_count >= t.min_repetitions
        })
        .map(|_| ())
        .ok_or(PolicyRefusalV1::NotApproved)
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
