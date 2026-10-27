//! Fresh paid ClaimVerification readers and unsigned signed-consent builders.
//! Ledger/aux roots must come from the client's authenticated selected chain.
//! These APIs do not infer full128 approval or native free collateral from RPC.
use crate::runtime_pack::commit::Refusal;
use kaspa_consensus_core::palw_claim_challenge_v2::{
    PAID_CLAIM_CLASSES_V2, PAID_CLAIM_JOBS_V2, PalwPaidClaimAuthorityV2, PalwPaidClaimLockV2,
};
use kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1;
use kaspa_hashes::Hash64;
use misaka_palw_kernel::claim_paid_v2::*;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::route::KernelRouteObjectV1;

pub struct VerifiedPaidClaimSourcesV2 {
    route: PalwKernelRouteStateV1,
    authority: PalwPaidClaimAuthorityV2,
    check: PaidClaimCheckAuthorityV2,
    lock: kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwPaidClaimLockV2>,
    round: Option<kaspa_consensus_core::palw_claim_interactive_v3::PalwPaidClaimRoundV3>,
}
impl VerifiedPaidClaimSourcesV2 {
    pub fn authority(&self) -> &PalwPaidClaimAuthorityV2 {
        &self.authority
    }
    pub fn lock(&self) -> &PalwPaidClaimLockV2 {
        &self.lock
    }
    pub fn verify_program_or_pipeline(
        &self,
        material: &dyn misaka_palw_kernel::ledger::PublicSourceV1,
        artifact: &dyn misaka_palw_kernel::ledger::PublicArtifactV1,
    ) -> Result<PaidClaimVerificationResultV2, Refusal> {
        let ledger = self.route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?;
        let outsider = misaka_palw_kernel::ledger::OutsiderV1 {
            ledger: &ledger,
            claim: self.authority.enrollment.base_claim,
            material,
            artifact,
            salt: [0; 64],
        };
        self.check.verify_program_or_pipeline(&outsider).map_err(|e| Refusal::new("CLAIM_CHECK_REFUSED", e))
    }
    pub fn verify_segmented(
        &self,
        material: &dyn misaka_palw_kernel::ledger::PublicSourceV1,
        artifact: &dyn misaka_palw_kernel::ledger::PublicArtifactV1,
        positions: &dyn misaka_palw_kernel::element::SegMaterialV1,
        tokens: &[u32],
    ) -> Result<PaidClaimVerificationResultV2, Refusal> {
        let ledger = self.route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?;
        let outsider = misaka_palw_kernel::ledger::OutsiderV1 {
            ledger: &ledger,
            claim: self.authority.enrollment.base_claim,
            material,
            artifact,
            salt: [0; 64],
        };
        self.check.verify_segmented(&outsider, positions, tokens).map_err(|e| Refusal::new("CLAIM_CHECK_REFUSED", e))
    }
    pub fn verify_typed(
        &self,
        material: &dyn misaka_palw_kernel::ledger::PublicSourceV1,
        artifact: &dyn misaka_palw_kernel::ledger::PublicArtifactV1,
        snapshots: &dyn misaka_palw_kernel::spec::outsider::SnapshotSourceV1,
    ) -> Result<PaidClaimVerificationResultV2, Refusal> {
        let ledger = self.route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?;
        let outsider = misaka_palw_kernel::spec::outsider::SpecOutsiderV1 {
            ledger: &ledger,
            claim: self.authority.enrollment.base_claim,
            material,
            artifact,
            snapshots,
            salt: [0; 64],
        };
        self.check.verify_typed(&outsider).map_err(|e| Refusal::new("CLAIM_CHECK_REFUSED", e))
    }
    /// Only the private result of an actual complete local verification can build
    /// this success receipt. It still needs the assigned seat's native signature.
    pub fn receipt(
        &self,
        checked: &VerifiedPaidClaimCheckV2,
        signer: Digest,
        signed_daa: u64,
    ) -> Result<KernelRouteObjectV1, Refusal> {
        if !self.authority.enrollment.lifetime.receipt_open(signed_daa)
            || !self.lock.seats.iter().any(|s| kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(s) == signer)
        {
            return Err(Refusal::new("CLAIM_RECEIPT_AUTHORITY", "not an assigned live signer"));
        }
        let receipt = checked.receipt(self.lock.assignment, signer, signed_daa);
        if receipt.enrollment != self.authority.enrollment.id()
            || receipt.source_context != self.lock.context
            || receipt.sample_seed != self.lock.seed
        {
            return Err(Refusal::new("CLAIM_RECEIPT_AUTHORITY", "local result belongs to another enrollment/lock"));
        }
        let object = if let Some(round) = &self.round {
            if receipt.round != round.round
                || receipt.message_root != round.message_root
                || receipt.transcript_root != round.transcript_root
            {
                return Err(Refusal::new("CLAIM_ROUND_RECEIPT_AUTHORITY", "local result replaces the signed proof round"));
            }
            PaidClaimObjectV2::RoundReceipt { receipt }
        } else if self
            .route
            .paid_bootstrap_binding_v6(&self.authority.enrollment.id())
            .map_err(|why| Refusal::new("CLAIM_SOURCE_INVALID", why.to_string()))?
            .is_some()
        {
            PaidClaimObjectV2::ReceiptApprovedBootstrapV6 { receipt }
        } else {
            PaidClaimObjectV2::Receipt { receipt }
        };
        Ok(KernelRouteObjectV1::PaidClaimV2 { object })
    }
}
pub fn paid_claim_sources_from_kernel_rows_v2(
    header: &[u8],
    rows: Vec<(u8, Vec<u8>, Vec<u8>)>,
    ledger_root: &Hash64,
    aux_root: &Hash64,
    context: &Digest,
) -> Result<VerifiedPaidClaimSourcesV2, Refusal> {
    let route = PalwKernelRouteStateV1::from_served_rows_v1(header, rows, ledger_root, aux_root)
        .map_err(|e| Refusal::new("ROWS_NOT_THE_CHAIN", e))?;
    paid_claim_sources_from_route_v2(route, context)
}

/// As with the round reader, authentication of this route's selected-chain
/// origin belongs to the caller. No raw projection installs native authority.
pub fn paid_claim_sources_from_route_v2(
    route: PalwKernelRouteStateV1,
    context: &Digest,
) -> Result<VerifiedPaidClaimSourcesV2, Refusal> {
    let authority = route.paid_claim_authority_v2(context).map_err(|e| Refusal::new("CLAIM_AUTHORITY_INVALID", e))?;
    let now = route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?.daa;
    let check = route.paid_claim_check_authority_v2(context, now).map_err(|e| Refusal::new("CLAIM_SOURCE_INVALID", e))?;
    let lock = route
        .paid_claim_effective_lock_v6(&authority)
        .map_err(|e| Refusal::new("CLAIM_LOCK_INVALID", e))?
        .ok_or_else(|| Refusal::new("CLAIM_LOCK_MISSING", "native paid future lock not admitted"))?;
    if !lock.source_valid
        || lock.context != check.context()
        || lock.seed != check.seed()
        || lock.enrollment != authority.enrollment.id()
    {
        return Err(Refusal::new("CLAIM_LOCK_INVALID", "native lock differs from re-derived future seed"));
    }
    Ok(VerifiedPaidClaimSourcesV2 { route, authority, check, lock, round: None })
}

/// Producer explicitly signs the whole V2 enrollment and its unmodified base
/// claim body. This builder does not enroll an existing committed claim.
pub fn paid_claim_commit_object_approved_bootstrap_v6(
    route: &PalwKernelRouteStateV1,
    source_policy: Digest,
    original: KernelRouteObjectV1,
) -> Result<KernelRouteObjectV1, Refusal> {
    let KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::Commit { enrollment, commit, salt } } = original else {
        return Err(Refusal::new(
            "CLAIM_BOOTSTRAP_V6",
            "V6 builder accepts only a new unsigned V2 Commit, never an existing signed WAL",
        ));
    };
    let class: PaidClaimClassV2 = route
        .aux_row(PAID_CLAIM_CLASSES_V2, &enrollment.class)
        .ok_or_else(|| Refusal::new("CLAIM_BOOTSTRAP_V6", "V6 class consent absent"))?;
    if !class.supports_approved_bootstrap_source_v6() {
        return Err(Refusal::new(
            "CLAIM_BOOTSTRAP_V6",
            "V6 source supports the explicit noninteractive unreviewed mechanics scope only",
        ));
    }
    if source_policy == [0; 64]
        || route.paid_claim_for_base_v2(&enrollment.base_claim).map_err(|why| Refusal::new("CLAIM_BOOTSTRAP_V6", why))?.is_some()
    {
        return Err(Refusal::new("CLAIM_BOOTSTRAP_V6", "V6 policy absent or this native work already has an immutable enrollment"));
    }
    Ok(KernelRouteObjectV1::PaidClaimV2 {
        object: PaidClaimObjectV2::CommitWithApprovedBootstrapV6 { enrollment, commit, salt, source_policy },
    })
}

pub fn paid_claim_commit_object_v2(
    route: &PalwKernelRouteStateV1,
    class: Digest,
    job: Digest,
    commit: misaka_palw_kernel::route::SaltedCommitV1,
    salt: Digest,
    commitment_root: Digest,
    material_consent: Digest,
    commit_daa: u64,
) -> Result<KernelRouteObjectV1, Refusal> {
    let c: PaidClaimClassV2 = route
        .aux_row(PAID_CLAIM_CLASSES_V2, &borsh::to_vec(&class).unwrap())
        .ok_or_else(|| Refusal::new("CLAIM_CLASS_MISSING", "registrant consent missing"))?;
    let j: PaidClaimJobV2 = route
        .aux_row(PAID_CLAIM_JOBS_V2, &borsh::to_vec(&job).unwrap())
        .ok_or_else(|| Refusal::new("CLAIM_JOB_MISSING", "payer consent missing"))?;
    let (base_claim, base_job) = match &commit {
        misaka_palw_kernel::route::SaltedCommitV1::Claim { claim, .. }
        | misaka_palw_kernel::route::SaltedCommitV1::Segmented { claim, .. } => (claim.id(), claim.job_id),
        misaka_palw_kernel::route::SaltedCommitV1::Pipeline { claim, .. } => (claim.id(), claim.job_id),
        misaka_palw_kernel::route::SaltedCommitV1::Spec { claim } => (claim.id(), claim.job_id()),
    };
    let l = route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?;
    if j.class != class
        || j.base_job != base_job
        || commit_daa > j.last_commit_daa
        || l.claims.contains_key(&base_claim)
        // F2 (2026-10-17): whether a live source opt-in of the job blocks the paid commit is the fold's policy-dependent rule.
        || material_consent == [0; 64]
    {
        return Err(Refusal::new("CLAIM_ENROLLMENT_INVALID", "prospective class/job/material consent or commit deadline mismatch"));
    }
    let lifetime = PaidClaimLifetimeV2::of(&c, commit_daa).map_err(|e| Refusal::new("CLAIM_LIFETIME_INVALID", e))?;
    let enrollment = PaidClaimEnrollmentV2 {
        version: 2,
        class,
        job,
        base_claim,
        producer: commit.producer(),
        commit_root: commitment_root,
        lifetime,
        accepted_until_daa: commit_daa
            .checked_add(c.maximum_commit_delay_daa)
            .ok_or_else(|| Refusal::new("CLAIM_LIFETIME_OVERFLOW", "signed acceptance bound overflow"))?,
        material_consent,
        round: 0,
        prior_transcript: [0; 64],
    };
    Ok(KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::Commit { enrollment, commit, salt } })
}
pub fn paid_claim_funding_object_v2(
    route: &PalwKernelRouteStateV1,
    enrollment: Digest,
    sponsor: Digest,
    fee_per_source: u64,
    max_sources: u32,
    policy: &kaspa_consensus_core::palw_beacon_source_fee_v1::PalwBeaconSourceFeePolicyV1,
) -> Result<KernelRouteObjectV1, Refusal> {
    for ((table, key), bytes) in &route.aux {
        if *table != kaspa_consensus_core::palw_claim_challenge_v2::PAID_CLAIM_ENROLLMENTS_V2 || key.len() != 64 {
            continue;
        }
        let a: PalwPaidClaimAuthorityV2 =
            borsh::from_slice(bytes).map_err(|e| Refusal::new("CLAIM_AUTHORITY_INVALID", e.to_string()))?;
        if a.enrollment.id() != enrollment {
            continue;
        }
        a.validate(&*route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?)
            .map_err(|e| Refusal::new("CLAIM_AUTHORITY_INVALID", e))?;
        let cap = policy.max_funded_sources.min(a.source_target.terms.max_total);
        let minimum = policy.minimum_fee_for(&a.source_target).map_err(|e| Refusal::new("CLAIM_FEE_SCOPE_INVALID", e))?;
        if a.sponsor.is_some()
            || route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?.daa >= a.source_target.terms.seal_start
            || fee_per_source < minimum
            || max_sources < a.class.source_policy.work_count_k
            || max_sources > cap
        {
            return Err(Refusal::new("CLAIM_FEE_SCOPE_INVALID", "late/replayed/insufficient funded future source"));
        }
        let terms = misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTermsV1 {
            source: a.source_target.terms,
            sponsor,
            fee_per_source,
            max_sources,
        };
        return Ok(KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::Fund { enrollment, terms } });
    }
    Err(Refusal::new("CLAIM_ENROLLMENT_MISSING", "producer consent not admitted"))
}

/// Registrant signs a new verification class identity; the mathematical base
/// class remains unchanged. Its checker timings include a complete future source
/// window followed by the original class's checking/challenge/court rights.
pub fn paid_claim_class_object_v2(
    route: &PalwKernelRouteStateV1,
    policy: &kaspa_consensus_core::palw_claim_challenge_v2::PalwClaimChallengePolicyV2,
    registrant: Digest,
    base_class: Digest,
    source_policy: misaka_palw_challenge::PostCommitChallengePolicyV1,
    source: &kaspa_consensus_core::palw_beacon_source_opt_in_v1::PalwBeaconSourcePolicyV1,
) -> Result<KernelRouteObjectV1, Refusal> {
    policy.validate().map_err(|e| Refusal::new("CLAIM_POLICY_INVALID", e))?;
    let l = route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?;
    let (target, liability) = if let Some(c) = l.classes.get(&base_class) {
        let segmented = c.descriptor.digest() == misaka_palw_kernel::descriptor::k2_tir_v4_descriptor().digest()
            || c.descriptor.digest() == misaka_palw_kernel::descriptor::k2_tir_v5_descriptor().digest();
        (if segmented { PaidClaimTargetV2::Segmented } else { PaidClaimTargetV2::Program }, c.liability_daa)
    } else if l.pipeline_classes.contains_key(&base_class) {
        (PaidClaimTargetV2::Pipeline, l.policy.liability_daa)
    } else if l.typed.classes.contains_key(&base_class) {
        (PaidClaimTargetV2::TypedSpec, l.policy.liability_daa)
    } else {
        return Err(Refusal::new("CLAIM_BASE_CLASS_MISSING", "the immutable mathematical class must be native registered"));
    };
    let check_window_daa = source_policy
        .beacon_window_slots
        .checked_mul(2)
        .and_then(|v| v.checked_add(source_policy.anchor_delay_slots))
        .and_then(|v| v.checked_add(source.source_final_slack_daa))
        .and_then(|v| v.checked_add(source_policy.settlement_depth_d))
        .and_then(|v| v.checked_add(l.policy.check_window_daa))
        .and_then(|v| v.checked_add(l.policy.check_window_daa))
        .ok_or_else(|| Refusal::new("CLAIM_LIFETIME_OVERFLOW", "future source/check window overflow"))?;
    let court_and_grace_daa = l
        .policy
        .court_deadline_daa
        .checked_add(l.policy.proof_grace_daa)
        .ok_or_else(|| Refusal::new("CLAIM_LIFETIME_OVERFLOW", "court/grace overflow"))?;
    let class = PaidClaimClassV2 {
        version: 2,
        registrant,
        base_class,
        target,
        checking: if l.mode_of_class(&base_class).is_optimistic() {
            PaidClaimCheckingV2::ProspectiveOpv
        } else {
            PaidClaimCheckingV2::Panel
        },
        checker_contract: policy.checker_contract,
        source_policy,
        check_window_daa,
        challenge_window_daa: if l.mode_of_class(&base_class).is_optimistic() {
            l.opv.policy.map_or(l.policy.challenge_window_daa, |p| p.window_daa())
        } else {
            l.policy.challenge_window_daa
        },
        court_and_grace_daa,
        liability_daa: liability.max(court_and_grace_daa),
        maximum_stream_blocks: policy.maximum_stream_blocks,
        maximum_rounds: 1,
        maximum_commit_delay_daa: l.policy.check_window_daa,
        review_scope: (policy.lifetime_review_scope != [0; 64]).then_some(policy.lifetime_review_scope),
        interactive_plan: None,
    };
    class.validate().map_err(|e| Refusal::new("CLAIM_CLASS_INVALID", e))?;
    Ok(KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::RegisterClass { class } })
}
pub fn paid_claim_job_object_v2(
    route: &PalwKernelRouteStateV1,
    class: Digest,
    base_job: Digest,
    payer: Digest,
    nonce: Digest,
    last_commit_daa: u64,
    maximum_consumers: u32,
) -> Result<KernelRouteObjectV1, Refusal> {
    let c: PaidClaimClassV2 = route
        .aux_row(PAID_CLAIM_CLASSES_V2, &borsh::to_vec(&class).unwrap())
        .ok_or_else(|| Refusal::new("CLAIM_CLASS_MISSING", "registrant consent missing"))?;
    let l = route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?;
    let baseclass = l
        .jobs
        .get(&base_job)
        .map(|j| j.class_binding_id)
        .or_else(|| l.pipeline_jobs.get(&base_job).map(|j| j.class_binding_id))
        .or_else(|| l.tiled_jobs.get(&base_job).map(|j| j.job.class_binding_id))
        .or_else(|| l.typed.jobs.get(&base_job).map(|j| j.class()));
    if l.job_poster(&base_job) != Some(payer)
        || baseclass != Some(c.base_class)
        || !misaka_palw_kernel::claim_paid_v2::paid_job_enrollment_is_prospective_v3(&l, &base_job, l.daa)
        || route.paid_claim_job_is_enrolled_v3(&base_job).map_err(|e| Refusal::new("CLAIM_JOB_AUTHORITY", e))?
        || maximum_consumers == 0
        || last_commit_daa <= l.daa
    {
        return Err(Refusal::new(
            "CLAIM_PAYER_CONSENT_INVALID",
            "actual payer must enroll the job before any live seal, source contract or claim",
        ));
    }
    Ok(KernelRouteObjectV1::PaidClaimV2 {
        object: PaidClaimObjectV2::EnrollJob {
            job: PaidClaimJobV2 { version: 2, class, base_job, payer, nonce, last_commit_daa, maximum_consumers },
        },
    })
}

/// Derive the immutable statement through the existing mathematical admission
/// path for Program, Pipeline, Segmented and typed claims. This unsigned rehearsal
/// does not debit native collateral, substitute producer consent or approve a fee.
pub fn paid_claim_commit_object_derived_v2(
    route: &PalwKernelRouteStateV1,
    class: Digest,
    job: Digest,
    commit: misaka_palw_kernel::route::SaltedCommitV1,
    salt: Digest,
    material_consent: Digest,
    commit_daa: u64,
) -> Result<KernelRouteObjectV1, Refusal> {
    let base_claim = match &commit {
        misaka_palw_kernel::route::SaltedCommitV1::Claim { claim, .. }
        | misaka_palw_kernel::route::SaltedCommitV1::Segmented { claim, .. } => claim.id(),
        misaka_palw_kernel::route::SaltedCommitV1::Pipeline { claim, .. } => claim.id(),
        misaka_palw_kernel::route::SaltedCommitV1::Spec { claim } => claim.id(),
    };
    let mut l = route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?;
    // F2 (2026-10-17): a live source opt-in of the job no longer blocks a paid commit under the seal-reveal policy; the native fold
    // applies the policy-dependent rule.
    l.try_replay_salted_claim_v4(&commit, &salt, commit.producer(), commit_daa)
        .map_err(|e| Refusal::new("CLAIM_ADMISSION_INVALID", e))?;
    let subject = l
        .try_claim_challenge_subject_v4(&base_claim)
        .map_err(|e| Refusal::new("CLAIM_ADMISSION_CAPACITY", e.to_string()))?
        .ok_or_else(|| Refusal::new("CLAIM_SUBJECT_MISSING", "all-target base statement was not admitted"))?;
    paid_claim_commit_object_v2(route, class, job, commit, salt, subject.commitment_root, material_consent, commit_daa)
}

/// Chain-specific signing for the new contract only. Role checks prevent the SDK
/// from letting a source sponsor sign producer/payer consent by accident.
pub fn sign_paid_claim_object_v2(
    key: &dyn kaspa_pq_validator_core::MessageSigner,
    domain: Hash64,
    bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    object: &KernelRouteObjectV1,
) -> Result<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2, String> {
    let KernelRouteObjectV1::PaidClaimV2 { object: inner } = object else {
        return Err("paid claim signer accepts inner71 only".into());
    };
    let actor = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond);
    let named = match inner {
        // PRINCIPLES §8: the SDK never signs a retired compact V4 contract or proof binding.
        PaidClaimObjectV2::RegisterCompactContractV4 { .. } | PaidClaimObjectV2::BindCompactProofV4 { .. } => {
            return Err(misaka_palw_kernel::claim_paid_v2::COMPACT_V4_RETIRED_REASON.into());
        }
        PaidClaimObjectV2::RegisterClass { class } => Some(class.registrant),
        PaidClaimObjectV2::EnrollJob { job } => Some(job.payer),
        PaidClaimObjectV2::Commit { enrollment, commit, .. }
        | PaidClaimObjectV2::CommitWithMaterialGenerationV5 { enrollment, commit, .. }
        | PaidClaimObjectV2::CommitWithApprovedBootstrapV6 { enrollment, commit, .. }
        | PaidClaimObjectV2::CommitApprovedBootstrapWithMaterialGenerationV7 { enrollment, commit, .. } => {
            if commit.producer() != enrollment.producer {
                return Err("producer consent differs from base claim".into());
            }
            Some(enrollment.producer)
        }
        PaidClaimObjectV2::Fund { terms, .. } => Some(terms.sponsor),
        PaidClaimObjectV2::Receipt { receipt }
        | PaidClaimObjectV2::RoundReceipt { receipt }
        | PaidClaimObjectV2::ReceiptApprovedBootstrapV6 { receipt } => Some(receipt.signer),
        PaidClaimObjectV2::FundRound { terms, .. } => Some(terms.sponsor),
        PaidClaimObjectV2::LockRound { .. } => None,
        PaidClaimObjectV2::Lock { .. } | PaidClaimObjectV2::LockApprovedBootstrapV6 { .. } => None,
        PaidClaimObjectV2::StageMessage { .. } => None, // Native admission checks the enrolled producer.
        // RFC-0007 §4.1 T6 (dormant): native admission checks the enrolled producer.
        PaidClaimObjectV2::TrapCommitment { .. } => None,
        PaidClaimObjectV2::ReviewScope { .. }
        | PaidClaimObjectV2::EconomicEvidence { .. }
        | PaidClaimObjectV2::IndependenceEvidence { .. } => None,
    };
    if named.is_some_and(|id| id != actor) {
        return Err("the signing bond is not this enrollment's registrant/payer/producer/sponsor/seat".into());
    }
    let bytes = object.encode();
    if bytes.len() > object.max_encoded_bytes()
        || bytes.len() > kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1
    {
        return Err("paid claim object exceeds native assembled bound".into());
    }
    KernelRouteObjectV1::decode(&bytes).map_err(|e| e.to_string())?;
    let message = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_message_v1(domain, &bond, &bytes);
    let signature = key.sign_message(
        message.as_byte_slice(),
        kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
    )?;
    Ok(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature })
}

pub mod all_target_bootstrap;
pub mod cli;
pub mod material_generation_v5;
pub mod material_generation_v7;
pub mod private_bootstrap;
pub mod producer_resources;
pub mod provider_actions_v4;
pub mod provider_handoff_v4;
/// RFC-0007: the explicit drawn-verifier assumption report of one paid claim (reporting only; never a credit path).
pub mod assumptions_v1;
/// RFC-0006: every registered class's derived worst-case bounds, with time/cost left UNKNOWN until measured.
pub mod model_bounds_v1;
pub mod provider_reader_v4;

/// Portable public material for the all-target CLI verifier. Every value is
/// authenticated by the mathematical verifier against the anchored commitments.
#[derive(Clone, Debug, Default, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PaidClaimMaterialWireV2 {
    pub nodes: std::collections::BTreeMap<(u8, u32, u16, u16), misaka_palw_kernel::public::TensorWireV1>,
    pub inputs: std::collections::BTreeMap<(u8, u32, u16), misaka_palw_kernel::public::TensorWireV1>,
    pub params: std::collections::BTreeMap<(u16, u16, Option<u16>), misaka_palw_kernel::public::TensorWireV1>,
    pub segmented_positions: std::collections::BTreeMap<u32, Vec<Vec<misaka_palw_kernel::public::TensorWireV1>>>,
    pub segmented_siblings: std::collections::BTreeMap<u32, Vec<Digest>>,
    pub tokens: Vec<u32>,
    pub snapshot_items: std::collections::BTreeMap<(Digest, u64), misaka_palw_kernel::spec::retrieval::RetrievalItemV1>,
}
impl misaka_palw_kernel::ledger::PublicSourceV1 for PaidClaimMaterialWireV2 {
    fn node(&self, s: u8, p: u32, o: u16, n: u16) -> Option<misaka_palw_tir::Tensor> {
        self.nodes.get(&(s, p, o, n))?.decode().ok()
    }
    fn input(&self, s: u8, p: u32, k: u16) -> Option<misaka_palw_tir::Tensor> {
        self.inputs.get(&(s, p, k))?.decode().ok()
    }
}
impl misaka_palw_kernel::ledger::PublicArtifactV1 for PaidClaimMaterialWireV2 {
    fn param(&self, p: u16, i: u16, l: Option<u16>) -> Option<misaka_palw_tir::Tensor> {
        self.params.get(&(p, i, l))?.decode().ok()
    }
}
impl misaka_palw_kernel::element::SegMaterialV1 for PaidClaimMaterialWireV2 {
    fn position(&self, p: u32) -> Option<Vec<Vec<misaka_palw_tir::Tensor>>> {
        self.segmented_positions.get(&p)?.iter().map(|o| o.iter().map(|t| t.decode().ok()).collect()).collect()
    }
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
        self.segmented_siblings.get(&p).cloned()
    }
}
impl misaka_palw_kernel::spec::outsider::SnapshotSourceV1 for PaidClaimMaterialWireV2 {
    fn item(&self, root: &Digest, id: u64) -> Option<misaka_palw_kernel::spec::retrieval::RetrievalItemV1> {
        self.snapshot_items.get(&(*root, id)).cloned()
    }
}
impl VerifiedPaidClaimSourcesV2 {
    pub fn verify_material(&self, material: &PaidClaimMaterialWireV2) -> Result<PaidClaimVerificationResultV2, Refusal> {
        let check = self.check.bind_opening_v3(material).map_err(|e| Refusal::new("CLAIM_PROOF_OPENING", e))?;
        let ledger = self.route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?;
        let outsider = misaka_palw_kernel::ledger::OutsiderV1 {
            ledger: &ledger,
            claim: self.authority.enrollment.base_claim,
            material,
            artifact: material,
            salt: [0; 64],
        };
        let finding = match self.authority.class.target {
            PaidClaimTargetV2::Program | PaidClaimTargetV2::Pipeline => check.verify_program_or_pipeline(&outsider),
            PaidClaimTargetV2::Segmented => check.verify_segmented(&outsider, material, &material.tokens),
            PaidClaimTargetV2::TypedSpec => check.verify_typed(&misaka_palw_kernel::spec::outsider::SpecOutsiderV1 {
                ledger: &ledger,
                claim: self.authority.enrollment.base_claim,
                material,
                artifact: material,
                snapshots: material,
                salt: [0; 64],
            }),
        };
        finding.map_err(|e| Refusal::new("CLAIM_PROOF_CHECK", e))
    }
}

/// Final discovery stays usable after the checking interval. The old base Final
/// retains its canonical payment identity, while this rooted V2 row records the
/// new verification identity; it never claims full128 approval from seat votes.
pub fn paid_claim_final_from_kernel_rows_v2(
    header: &[u8],
    rows: Vec<(u8, Vec<u8>, Vec<u8>)>,
    ledger_root: &Hash64,
    aux_root: &Hash64,
    enrollment: Digest,
) -> Result<kaspa_consensus_core::palw_claim_challenge_v2::PalwPaidClaimFinalV2, Refusal> {
    use kaspa_consensus_core::palw_claim_challenge_v2::{PAID_CLAIM_ENROLLMENTS_V2, PAID_CLAIM_FINALS_V2};
    let route = PalwKernelRouteStateV1::from_served_rows_v1(header, rows, ledger_root, aux_root)
        .map_err(|e| Refusal::new("ROWS_NOT_THE_CHAIN", e))?;
    let final_row: kaspa_consensus_core::palw_claim_challenge_v2::PalwPaidClaimFinalV2 = route
        .aux_row(PAID_CLAIM_FINALS_V2, &borsh::to_vec(&enrollment).unwrap())
        .ok_or_else(|| Refusal::new("CLAIM_FINAL_MISSING", "paid V2 Final projection absent"))?;
    let mut found = None;
    for ((table, key), bytes) in route.aux.range((PAID_CLAIM_ENROLLMENTS_V2, Vec::new())..(PAID_CLAIM_ENROLLMENTS_V2 + 1, Vec::new()))
    {
        if *table != PAID_CLAIM_ENROLLMENTS_V2 || key.len() != 64 {
            continue;
        }
        let a: PalwPaidClaimAuthorityV2 =
            borsh::from_slice(bytes).map_err(|e| Refusal::new("CLAIM_AUTHORITY_INVALID", e.to_string()))?;
        if a.enrollment.id() == enrollment {
            found = Some(a);
            break;
        }
    }
    let authority = found.ok_or_else(|| Refusal::new("CLAIM_AUTHORITY_MISSING", "Final has no signed prospective enrollment"))?;
    let l = route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?;
    authority.validate(&l).map_err(|e| Refusal::new("CLAIM_AUTHORITY_INVALID", e))?;
    let claim = l
        .claims
        .get(&authority.enrollment.base_claim)
        .ok_or_else(|| Refusal::new("CLAIM_BASE_MISSING", "canonical payment claim absent"))?;
    let (lock, completed_rounds, transcript_root) = if let Some(plan) = &authority.class.interactive_plan {
        if !route.paid_claim_interactive_complete_v3(&authority, &l).map_err(|e| Refusal::new("CLAIM_ROUNDS", e))? {
            return Err(Refusal::new("CLAIM_FINAL_INCOMPLETE", "all signed proof rounds must pass"));
        }
        let last = route.paid_claim_round_v3(&enrollment, plan.rounds - 1).map_err(|e| Refusal::new("CLAIM_ROUNDS", e))?;
        (
            kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1::try_from_clone_v1(
                last.lock.as_ref().ok_or_else(|| Refusal::new("CLAIM_ROUND_LOCK", "last round lock missing"))?,
            )
            .map_err(|why| Refusal::new("CLAIM_ROUND_LOCK", why.to_string()))?,
            plan.rounds,
            last.transcript_root,
        )
    } else {
        (
            route
                .paid_claim_effective_lock_v6(&authority)
                .map_err(|e| Refusal::new("CLAIM_LOCK_INVALID", e))?
                .ok_or_else(|| Refusal::new("CLAIM_LOCK_MISSING", "Final has no rooted future lock"))?,
            1,
            [0; 64],
        )
    };
    if !matches!(claim.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. })
        || !lock.source_valid
        || final_row.enrollment != enrollment
        || final_row.base_claim != authority.enrollment.base_claim
        || final_row.verification_class != authority.class.id()
        || final_row.source_context != lock.context
        || final_row.sample_seed != lock.seed
        || final_row.signed_lifetime != authority.enrollment.lifetime
        || final_row.logical_final_daa != authority.enrollment.lifetime.dispute_until.max(claim.life.final_hold_until)
        || final_row.logical_final_daa > authority.enrollment.lifetime.liability_until
        || final_row.completed_rounds != completed_rounds
        || final_row.transcript_root != transcript_root
        || final_row.complete_receipts < lock.quorum
        || lock.quorum == 0
    {
        return Err(Refusal::new("CLAIM_FINAL_INVALID", "Final does not bind its complete versioned receipt/seed/clock authority"));
    }
    Ok(final_row)
}

/// Select finite signed interactive semantics before class enrollment. Original
/// V1 class/claim codecs remain unchanged; the new unshipped V2 class carries the plan.
pub fn paid_claim_interactive_class_object_v3(
    route: &PalwKernelRouteStateV1,
    policy: &kaspa_consensus_core::palw_claim_challenge_v2::PalwClaimChallengePolicyV2,
    registrant: Digest,
    base_class: Digest,
    mut challenge: misaka_palw_challenge::PostCommitChallengePolicyV1,
    source: &kaspa_consensus_core::palw_beacon_source_opt_in_v1::PalwBeaconSourcePolicyV1,
    rounds: u32,
    maximum_material_bytes: u64,
) -> Result<KernelRouteObjectV1, Refusal> {
    let mode = challenge.interactive_mode;
    // Build clock floors through the original builder without advertising an
    // uncompiled interactive enum. Restore the explicit mode before signing.
    let transform = challenge.transcript_transform_id;
    challenge.interactive_mode = misaka_palw_challenge::InteractiveModeV1::NonInteractive;
    challenge.transcript_transform_id = misaka_palw_challenge::ImplementedV1::transcript_none();
    let mut object = paid_claim_class_object_v2(route, policy, registrant, base_class, challenge.clone(), source)?;
    let KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::RegisterClass { class } } = &mut object else { unreachable!() };
    challenge.interactive_mode = mode;
    challenge.transcript_transform_id = transform;
    class.source_policy = challenge;
    class.maximum_rounds = rounds;
    if rounds > policy.maximum_rounds {
        return Err(Refusal::new("CLAIM_ROUND_LIMIT", "plan exceeds network finite round capacity"));
    }
    class.check_window_daa = class
        .check_window_daa
        .checked_mul(u64::from(rounds))
        .ok_or_else(|| Refusal::new("CLAIM_LIFETIME_OVERFLOW", "round budget overflow"))?;
    class.interactive_plan = Some(
        misaka_palw_kernel::claim_interactive_v3::CompiledClaimProofPlanV3::compile(
            class.target,
            mode,
            rounds,
            maximum_material_bytes,
            class.maximum_stream_blocks,
        )
        .map_err(|e| Refusal::new("CLAIM_PROOF_PLAN", e))?,
    );
    class.validate().map_err(|e| Refusal::new("CLAIM_PROOF_PLAN", e))?;
    Ok(object)
}
pub fn paid_claim_round_message_object_v3(
    route: &PalwKernelRouteStateV1,
    enrollment: Digest,
    round: u32,
    material: &PaidClaimMaterialWireV2,
) -> Result<KernelRouteObjectV1, Refusal> {
    let a = route.paid_claim_enrollment_v3(&enrollment).map_err(|e| Refusal::new("CLAIM_ENROLLMENT", e))?;
    let plan = a.class.interactive_plan.as_ref().ok_or_else(|| Refusal::new("CLAIM_PROOF_PLAN", "signed plan missing"))?;
    let prior = if round == 0 {
        [0; 64]
    } else {
        let previous = route.paid_claim_round_v3(&enrollment, round - 1).map_err(|e| Refusal::new("CLAIM_PREVIOUS_ROUND", e))?;
        if !previous.complete() {
            return Err(Refusal::new("CLAIM_PREVIOUS_ROUND", "previous opening has not passed full quorum"));
        }
        previous.transcript_root
    };
    let message_root = plan.material_root(material).map_err(|e| Refusal::new("CLAIM_PROOF_OPENING", e))?;
    Ok(KernelRouteObjectV1::PaidClaimV2 {
        object: PaidClaimObjectV2::StageMessage { enrollment, round, prior_transcript: prior, message_root },
    })
}
pub fn paid_claim_round_funding_object_v3(
    route: &PalwKernelRouteStateV1,
    enrollment: Digest,
    round: u32,
    sponsor: Digest,
    fee_per_source: u64,
    max_sources: u32,
    policy: &kaspa_consensus_core::palw_beacon_source_fee_v1::PalwBeaconSourceFeePolicyV1,
) -> Result<KernelRouteObjectV1, Refusal> {
    let row = route.paid_claim_round_v3(&enrollment, round).map_err(|e| Refusal::new("CLAIM_ROUND", e))?;
    let a = route.paid_claim_enrollment_v3(&enrollment).map_err(|e| Refusal::new("CLAIM_ENROLLMENT", e))?;
    if a.class.source_policy.interactive_mode != misaka_palw_challenge::InteractiveModeV1::StagedBeacon
        || row.sponsor.is_some()
        || route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER", e))?.daa >= row.target.terms.seal_start
        || fee_per_source < policy.minimum_fee_for(&row.target).map_err(|e| Refusal::new("CLAIM_FEE", e))?
        || max_sources < a.class.source_policy.work_count_k
        || max_sources > policy.max_funded_sources.min(row.target.terms.max_total)
    {
        return Err(Refusal::new("CLAIM_ROUND_FUNDING", "late/replayed/unsupported/underfunded round"));
    }
    let terms = misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTermsV1 {
        source: row.target.terms,
        sponsor,
        fee_per_source,
        max_sources,
    };
    Ok(KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::FundRound { enrollment, round, terms } })
}
pub fn paid_claim_round_sources_from_kernel_rows_v3(
    header: &[u8],
    rows: Vec<(u8, Vec<u8>, Vec<u8>)>,
    ledger_root: &Hash64,
    aux_root: &Hash64,
    enrollment: Digest,
    round: u32,
) -> Result<VerifiedPaidClaimSourcesV2, Refusal> {
    let route = PalwKernelRouteStateV1::from_served_rows_v1(header, rows, ledger_root, aux_root)
        .map_err(|e| Refusal::new("ROWS_NOT_THE_CHAIN", e))?;
    paid_claim_round_sources_from_route_v3(route, enrollment, round)
}

/// Consume an already pinned route projection without exporting/duplicating all
/// rows. The caller authenticates its own selected-chain snapshot; this pure
/// constructor validates signed enrollment, future lock and assignment only.
/// It does not turn an arbitrary caller-supplied route into chain authority.
pub fn paid_claim_round_sources_from_route_v3(
    route: PalwKernelRouteStateV1,
    enrollment: Digest,
    round: u32,
) -> Result<VerifiedPaidClaimSourcesV2, Refusal> {
    let authority = route.paid_claim_enrollment_v3(&enrollment).map_err(|e| Refusal::new("CLAIM_ENROLLMENT", e))?;
    let row = route.paid_claim_round_v3(&enrollment, round).map_err(|e| Refusal::new("CLAIM_ROUND", e))?;
    let check = route
        .paid_claim_round_check_authority_v3(&enrollment, round, route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER", e))?.daa)
        .map_err(|e| Refusal::new("CLAIM_ROUND_SOURCE", e))?;
    let lock = kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1::try_from_clone_v1(
        row.lock.as_ref().ok_or_else(|| Refusal::new("CLAIM_ROUND_LOCK", "native future lock missing"))?,
    )
    .map_err(|why| Refusal::new("CLAIM_ROUND_LOCK", why.to_string()))?;
    if !lock.source_valid || lock.seed != check.seed() {
        return Err(Refusal::new("CLAIM_ROUND_SOURCE", "rooted lock differs from verified future source"));
    }
    Ok(VerifiedPaidClaimSourcesV2 { route, authority, check, lock, round: Some(row) })
}

pub mod compact_v4;
