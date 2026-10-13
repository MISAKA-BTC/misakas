//! Fresh paid ClaimVerification readers and unsigned signed-consent builders.
//! Ledger/aux roots must come from the client's authenticated selected chain.
//! These APIs do not infer full128 approval or native free collateral from RPC.
use crate::runtime_pack::commit::Refusal;
use kaspa_consensus_core::palw_claim_challenge_v2::{
    PAID_CLAIM_CLASSES_V2, PAID_CLAIM_JOBS_V2, PAID_CLAIM_LOCKS_V2, PalwPaidClaimAuthorityV2, PalwPaidClaimLockV2,
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
    lock: PalwPaidClaimLockV2,
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
        Ok(KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::Receipt { receipt } })
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
    let authority = route.paid_claim_authority_v2(context).map_err(|e| Refusal::new("CLAIM_AUTHORITY_INVALID", e))?;
    let now = route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?.daa;
    let check = route.paid_claim_check_authority_v2(context, now).map_err(|e| Refusal::new("CLAIM_SOURCE_INVALID", e))?;
    let lock: PalwPaidClaimLockV2 = route
        .aux_row(PAID_CLAIM_LOCKS_V2, &borsh::to_vec(&authority.enrollment.id()).unwrap())
        .ok_or_else(|| Refusal::new("CLAIM_LOCK_MISSING", "native paid future lock not admitted"))?;
    if !lock.source_valid || lock.context != *context || lock.seed != check.seed() || lock.enrollment != authority.enrollment.id() {
        return Err(Refusal::new("CLAIM_LOCK_INVALID", "native lock differs from re-derived future seed"));
    }
    Ok(VerifiedPaidClaimSourcesV2 { route, authority, check, lock })
}

/// Producer explicitly signs the whole V2 enrollment and its unmodified base
/// claim body. This builder does not enroll an existing committed claim.
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
    for ((table, _), bytes) in &route.aux {
        if *table != kaspa_consensus_core::palw_claim_challenge_v2::PAID_CLAIM_ENROLLMENTS_V2 {
            continue;
        }
        let a: PalwPaidClaimAuthorityV2 =
            borsh::from_slice(bytes).map_err(|e| Refusal::new("CLAIM_AUTHORITY_INVALID", e.to_string()))?;
        if a.enrollment.id() != enrollment {
            continue;
        }
        a.validate(&route.ledger().map_err(|e| Refusal::new("CLAIM_LEDGER_INVALID", e))?)
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
        || l.claims.values().any(|c| c.job_id == base_job)
        || maximum_consumers == 0
        || last_commit_daa <= l.daa
    {
        return Err(Refusal::new("CLAIM_PAYER_CONSENT_INVALID", "actual payer must enroll the job before any claim"));
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
    l.begin_block(commit_daa).map_err(|e| Refusal::new("CLAIM_ADMISSION_INVALID", e.to_string()))?;
    let inner = KernelRouteObjectV1::CommitClaimSalted { commit: commit.clone(), salt };
    l.apply_object(&inner, &misaka_palw_kernel::ledger::AuthV1 { signer_bond: commit.producer() })
        .map_err(|e| Refusal::new("CLAIM_ADMISSION_INVALID", e.to_string()))?;
    let subject = l
        .claim_challenge_subject(&base_claim)
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
        return Err("paid claim signer accepts inner64 only".into());
    };
    let actor = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond);
    let named = match inner {
        PaidClaimObjectV2::RegisterClass { class } => Some(class.registrant),
        PaidClaimObjectV2::EnrollJob { job } => Some(job.payer),
        PaidClaimObjectV2::Commit { enrollment, commit, .. } => {
            if commit.producer() != enrollment.producer {
                return Err("producer consent differs from base claim".into());
            }
            Some(enrollment.producer)
        }
        PaidClaimObjectV2::Fund { terms, .. } => Some(terms.sponsor),
        PaidClaimObjectV2::Receipt { receipt } => Some(receipt.signer),
        PaidClaimObjectV2::Lock { .. } => None,
        PaidClaimObjectV2::StageMessage { .. } => return Err("interactive proof checker is not registered".into()),
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

pub mod cli;

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
        match self.authority.class.target {
            PaidClaimTargetV2::Program | PaidClaimTargetV2::Pipeline => self.verify_program_or_pipeline(material, material),
            PaidClaimTargetV2::Segmented => self.verify_segmented(material, material, material, &material.tokens),
            PaidClaimTargetV2::TypedSpec => self.verify_typed(material, material, material),
        }
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
    for ((table, _), bytes) in route.aux.range((PAID_CLAIM_ENROLLMENTS_V2, Vec::new())..(PAID_CLAIM_ENROLLMENTS_V2 + 1, Vec::new())) {
        if *table != PAID_CLAIM_ENROLLMENTS_V2 {
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
    let lock: PalwPaidClaimLockV2 = route
        .aux_row(PAID_CLAIM_LOCKS_V2, &borsh::to_vec(&enrollment).unwrap())
        .ok_or_else(|| Refusal::new("CLAIM_LOCK_MISSING", "Final has no rooted future lock"))?;
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
        || final_row.complete_receipts < lock.quorum
        || lock.quorum == 0
    {
        return Err(Refusal::new("CLAIM_FINAL_INVALID", "Final does not bind its complete versioned receipt/seed/clock authority"));
    }
    Ok(final_row)
}
