//! Prospective paid claim contract. Base mathematical class/job/claim bytes remain
//! V1 data; the V2 verification identity additionally commits every consent,
//! deadline and source policy. Enrollment never reinterprets an existing claim.
use crate::hash::{Digest, object_id};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::{
    BeaconContextV1, ChallengeSubjectV1, PostCommitChallengePolicyV1, RootV1, SubjectKindV1, VerifiedWorkBeaconV1,
};

/// PRINCIPLES §8 (2026-10-15): the native compact V4 plan, its producer proof record and proof binding are retired. The
/// object variants below keep their wire identity for decoding only; every constructor that would admit, bind or credit them
/// refuses.
pub const COMPACT_V4_RETIRED: bool = true;
pub const COMPACT_V4_RETIRED_REASON: &str =
    "compact V4 is retired (PRINCIPLES §8): no compact proof record is generated, bound, accepted or credited";

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PaidClaimTargetV2 {
    Program,
    Pipeline,
    Segmented,
    TypedSpec,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PaidClaimCheckingV2 {
    /// Ordinary new Panel class. Existing OPV classes are never changed.
    Panel,
    /// Producer and payer explicitly enroll this prospective OPV computation in
    /// additional verification. It retains the original canonical payment right.
    ProspectiveOpv,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PaidClaimClassV2 {
    pub version: u16,
    pub registrant: Digest,
    pub base_class: Digest,
    pub target: PaidClaimTargetV2,
    pub checking: PaidClaimCheckingV2,
    pub checker_contract: Digest,
    pub source_policy: PostCommitChallengePolicyV1,
    pub check_window_daa: u64,
    pub challenge_window_daa: u64,
    pub court_and_grace_daa: u64,
    pub liability_daa: u64,
    pub maximum_stream_blocks: u64,
    pub maximum_rounds: u32,
    /// Signed prospective inclusion allowance. The absolute endpoints remain
    /// fixed; late inclusion consumes this allowance instead of extending H.
    pub maximum_commit_delay_daa: u64,
    pub review_scope: Option<Digest>,
    /// Signed, separately versioned interactive proof semantics.
    pub interactive_plan: Option<crate::claim_interactive_v3::CompiledClaimProofPlanV3>,
}
impl PaidClaimClassV2 {
    pub fn verification_contract_v3(&self) -> Digest {
        self.interactive_plan.as_ref().map_or(self.checker_contract, |p| p.id())
    }
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/kernel/paid-class/v2", self)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.source_policy.validate().map_err(|e| e.to_string())?;
        if self.version != 2
            || !self.source_policy.is_paid_opt_in_source()
            || self.checker_contract != checker_contract_v2()
            || self.check_window_daa == 0
            || self.challenge_window_daa == 0
            || self.court_and_grace_daa == 0
            || self.liability_daa < self.court_and_grace_daa
            || self.maximum_stream_blocks == 0
            || self.maximum_rounds == 0
            || self.maximum_commit_delay_daa >= self.check_window_daa
        {
            return Err("unsupported/unbounded paid claim checker contract".into());
        }
        if self.source_policy.interactive_mode == misaka_palw_challenge::InteractiveModeV1::NonInteractive && self.maximum_rounds != 1
        {
            return Err("noninteractive checking has exactly one committed statement".into());
        }
        match (&self.interactive_plan, self.source_policy.interactive_mode) {
            (None, misaka_palw_challenge::InteractiveModeV1::NonInteractive) => {}
            (Some(plan), mode) if mode != misaka_palw_challenge::InteractiveModeV1::NonInteractive => {
                plan.validate(self.target, mode, self.maximum_rounds, self.maximum_stream_blocks)?;
            }
            _ => return Err("interactive mode requires its exact signed compiled proof plan".into()),
        }
        Ok(())
    }
}
pub fn checker_contract_v2() -> Digest {
    crate::hash::id(b"misaka-palw/kernel/paid-checker-contract/v2", b"direct-seed/bounded-mersenne-rejection/exact-edges/all-targets")
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PaidClaimJobV2 {
    pub version: u16,
    pub class: Digest,
    pub base_job: Digest,
    pub payer: Digest,
    pub nonce: Digest,
    /// An absolute bound signed by the job payer before any enrolled reveal.
    pub last_commit_daa: u64,
    pub maximum_consumers: u32,
}
impl PaidClaimJobV2 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/kernel/paid-job/v2", self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PaidClaimLifetimeV2 {
    /// The signed not-before anchor. Actual accepted DAA is separately rooted
    /// by native authority, inside the signed finite acceptance interval.
    pub committed_daa: u64,
    pub checking_until: u64,
    pub challenge_until: u64,
    pub dispute_until: u64,
    pub liability_until: u64,
}
impl PaidClaimLifetimeV2 {
    pub fn of(c: &PaidClaimClassV2, now: u64) -> Result<Self, String> {
        c.validate()?;
        let checking_until = now.checked_add(c.check_window_daa).ok_or("claim check deadline overflow")?;
        let challenge_until = checking_until.checked_add(c.challenge_window_daa).ok_or("claim challenge deadline overflow")?;
        let dispute_until = challenge_until.checked_add(c.court_and_grace_daa).ok_or("claim dispute deadline overflow")?;
        let liability_until = dispute_until.checked_add(c.liability_daa).ok_or("claim liability deadline overflow")?;
        Ok(Self { committed_daa: now, checking_until, challenge_until, dispute_until, liability_until })
    }
    pub fn receipt_open(&self, now: u64) -> bool {
        now >= self.committed_daa && now <= self.checking_until
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PaidClaimEnrollmentV2 {
    pub version: u16,
    pub class: Digest,
    pub job: Digest,
    pub base_claim: Digest,
    pub producer: Digest,
    pub commit_root: Digest,
    /// These endpoints are part of the producer's signed object. Native inclusion
    /// requires exact equality to class-derived endpoints; no unsigned expiry.
    pub lifetime: PaidClaimLifetimeV2,
    pub accepted_until_daa: u64,
    pub material_consent: Digest,
    pub round: u32,
    pub prior_transcript: Digest,
}
impl PaidClaimEnrollmentV2 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/kernel/paid-claim-enrollment/v2", self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PaidClaimObjectV2 {
    RegisterClass {
        class: PaidClaimClassV2,
    },
    EnrollJob {
        job: PaidClaimJobV2,
    },
    /// Commit all target forms under the same explicit producer consent.
    Commit {
        enrollment: PaidClaimEnrollmentV2,
        commit: crate::route::SaltedCommitV1,
        salt: Digest,
    },
    Fund {
        enrollment: Digest,
        terms: crate::beacon_source_fee::BeaconSourceFeeTermsV1,
    },
    Lock {
        enrollment: Digest,
    },
    Receipt {
        receipt: PaidClaimReceiptV2,
    },
    /// Each new staged message consumes another lifetime admission and must be
    /// fixed before a fresh source window; never derive future staged rounds from
    /// the first seed. The payload's proof semantics are checker-contract bound.
    StageMessage {
        enrollment: Digest,
        round: u32,
        prior_transcript: Digest,
        message_root: Digest,
    },
    FundRound {
        enrollment: Digest,
        round: u32,
        terms: crate::beacon_source_fee::BeaconSourceFeeTermsV1,
    },
    LockRound {
        enrollment: Digest,
        round: u32,
    },
    RoundReceipt {
        receipt: PaidClaimReceiptV2,
    },
    ReviewScope {
        scope: misaka_palw_challenge::approval_scope::LifetimeReviewScopeV3,
    },
    EconomicEvidence {
        observation: misaka_palw_challenge::approval_scope::SourceEconomicObservationV3,
        payment: NativeSourcePaymentProofV3,
    },
    IndependenceEvidence {
        observation: misaka_palw_challenge::approval_scope::SourceIndependenceObservationV3,
    },
    /// Appended subvariants preserve every V3 discriminant and encoding.
    RegisterCompactContractV4 {
        registration: crate::claim_compact_v1::native_v4::CompactClaimRegistrationV4,
    },
    BindCompactProofV4 {
        enrollment: Digest,
        round: u32,
        proof: crate::claim_compact_v1::native_v4::NativeCompactProofV4,
    },
    /// **RFC-0007 §4.1 T6** (consumer-owned; dormant `palw_paid_claim_trap_v1`, appended — every earlier discriminant and encoding
    /// is unchanged): the producer's ONE salted commitment of a paid claim, to a planted fault (a trap) or to the null fault (an
    /// ordinary claim), posted before the claim's beacon seal window. Past the fence every non-staged paid claim carries exactly
    /// one and no seat is drawn without it, so its presence, size and poster say nothing. Opened, for a trap only, by the audit
    /// mesh's existing reveal object.
    TrapCommitment {
        enrollment: Digest,
        commitment: Digest,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PaidClaimReceiptV2 {
    pub version: u16,
    pub enrollment: Digest,
    pub verification_class: Digest,
    pub assignment: Digest,
    pub source_context: Digest,
    pub sample_seed: Digest,
    pub evidence_root: Digest,
    pub transcript_root: Digest,
    pub signer: Digest,
    pub signed_daa: u64,
    pub complete: bool,
    pub round: u32,
    pub message_root: Digest,
}

pub fn paid_subject_v2(
    base: &ChallengeSubjectV1,
    class: &PaidClaimClassV2,
    e: &PaidClaimEnrollmentV2,
) -> Result<ChallengeSubjectV1, String> {
    class.validate()?;
    if e.version != 2
        || e.class != class.id()
        || e.base_claim != base.subject_id
        || e.commit_root != base.commitment_root
        || e.producer == [0; 64]
        || e.material_consent == [0; 64]
        || e.round >= class.maximum_rounds
    {
        return Err("paid claim enrollment does not bind the immutable base statement".into());
    }
    let mut s = base.clone();
    s.subject_id = e.id();
    s.challenge_policy_id = class.source_policy.id();
    s.subject_kind = SubjectKindV1::ClaimVerification;
    s.commitment_root = object_id(b"misaka-palw/kernel/paid-claim-subject/v2", &(base.commitment_root, e.id(), class.id()));
    Ok(s)
}
/// Native checker inventory is frozen before a source window opens. This V2
/// wrapper binds that snapshot as well as the mathematical statement; a producer
/// cannot choose checker operators after learning the paid beacon.
pub fn paid_subject_with_checker_snapshot_v2(
    base: &ChallengeSubjectV1,
    class: &PaidClaimClassV2,
    e: &PaidClaimEnrollmentV2,
    snapshot: Digest,
) -> Result<ChallengeSubjectV1, String> {
    if snapshot == [0; 64] {
        return Err("checker snapshot authority is missing".into());
    }
    let mut subject = paid_subject_v2(base, class, e)?;
    subject.constraint_root = RootV1::Present(snapshot);
    subject.commitment_root = object_id(b"misaka-palw/kernel/paid-checker-snapshot-subject/v2", &(subject.commitment_root, snapshot));
    Ok(subject)
}

/// The caller supplies eligible source inventory frozen from authenticated state
/// at enrollment, rather than accepting producer-nominated source profiles.
pub fn paid_context_v2(
    subject: &ChallengeSubjectV1,
    class: &PaidClaimClassV2,
    e: &PaidClaimEnrollmentV2,
    eligible_profiles: std::collections::BTreeSet<Digest>,
    accepted_daa: u64,
) -> BeaconContextV1 {
    BeaconContextV1 {
        chain_genesis: subject.chain_genesis,
        ruleset_id: subject.ruleset_id,
        policy: class.source_policy.clone(),
        subject_kind: SubjectKindV1::ClaimVerification,
        commitment_root: subject.commitment_root,
        commitment_position: accepted_daa,
        challenge_epoch: u64::from(e.round),
        eligible_profiles,
        excluded_profiles: Default::default(),
        candidate_profile_id: RootV1::Absent,
    }
}
/// Only a verified paid beacon can construct a usable local check authority.
/// A receipt asserting success is not a locally verified result.
#[derive(Clone)]
pub struct PaidClaimCheckAuthorityV2 {
    enrollment: Digest,
    base_claim: Digest,
    verification_class: Digest,
    evidence_root: Digest,
    transcript_root: Digest,
    source_context: Digest,
    sample_seed: Digest,
    maximum_stream_blocks: u64,
    round: u32,
    message_root: Digest,
    opening_bound: bool,
    interactive_plan: Option<crate::claim_interactive_v3::CompiledClaimProofPlanV3>,
    compact_proof_root: Option<Digest>,
    compact_consumer: Option<std::rc::Rc<dyn crate::claim_compact_v1::native_v4::CompactRelationConsumerV4>>,
    can_complete: bool,
}
/// **The bootstrap lock's marker anchor** (user decision 2026-10-17, dormant): a paid lock taken before any approved paid Final
/// exists carries this anchor in place of a work beacon's challenge anchor, so every reader tells a bootstrap lock from a beacon
/// lock off the authenticated row alone. A beacon anchor equal to it is a 512-bit preimage of a fixed domain hash.
pub fn paid_claim_bootstrap_anchor_v2() -> Digest {
    object_id(b"MISAKA/PALW/PAID-CLAIM-BOOTSTRAP-ANCHOR/V2", &())
}
/// **The seal-reveal lock's marker anchor** (user decision 2026-10-17 on F1, dormant): a paid lock seeded by the commitments of claims
/// sealed after the target (`misaka_palw_challenge::paid_opt_in::collect_paid_seal_reveal_seed_v6`) carries this anchor; readers
/// re-derive the authority from the lock row, as for a bootstrap lock.
pub fn paid_claim_seal_anchor_v2() -> Digest {
    object_id(b"MISAKA/PALW/PAID-CLAIM-SEAL-REVEAL-ANCHOR/V2", &())
}
/// **The bootstrap checker-draw seed** of one paid claim: the network's pinned bootstrap seed (genesis / annual-fork params), bound
/// to the claim's beacon context and its signed statement. PREDICTABLE by construction — anyone can compute the draw before the
/// claim commits; that is why the window ends at the first approved paid Final and its claims earn the reduced reward.
pub fn paid_claim_bootstrap_seed_v2(bootstrap_seed: &Digest, context: &BeaconContextV1, commitment_root: &Digest) -> Digest {
    object_id(b"MISAKA/PALW/PAID-CLAIM-BOOTSTRAP-SEED/V2", &(bootstrap_seed, context.id(), commitment_root))
}
impl PaidClaimCheckAuthorityV2 {
    pub fn verify(
        class: &PaidClaimClassV2,
        enrollment: &PaidClaimEnrollmentV2,
        subject: &ChallengeSubjectV1,
        context: &BeaconContextV1,
        beacon: &VerifiedWorkBeaconV1,
        now: u64,
        checker_snapshot: Digest,
    ) -> Result<Self, String> {
        Self::verify_seeded(class, enrollment, subject, context, now, checker_snapshot, |expected| {
            misaka_palw_challenge::challenge_seed_v1(context, expected, beacon).map_err(|e| e.to_string())
        })
    }
    /// **The bootstrap check authority** (dormant; user decision 2026-10-17): the same statement checks as [`Self::verify`], with
    /// the checker draw seeded by `sample_seed` instead of a locked work beacon. The native lock computes that seed with
    /// [`paid_claim_bootstrap_seed_v2`] from the network's pinned bootstrap seed, only while no approved paid Final exists, and
    /// writes it into the lock row (anchor [`paid_claim_bootstrap_anchor_v2`]); every later reader passes the lock row's seed — an
    /// authenticated chain row, never a value a party supplies.
    #[allow(clippy::too_many_arguments)]
    pub fn verify_bootstrap_v2(
        class: &PaidClaimClassV2,
        enrollment: &PaidClaimEnrollmentV2,
        subject: &ChallengeSubjectV1,
        context: &BeaconContextV1,
        now: u64,
        checker_snapshot: Digest,
        sample_seed: Digest,
    ) -> Result<Self, String> {
        if class.interactive_plan.is_some() {
            return Err("a staged/interactive paid class takes no bootstrap seed".into());
        }
        Self::verify_seeded(class, enrollment, subject, context, now, checker_snapshot, |_| Ok(sample_seed))
    }
    fn verify_seeded(
        class: &PaidClaimClassV2,
        enrollment: &PaidClaimEnrollmentV2,
        subject: &ChallengeSubjectV1,
        context: &BeaconContextV1,
        now: u64,
        checker_snapshot: Digest,
        seed_of: impl FnOnce(&ChallengeSubjectV1) -> Result<Digest, String>,
    ) -> Result<Self, String> {
        let expected = paid_subject_with_checker_snapshot_v2(subject, class, enrollment, checker_snapshot)?;
        if !enrollment.lifetime.receipt_open(now)
            || context.policy != class.source_policy
            || context.subject_kind != SubjectKindV1::ClaimVerification
            || context.commitment_root != expected.commitment_root
            || context.commitment_position < enrollment.lifetime.committed_daa
            || context.commitment_position > enrollment.accepted_until_daa
            || context.challenge_epoch != u64::from(enrollment.round)
        {
            return Err("paid check authority is late or substitutes its signed statement/policy".into());
        }
        let seed = seed_of(&expected)?;
        Ok(Self {
            enrollment: enrollment.id(),
            base_claim: enrollment.base_claim,
            verification_class: class.id(),
            evidence_root: enrollment.commit_root,
            transcript_root: enrollment.prior_transcript,
            source_context: context.id(),
            sample_seed: seed,
            maximum_stream_blocks: class.maximum_stream_blocks,
            round: 0,
            message_root: [0; 64],
            opening_bound: class.interactive_plan.is_none(),
            interactive_plan: class.interactive_plan.clone(),
            compact_proof_root: None,
            compact_consumer: None,
            can_complete: class.interactive_plan.as_ref().is_none_or(|p| p.version != 4),
        })
    }
    /// Caller must supply the independently verified source beacon; the transcript
    /// validator binds every producer message and challenge before this constructor.
    fn for_interactive_round_inner(
        class: &PaidClaimClassV2,
        enrollment: &PaidClaimEnrollmentV2,
        context: &BeaconContextV1,
        source_subject: &ChallengeSubjectV1,
        beacon: &VerifiedWorkBeaconV1,
        round: u32,
        message_root: Digest,
        prior_transcript: Digest,
        message_daa: u64,
        now: u64,
        prior_rounds: &[(Digest, Digest)],
        prior_compact: Option<&[(Digest, Digest, Digest)]>,
    ) -> Result<Self, String> {
        class.validate()?;
        let plan = class.interactive_plan.as_ref().ok_or("interactive proof plan missing")?;
        if (plan.version == 4) != prior_compact.is_some() {
            return Err("compact plans require the explicit V4 transcript constructor".into());
        }

        if enrollment.version != 2
            || enrollment.class != class.id()
            || round >= plan.rounds
            || message_root == [0; 64]
            || !enrollment.lifetime.receipt_open(now)
            || context.policy != class.source_policy
        {
            return Err("interactive round is outside its signed plan/lifetime".into());
        }
        let base_seed = misaka_palw_challenge::challenge_seed_v1(context, source_subject, beacon).map_err(|e| e.to_string())?;
        let sample_seed = match plan.mode {
            misaka_palw_challenge::InteractiveModeV1::StagedBeacon => misaka_palw_challenge::staged_round_challenge_v1(
                plan.mode,
                &base_seed,
                round,
                &message_root,
                message_daa,
                beacon,
                context.commitment_position.checked_add(context.policy.anchor_delay_slots).ok_or("round source overflow")?,
            )
            .map_err(|e| e.to_string())?,
            misaka_palw_challenge::InteractiveModeV1::TranscriptBoundFiatShamir if plan.version == 4 => {
                crate::claim_interactive_v3::fs_round_seed_v4(
                    plan,
                    enrollment.commit_root,
                    class.source_policy.id(),
                    beacon,
                    round,
                    prior_transcript,
                    message_root,
                    prior_compact.unwrap(),
                )?
            }
            misaka_palw_challenge::InteractiveModeV1::TranscriptBoundFiatShamir => crate::claim_interactive_v3::fs_round_seed_v3(
                plan,
                enrollment.commit_root,
                class.source_policy.id(),
                beacon,
                round,
                prior_transcript,
                message_root,
                prior_rounds,
            )?,
            _ => return Err("interactive checker cannot use a noninteractive transcript".into()),
        };
        let source_context = context.id();
        let transcript_root = crate::claim_interactive_v3::transcript_root_v3(
            plan,
            enrollment.id(),
            round,
            prior_transcript,
            message_root,
            source_context,
            sample_seed,
        );
        Ok(Self {
            enrollment: enrollment.id(),
            base_claim: enrollment.base_claim,
            verification_class: class.id(),
            evidence_root: enrollment.commit_root,
            transcript_root,
            source_context,
            sample_seed,
            maximum_stream_blocks: class.maximum_stream_blocks,
            round,
            message_root,
            opening_bound: false,
            interactive_plan: Some(plan.clone()),
            compact_proof_root: None,
            compact_consumer: None,
            can_complete: plan.version != 4,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn for_interactive_round_v3(
        class: &PaidClaimClassV2,
        enrollment: &PaidClaimEnrollmentV2,
        context: &BeaconContextV1,
        subject: &ChallengeSubjectV1,
        beacon: &VerifiedWorkBeaconV1,
        round: u32,
        message: Digest,
        prior: Digest,
        message_daa: u64,
        now: u64,
        previous: &[(Digest, Digest)],
    ) -> Result<Self, String> {
        Self::for_interactive_round_inner(
            class,
            enrollment,
            context,
            subject,
            beacon,
            round,
            message,
            prior,
            message_daa,
            now,
            previous,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn for_interactive_round_v4(
        class: &PaidClaimClassV2,
        enrollment: &PaidClaimEnrollmentV2,
        context: &BeaconContextV1,
        subject: &ChallengeSubjectV1,
        beacon: &VerifiedWorkBeaconV1,
        round: u32,
        message: Digest,
        prior: Digest,
        message_daa: u64,
        now: u64,
        previous: &[(Digest, Digest, Digest)],
    ) -> Result<Self, String> {
        // PRINCIPLES §8 (2026-10-15): the V4 compact plan is retired; no check authority is built for it.
        if COMPACT_V4_RETIRED {
            return Err(COMPACT_V4_RETIRED_REASON.into());
        }
        Self::for_interactive_round_inner(
            class,
            enrollment,
            context,
            subject,
            beacon,
            round,
            message,
            prior,
            message_daa,
            now,
            &[],
            Some(previous),
        )
    }
    pub fn with_compact_proof_binding_v4(mut self, root: Digest) -> Result<Self, String> {
        if COMPACT_V4_RETIRED {
            return Err(COMPACT_V4_RETIRED_REASON.into());
        }
        if self.interactive_plan.as_ref().is_none_or(|p| p.version != 4) || self.compact_proof_root.is_some() || root == [0; 64] {
            return Err("compact proof binding has wrong plan/duplicate/zero root".into());
        }
        self.compact_proof_root = Some(root);
        self.transcript_root = crate::claim_compact_v1::native_v4::compact_transcript_root_v4(self.transcript_root, root);
        self.can_complete = true;
        Ok(self)
    }
    pub fn bind_compact_opening_v4<T: BorshSerialize>(
        &self,
        material: &T,
        registration: &crate::claim_compact_v1::native_v4::CompactClaimRegistrationV4,
        session: std::rc::Rc<crate::claim_compact_v1::native_v4::CompactVerificationSessionV4>,
        proof: Option<&crate::claim_compact_v1::native_v4::NativeCompactProofV4>,
    ) -> Result<Self, String> {
        if COMPACT_V4_RETIRED {
            return Err(COMPACT_V4_RETIRED_REASON.into());
        }
        let plan = self.interactive_plan.as_ref().ok_or("compact plan missing")?;
        if plan.version != 4
            || *plan != registration.plan
            || plan.material_root(material)? != self.message_root
            || plan.compiler
                != crate::claim_compact_v1::native_v4::compact_suite_id_v4(
                    &registration.contract,
                    &registration.coverage,
                    registration.adversary,
                )
            || registration.contract.coverage_root != registration.coverage.root()
            || !session.matches(registration, self.sample_seed)
        {
            return Err("compact material/plan differs from signed native round".into());
        }
        let mut bound = self.clone();
        if let Some(proof) = proof {
            if self.compact_proof_root != Some(proof.root()) {
                return Err("compact proof differs from producer post-seed binding".into());
            }
        } else {
            bound.can_complete = false;
        }
        bound.opening_bound = true;
        bound.compact_consumer = Some(session);
        Ok(bound)
    }
    pub fn enrollment(&self) -> Digest {
        self.enrollment
    }
    pub fn context(&self) -> Digest {
        self.source_context
    }
    pub fn seed(&self) -> Digest {
        self.sample_seed
    }
    pub fn bind_opening_v3<T: BorshSerialize>(&self, material: &T) -> Result<Self, String> {
        let mut bound = self.clone();
        if let Some(plan) = &self.interactive_plan {
            if plan.version == 4 {
                return Err("V4 needs the registered compact verifier and post-seed proof binding".into());
            }
            if plan.material_root(material)? != self.message_root {
                return Err("full-material opening differs from the signed round message".into());
            }
            bound.opening_bound = true;
        }
        Ok(bound)
    }
    pub fn randomness(&self) -> Result<crate::challenge::CheckerRandomnessV2, String> {
        if !self.opening_bound {
            return Err("interactive full-material message has not been authenticated".into());
        }
        let randomness = crate::challenge::CheckerRandomnessV2::new(self.sample_seed, self.maximum_stream_blocks)?;
        Ok(if let Some(consumer) = &self.compact_consumer { randomness.with_compact(consumer.clone()) } else { randomness })
    }
    pub fn check_program_or_pipeline(
        &self,
        outsider: &crate::ledger::OutsiderV1<'_>,
    ) -> Result<crate::ledger::OutsiderFindingV1, String> {
        if outsider.claim != self.base_claim {
            return Err("paid checker belongs to another base claim".into());
        }
        outsider.check_seeded_v2(&self.randomness()?)
    }
    pub fn check_typed(
        &self,
        outsider: &crate::spec::outsider::SpecOutsiderV1<'_>,
    ) -> Result<crate::ledger::OutsiderFindingV1, String> {
        if outsider.claim != self.base_claim {
            return Err("paid checker belongs to another base claim".into());
        }
        outsider.check_seeded_v2(&self.randomness()?)
    }
    /// Segmented suites already check every position by deterministic replay.
    /// Their exact checker is preserved, while source/enrollment admission remains
    /// explicit. A sampled diagnostic subset can never issue complete coverage.
    pub fn check_segmented(
        &self,
        outsider: &crate::ledger::OutsiderV1<'_>,
        positions: &dyn crate::element::SegMaterialV1,
        tokens: &[u32],
    ) -> Result<crate::ledger::OutsiderFindingV1, String> {
        if !self.opening_bound || outsider.claim != self.base_claim {
            return Err("paid segmented checker has another claim or unauthenticated opening".into());
        }
        let finding = outsider.check_segmented_computation(positions, tokens)?;
        if matches!(finding, crate::ledger::OutsiderFindingV1::Clean) && self.compact_consumer.is_some() {
            let view = outsider.ledger.seg_claim_view_v1(&outsider.claim).ok_or("compact segmented claim missing")?;
            let row = outsider.ledger.claims.get(&outsider.claim).ok_or("compact base claim missing")?;
            let class = outsider.ledger.classes.get(&row.class_binding_id).ok_or("compact segmented class missing")?;
            crate::claim_compact_v1::native_v4::check_segmented_compact_v4(
                &view.context(),
                class.plan.root(),
                positions,
                &|index, layer| outsider.artifact.param(0, index, layer),
                tokens,
                &self.randomness()?,
            )?;
        }
        Ok(finding)
    }
    fn checked(&self, finding: crate::ledger::OutsiderFindingV1) -> PaidClaimVerificationResultV2 {
        let compact_complete = self.interactive_plan.as_ref().is_none_or(|p| p.version != 4)
            || self.compact_consumer.as_ref().is_some_and(|s| s.complete().is_ok());
        let complete = (self.can_complete && compact_complete && matches!(finding, crate::ledger::OutsiderFindingV1::Clean))
            .then_some(VerifiedPaidClaimCheckV2 {
                enrollment: self.enrollment,
                verification_class: self.verification_class,
                source_context: self.source_context,
                sample_seed: self.sample_seed,
                evidence_root: self.evidence_root,
                transcript_root: self.transcript_root,
                round: self.round,
                message_root: self.message_root,
            });
        PaidClaimVerificationResultV2 { finding, complete }
    }
    pub fn verify_program_or_pipeline(
        &self,
        outsider: &crate::ledger::OutsiderV1<'_>,
    ) -> Result<PaidClaimVerificationResultV2, String> {
        Ok(self.checked(self.check_program_or_pipeline(outsider)?))
    }
    pub fn verify_typed(&self, outsider: &crate::spec::outsider::SpecOutsiderV1<'_>) -> Result<PaidClaimVerificationResultV2, String> {
        Ok(self.checked(self.check_typed(outsider)?))
    }
    pub fn verify_segmented(
        &self,
        outsider: &crate::ledger::OutsiderV1<'_>,
        positions: &dyn crate::element::SegMaterialV1,
        tokens: &[u32],
    ) -> Result<PaidClaimVerificationResultV2, String> {
        Ok(self.checked(self.check_segmented(outsider, positions, tokens)?))
    }
}

/// Consumer-derived from authenticated enrollment/receipts. It is not a new V1
/// row encoding and a transaction cannot install it in the bare ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaidClaimConsumerV2 {
    pub lifetime: PaidClaimLifetimeV2,
    pub complete: bool,
}

/// Locally verified complete results cannot be deserialized from a seat receipt.
pub struct VerifiedPaidClaimCheckV2 {
    enrollment: Digest,
    verification_class: Digest,
    source_context: Digest,
    sample_seed: Digest,
    evidence_root: Digest,
    transcript_root: Digest,
    round: u32,
    message_root: Digest,
}
pub struct PaidClaimVerificationResultV2 {
    pub finding: crate::ledger::OutsiderFindingV1,
    pub complete: Option<VerifiedPaidClaimCheckV2>,
}
impl VerifiedPaidClaimCheckV2 {
    pub fn receipt(&self, assignment: Digest, signer: Digest, signed_daa: u64) -> PaidClaimReceiptV2 {
        PaidClaimReceiptV2 {
            version: 2,
            enrollment: self.enrollment,
            verification_class: self.verification_class,
            assignment,
            source_context: self.source_context,
            sample_seed: self.sample_seed,
            evidence_root: self.evidence_root,
            transcript_root: self.transcript_root,
            signer,
            signed_daa,
            complete: true,
            round: self.round,
            message_root: self.message_root,
        }
    }
}

/// Native payment provenance: a canonical header and its complete transaction
/// body bind an actual coinbase output to the native drain observation. The
/// consumer imposes byte/count/court-work bounds before parsing. A promise is
/// never substituted for this proof.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeSourcePaymentProofV3 {
    pub header_bytes: Vec<u8>,
    pub transaction_bytes: Vec<Vec<u8>>,
    pub coinbase_output: u32,
}

/// New signed checker terms must precede every observable producer commitment.
/// A payer cannot retrospectively migrate an ordinary sealed job (including an
/// already prepaid beacon source) into the paid-checking/absolute-expiry route.
pub fn paid_job_enrollment_is_prospective_v3(ledger: &crate::ledger::KernelLedgerV1, job: &Digest, now: u64) -> bool {
    !ledger.claims.values().any(|claim| claim.job_id == *job)
        && !ledger.seals.iter().any(|((id, _), seal)| id == job && now <= seal.daa.saturating_add(ledger.policy.seal_ttl_daa))
        && !ledger.beacon_sources.values().any(|source| source.job == *job && source.outcome.is_none())
}
