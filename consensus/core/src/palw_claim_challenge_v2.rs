//! Prospective paid ClaimVerification authority. Aux rows are authenticated by the
//! existing kernel-route root/carriage/journal; none changes a V1 ledger codec.
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_state_v2::PalwBondKeyV2;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::claim_paid_v2::*;
use misaka_palw_kernel::hash::{Digest, object_id};

pub const PAID_CLAIM_CLASSES_V2: u8 = 113;
pub const PAID_CLAIM_JOBS_V2: u8 = 114;
pub const PAID_CLAIM_ENROLLMENTS_V2: u8 = 115;
pub const PAID_CLAIM_LOCKS_V2: u8 = 116;
pub const PAID_CLAIM_RECEIPTS_V2: u8 = 117;
pub const PAID_CLAIM_REVIEW_COUNTERS_V2: u8 = 118;
pub const PAID_CLAIM_EVIDENCE_V2: u8 = 119;
pub const PAID_CLAIM_REVIEW_SCOPES_V2: u8 = 122;
pub const PAID_CLAIM_ROUNDS_V2: u8 = 123;
pub const PAID_CLAIM_FINALS_V2: u8 = 124;

/// Explicit aux119 envelope for typed native resource preflight and restore.
/// These unshipped rows never reinterpret an older wire or observed receipt.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPaidClaimEvidenceV3 {
    Economic { version: u16, observation: misaka_palw_challenge::approval_scope::SourceEconomicObservationV3 },
    Independence { version: u16, observation: misaka_palw_challenge::approval_scope::SourceIndependenceObservationV3 },
}
impl PalwPaidClaimEvidenceV3 {
    pub fn id(&self) -> Digest {
        match self {
            Self::Economic { observation, .. } => observation.id(),
            Self::Independence { observation, .. } => observation.id(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Economic { version: 3, observation } => observation.validate(),
            Self::Independence { version: 3, observation } => observation.validate(),
            _ => Err("unknown paid source evidence envelope version".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimChallengePolicyV2 {
    pub version: u16,
    pub checker_contract: Digest,
    pub maximum_classes: u32,
    pub maximum_jobs: u32,
    pub maximum_live_claims: u32,
    pub maximum_enrollments_per_job: u32,
    pub maximum_stream_blocks: u64,
    pub maximum_rounds: u32,
    pub review_authority: Digest,
    /// An authenticated release scope from an independent review. Zero means
    /// private mechanics only, never public effective approval.
    pub lifetime_review_scope: Digest,
    pub source_independence_evidence: Digest,
    pub economic_evidence: Digest,
}
impl PalwClaimChallengePolicyV2 {
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/CLAIM-CHALLENGE-POLICY/V2", self)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 2
            || self.checker_contract != checker_contract_v2()
            || self.maximum_classes == 0
            || self.maximum_jobs == 0
            || self.maximum_live_claims == 0
            || self.maximum_enrollments_per_job == 0
            || self.maximum_stream_blocks == 0
            || self.maximum_rounds == 0
            || self.maximum_rounds > 64
            || self.maximum_classes > 65_536
            || self.maximum_jobs > 65_536
            || self.maximum_live_claims > 65_536
            || self.maximum_enrollments_per_job > 1024
            || u64::from(self.maximum_jobs) * u64::from(self.maximum_enrollments_per_job) > 1_000_000
        {
            return Err("paid claim policy has unsupported checker or unbounded resource/lifetime limits".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimChallengeFenceV2 {
    pub activation: ForkActivation,
    pub policy: PalwClaimChallengePolicyV2,
}
impl PalwClaimChallengeFenceV2 {
    /// Unapproved mechanics-only configuration. No shipped/public preset uses it.
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self {
            activation,
            policy: PalwClaimChallengePolicyV2 {
                version: 2,
                checker_contract: checker_contract_v2(),
                maximum_classes: 64,
                maximum_jobs: 1024,
                maximum_live_claims: 1024,
                maximum_enrollments_per_job: 8,
                maximum_stream_blocks: 1_000_000,
                maximum_rounds: 1,
                review_authority: [0; 64],
                lifetime_review_scope: [0; 64],
                source_independence_evidence: [0; 64],
                economic_evidence: [0; 64],
            },
        }
    }
}
impl Params {
    pub fn validate_palw_claim_challenge_v2(&self) -> Result<(), PalwModeV2Error> {
        if let Some(f) = self.palw_claim_challenge_v2 {
            f.policy.validate().map_err(PalwModeV2Error::InvalidOwned)?;
            if f.activation != ForkActivation::never() {
                return Err(PalwModeV2Error::InvalidOwned("palw_claim_challenge_v2 remains dormant: independent full128/source/economic review and native all-target acceptance must be evidenced".into()));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidCheckerCandidateV2 {
    pub bond: PalwBondKeyV2,
    pub operator: kaspa_hashes::Hash64,
    pub weight: u64,
}
pub fn paid_checker_snapshot_root_v2(candidates: &[PalwPaidCheckerCandidateV2]) -> Digest {
    object_id(b"MISAKA/PALW/PAID-CHECKER-SNAPSHOT/V2", &candidates)
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidClaimAuthorityV2 {
    pub version: u16,
    pub enrollment: PaidClaimEnrollmentV2,
    pub class: PaidClaimClassV2,
    pub job: PaidClaimJobV2,
    pub subject: misaka_palw_challenge::ChallengeSubjectV1,
    pub context: misaka_palw_challenge::BeaconContextV1,
    pub source_target: misaka_palw_kernel::beacon_source::BeaconSourceTargetV1,
    pub source_policy: crate::palw_beacon_source_opt_in_v1::PalwBeaconSourcePolicyV1,
    pub authorized_daa: u64,
    pub producer: PalwBondKeyV2,
    pub sponsor: Option<PalwBondKeyV2>,
    pub checker_candidates: Vec<PalwPaidCheckerCandidateV2>,
}
impl PalwPaidClaimAuthorityV2 {
    pub fn validate(&self, ledger: &misaka_palw_kernel::ledger::KernelLedgerV1) -> Result<(), String> {
        let e = &self.enrollment;
        let c = &self.class;
        let row = ledger.claims.get(&e.base_claim).ok_or("paid enrollment base claim missing")?;
        let base = ledger.claim_challenge_subject(&e.base_claim).ok_or("paid claim subject missing")?;
        if self.version != 2
            || self.authorized_daa < e.lifetime.committed_daa
            || self.authorized_daa > e.accepted_until_daa
            || row.committed_daa != self.authorized_daa
            || row.producer != e.producer
            || row.class_binding_id != c.base_class
            || row.job_id != self.job.base_job
            || self.job.class != c.id()
            || e.job != self.job.id()
            || e.class != c.id()
            || e.lifetime != PaidClaimLifetimeV2::of(c, e.lifetime.committed_daa)?
            || e.lifetime.committed_daa.checked_add(c.maximum_commit_delay_daa) != Some(e.accepted_until_daa)
            || self.subject
                != paid_subject_with_checker_snapshot_v2(&base, c, e, paid_checker_snapshot_root_v2(&self.checker_candidates))?
            || self.context != paid_context_v2(&self.subject, c, e, self.context.eligible_profiles.clone(), self.authorized_daa)
            || self.source_target != claim_source_target_v2(self.source_policy, self.context.clone(), e.lifetime, &ledger.policy)?
        {
            return Err("paid claim authority substitutes its policy/job/producer/statement/deadlines".into());
        }
        if self.checker_candidates.len() > 65_536
            || self.checker_candidates.is_empty()
            || self.checker_candidates.windows(2).any(|pair| pair[0].bond >= pair[1].bond)
            || self.checker_candidates.iter().any(|c| c.weight == 0 || c.weight > 1_000_000 || c.bond == self.producer)
        {
            return Err("checker snapshot is malformed, unbounded or duplicates the producer".into());
        }
        Ok(())
    }
}
pub fn claim_source_target_v2(
    source: crate::palw_beacon_source_opt_in_v1::PalwBeaconSourcePolicyV1,
    context: misaka_palw_challenge::BeaconContextV1,
    lifetime: PaidClaimLifetimeV2,
    ledger: &misaka_palw_kernel::ledger::LedgerPolicyV1,
) -> Result<misaka_palw_kernel::beacon_source::BeaconSourceTargetV1, String> {
    let floor = ledger
        .challenge_window_daa
        .checked_add(ledger.court_deadline_daa)
        .and_then(|v| v.checked_add(ledger.proof_grace_daa))
        .ok_or("claim source floor overflow")?;
    if context.subject_kind != misaka_palw_challenge::SubjectKindV1::ClaimVerification
        || !context.policy.is_paid_opt_in_source()
        || context.candidate_profile_id != misaka_palw_challenge::RootV1::Absent
        || source.source_final_slack_daa < floor
    {
        return Err("claim source needs its paid native source/court contract".into());
    }
    let seal_start =
        context.commitment_position.checked_add(context.policy.anchor_delay_slots).ok_or("claim source start overflow")?;
    let seal_end = seal_start.checked_add(context.policy.beacon_window_slots).ok_or("claim source seal overflow")?;
    let reveal_end = seal_end.checked_add(context.policy.beacon_window_slots).ok_or("claim source reveal overflow")?;
    let final_by = reveal_end.checked_add(source.source_final_slack_daa).ok_or("claim source Final overflow")?;
    let lock = final_by.checked_add(context.policy.settlement_depth_d).ok_or("claim source lock overflow")?;
    if lock.checked_add(ledger.check_window_daa).is_none_or(|end| end > lifetime.checking_until) {
        return Err("future source does not leave the signed claim checking interval".into());
    }
    let target = misaka_palw_kernel::beacon_source::BeaconSourceTargetV1 {
        terms: misaka_palw_kernel::beacon_source::BeaconSourceTermsV1 {
            context: context.id(),
            seal_start,
            seal_end,
            reveal_end,
            final_by,
            hold_until: lock.max(lifetime.liability_until),
            deposit: source.deposit_sompi,
            max_per_producer: source.max_per_producer,
            max_total: source.max_total,
        },
        context,
    };
    target.validate()?;
    Ok(target)
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidClaimLockV2 {
    pub enrollment: Digest,
    pub context: Digest,
    pub anchor: Digest,
    pub seed: Digest,
    pub locked_daa: u64,
    pub assignment: Digest,
    pub seats: Vec<PalwBondKeyV2>,
    pub quorum: u32,
    pub source_valid: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidClaimFinalV2 {
    pub enrollment: Digest,
    pub base_claim: Digest,
    pub verification_class: Digest,
    pub sample_seed: Digest,
    pub source_context: Digest,
    pub signed_lifetime: PaidClaimLifetimeV2,
    pub complete_receipts: u32,
    pub logical_final_daa: u64,
    pub completed_rounds: u32,
    pub transcript_root: Digest,
}
impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    pub fn paid_claim_authority_v2(&self, context: &Digest) -> Result<PalwPaidClaimAuthorityV2, String> {
        let a: PalwPaidClaimAuthorityV2 =
            self.aux_row(PAID_CLAIM_ENROLLMENTS_V2, &borsh::to_vec(context).unwrap()).ok_or("paid claim authority missing")?;
        a.validate(&self.ledger()?)?;
        if a.context.id() != *context || self.beacon_source_target_v4(context)? != a.source_target {
            return Err("paid claim context/target mismatch".into());
        }
        Ok(a)
    }
    pub fn paid_claim_for_base_v2(&self, claim: &Digest) -> Result<Option<PalwPaidClaimAuthorityV2>, String> {
        for ((table, _), bytes) in &self.aux {
            if *table == PAID_CLAIM_ENROLLMENTS_V2 {
                let a: PalwPaidClaimAuthorityV2 = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
                if a.enrollment.base_claim == *claim {
                    a.validate(&self.ledger()?)?;
                    return Ok(Some(a));
                }
            }
        }
        Ok(None)
    }
    pub fn paid_claim_check_authority_v2(
        &self,
        context: &Digest,
        now: u64,
    ) -> Result<misaka_palw_kernel::claim_paid_v2::PaidClaimCheckAuthorityV2, String> {
        let a = self.paid_claim_authority_v2(context)?;
        let (contract, sources, promises) = self.paid_beacon_material_v5(context)?.ok_or("paid claim source funding missing")?;
        let collected = misaka_palw_challenge::collect_paid_opt_in_work_beacon_v5(&a.context, &contract, &sources, &promises, now)
            .map_err(|e| e.to_string())?;
        let misaka_palw_challenge::SealedBeaconStateV3::Locked(beacon) = collected else {
            return Err("paid claim source has not locked".into());
        };
        let base = self.ledger()?.claim_challenge_subject(&a.enrollment.base_claim).ok_or("base statement missing")?;
        PaidClaimCheckAuthorityV2::verify(
            &a.class,
            &a.enrollment,
            &base,
            &a.context,
            &beacon,
            now,
            paid_checker_snapshot_root_v2(&a.checker_candidates),
        )
    }
}
impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    /// Used by both native fold loads and fresh SDK/root readers. Consumer state
    /// cannot be omitted from a cold restart merely because V1 rows still decode.
    pub fn apply_paid_claim_consumer_inputs_v2(&self, ledger: &mut misaka_palw_kernel::ledger::KernelLedgerV1) -> Result<(), String> {
        let mut inputs = std::collections::BTreeMap::new();
        for ((table, key), bytes) in
            self.aux.range((PAID_CLAIM_ENROLLMENTS_V2, Vec::new())..(PAID_CLAIM_ENROLLMENTS_V2 + 1, Vec::new()))
        {
            let a: PalwPaidClaimAuthorityV2 = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
            if *table != PAID_CLAIM_ENROLLMENTS_V2 || *key != borsh::to_vec(&a.context.id()).unwrap() {
                return Err("paid claim authority key mismatch".into());
            }
            a.validate(ledger)?;
            if ledger
                .claims
                .get(&a.enrollment.base_claim)
                .is_some_and(|row| row.life.final_hold_until > a.enrollment.lifetime.liability_until)
            {
                return Err("paid claim proof grace exceeded its signed absolute lifetime".into());
            }
            let complete = if a.class.interactive_plan.is_some() {
                self.paid_claim_interactive_complete_v3(&a, ledger)?
            } else if let Some(lock) =
                self.aux_row::<PalwPaidClaimLockV2>(PAID_CLAIM_LOCKS_V2, &borsh::to_vec(&a.enrollment.id()).unwrap())
            {
                let mut signers = std::collections::BTreeSet::new();
                for ((_, key), bytes) in self.aux.range((PAID_CLAIM_RECEIPTS_V2, Vec::new())..(PAID_CLAIM_RECEIPTS_V2 + 1, Vec::new()))
                {
                    let receipt: PaidClaimReceiptV2 = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
                    if receipt.enrollment != a.enrollment.id() {
                        continue;
                    }
                    let expected = object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(receipt.enrollment, receipt.signer));
                    if *key != borsh::to_vec(&expected).unwrap()
                        || receipt.version != 2
                        || !receipt.complete
                        || receipt.round != 0
                        || receipt.message_root != [0; 64]
                        || receipt.assignment != lock.assignment
                        || receipt.sample_seed != lock.seed
                        || receipt.source_context != lock.context
                        || receipt.verification_class != a.class.id()
                        || receipt.evidence_root != a.enrollment.commit_root
                        || receipt.transcript_root != a.enrollment.prior_transcript
                        || receipt.signed_daa < lock.locked_daa
                        || !a.enrollment.lifetime.receipt_open(receipt.signed_daa)
                        || !lock.seats.iter().any(|s| crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(s) == receipt.signer)
                        || !signers.insert(receipt.signer)
                    {
                        return Err("paid claim receipt projection is malformed/replayed".into());
                    }
                }
                lock.source_valid && lock.quorum > 0 && signers.len() >= lock.quorum as usize
            } else {
                false
            };
            if inputs.insert(a.enrollment.base_claim, PaidClaimConsumerV2 { lifetime: a.enrollment.lifetime, complete }).is_some() {
                return Err("base claim has multiple verification authorities".into());
            }
        }
        ledger.paid_claim_consumers_v2 = inputs;
        Ok(())
    }
}

impl Params {
    pub fn sync_palw_claim_challenge_v2(&mut self) {
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_claim_challenge_v2(self.palw_claim_challenge_v2);
        }
    }
}

/// The consumer compares review arithmetic with the actual registered checker,
/// not the source policy's M127 label or a requested repetition count. This
/// conservative bound includes complete plan positions and every typed step.
pub fn paid_claim_actual_algebraic_bound_v2(
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    claim: &Digest,
) -> Result<misaka_palw_challenge::EffectiveBitsV1, String> {
    use misaka_palw_challenge::{CumulativeFailureTermV1, EffectiveBitsV1, cumulative_failure_bits_v1};
    use misaka_palw_kernel::ledger::ClaimBodyV1;
    fn plan(p: &misaka_palw_kernel::plan::VerificationPlanV1) -> EffectiveBitsV1 {
        if p.relations.iter().any(|r| r.checker.is_probabilistic()) {
            EffectiveBitsV1::Bits(p.declared_error_bits)
        } else {
            EffectiveBitsV1::Complete
        }
    }
    let row = ledger.claims.get(claim).ok_or("paid algebraic base claim missing")?;
    match &row.body {
        ClaimBodyV1::Program { .. } => Ok(plan(&ledger.classes.get(&row.class_binding_id).ok_or("paid program class missing")?.plan)),
        ClaimBodyV1::Pipeline { .. } => {
            let p = &ledger.pipeline_classes.get(&row.class_binding_id).ok_or("paid pipeline class missing")?.plan;
            Ok(if p.stages.iter().any(|p| p.relations.iter().any(|r| r.checker.is_probabilistic())) {
                EffectiveBitsV1::Bits(p.declared_error_bits)
            } else {
                EffectiveBitsV1::Complete
            })
        }
        ClaimBodyV1::Segmented { .. } => Ok(EffectiveBitsV1::Complete),
        ClaimBodyV1::Spec(body) => {
            use misaka_palw_kernel::spec::composite::ComponentV1;
            use misaka_palw_kernel::spec::{SpecClaimV1, SpecClassKindV1};
            let c = ledger.typed.classes.get(&row.class_binding_id).ok_or("paid typed class missing")?;
            let terms = match (&body.claim, &c.kind) {
                (SpecClaimV1::Memory(m), SpecClassKindV1::Memory { rule, .. }) => {
                    vec![CumulativeFailureTermV1 { per_opportunity: plan(&rule.plan), opportunities: m.steps.len() as u128 }]
                }
                (SpecClaimV1::Retrieval(_), SpecClassKindV1::Retrieval { .. }) => {
                    vec![CumulativeFailureTermV1 { per_opportunity: EffectiveBitsV1::Complete, opportunities: 1 }]
                }
                (SpecClaimV1::Composite(_), SpecClassKindV1::Composite { components, .. }) => components
                    .iter()
                    .map(|c| CumulativeFailureTermV1 {
                        per_opportunity: match c {
                            ComponentV1::Model(m) => plan(&m.plan),
                            ComponentV1::Retrieval(_) => EffectiveBitsV1::Complete,
                        },
                        opportunities: 1,
                    })
                    .collect(),
                _ => return Err("paid typed checker/claim kind mismatch".into()),
            };
            cumulative_failure_bits_v1(&terms).map_err(|e| e.to_string())
        }
    }
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// Semantic restore fence for the new prospective namespace. Typed resource
    /// preflight happens before decoding; this verifies the meaning of the rows.
    /// It never recollects an expired historical beacon or erases terminal cash.
    pub fn assert_paid_claim_consistency_v3(
        &self,
        params: &crate::palw_state_v2::PalwStateParamsV2,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error;
        let refuse = |s: String| PalwStateV2Error::CarriageInconsistent(format!("paid claim: {s}"));
        let Some(route) = self.kernel_route() else { return Ok(()) };
        let owned = |t: u8| matches!(t,113..=119|122..=124);
        if !route.aux.keys().any(|(t, _)| owned(*t)) {
            return Ok(());
        }
        let fence = params.palw_claim_challenge_v2.ok_or_else(|| refuse("new paid namespace without compiled capability".into()))?;
        fence.policy.validate().map_err(&refuse)?;
        let now = self.last_point().map_or(0, |p| p.daa_score);
        if !fence.activation.is_active(now) {
            return Err(refuse("paid rows precede their compiled activation".into()));
        }
        let l = route.ledger().map_err(&refuse)?;
        let p = fence.policy;
        let count = |table: u8| route.aux.keys().filter(|(t, _)| *t == table).count();
        if count(PAID_CLAIM_CLASSES_V2) > p.maximum_classes as usize || count(PAID_CLAIM_JOBS_V2) > p.maximum_jobs as usize {
            return Err(refuse("retained class/job capacity exceeded".into()));
        }
        let mut live = 0u32;
        for ((table, key), bytes) in &route.aux {
            if !owned(*table) {
                continue;
            }
            let id: Digest = borsh::from_slice(key).map_err(|e| refuse(format!("aux key: {e}")))?;
            match *table {
                PAID_CLAIM_CLASSES_V2 => {
                    let c: PaidClaimClassV2 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    c.validate().map_err(&refuse)?;
                    let target = if let Some(base) = l.classes.get(&c.base_class) {
                        if crate::palw_claim_challenge_v2::is_segmented_descriptor_v3(&base.descriptor.digest()) {
                            PaidClaimTargetV2::Segmented
                        } else {
                            PaidClaimTargetV2::Program
                        }
                    } else if l.pipeline_classes.contains_key(&c.base_class) {
                        PaidClaimTargetV2::Pipeline
                    } else if l.typed.classes.contains_key(&c.base_class) {
                        PaidClaimTargetV2::TypedSpec
                    } else {
                        return Err(refuse("paid class lost its mathematical base".into()));
                    };
                    if id != c.id()
                        || c.registrant == [0; 64]
                        || c.target != target
                        || c.checker_contract != p.checker_contract
                        || c.check_window_daa < l.policy.check_window_daa
                        || c.challenge_window_daa
                            < if l.mode_of_class(&c.base_class).is_optimistic() {
                                l.opv.policy.map_or(l.policy.challenge_window_daa, |p| p.window_daa())
                            } else {
                                l.policy.challenge_window_daa
                            }
                        || l.policy
                            .court_deadline_daa
                            .checked_add(l.policy.proof_grace_daa)
                            .is_none_or(|tail| c.court_and_grace_daa < tail)
                        || c.liability_daa < l.classes.get(&c.base_class).map_or(l.policy.liability_daa, |base| base.liability_daa)
                        || c.maximum_rounds > p.maximum_rounds
                        || c.maximum_stream_blocks > p.maximum_stream_blocks
                        || c.review_scope != (p.lifetime_review_scope != [0; 64]).then_some(p.lifetime_review_scope)
                        || l.mode_of_class(&c.base_class).is_optimistic() != matches!(c.checking, PaidClaimCheckingV2::ProspectiveOpv)
                    {
                        return Err(refuse("class key/policy/target/prospective mode substituted".into()));
                    }
                }
                PAID_CLAIM_JOBS_V2 => {
                    let j: PaidClaimJobV2 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    let c: PaidClaimClassV2 = route
                        .aux_row(PAID_CLAIM_CLASSES_V2, &borsh::to_vec(&j.class).unwrap())
                        .ok_or_else(|| refuse("job class absent".into()))?;
                    let class = l
                        .jobs
                        .get(&j.base_job)
                        .map(|j| j.class_binding_id)
                        .or_else(|| l.pipeline_jobs.get(&j.base_job).map(|j| j.class_binding_id))
                        .or_else(|| l.tiled_jobs.get(&j.base_job).map(|j| j.job.class_binding_id))
                        .or_else(|| l.typed.jobs.get(&j.base_job).map(|j| j.class()));
                    if id != j.id()
                        || j.version != 2
                        || class != Some(c.base_class)
                        || l.job_poster(&j.base_job) != Some(j.payer)
                        || j.maximum_consumers == 0
                        || j.maximum_consumers > p.maximum_enrollments_per_job
                    {
                        return Err(refuse("job key/payer/mathematical class/capacity substituted".into()));
                    }
                    let mut admitted = 0u32;
                    for ((t, _), v) in &route.aux {
                        if *t == PAID_CLAIM_ENROLLMENTS_V2 {
                            let a: PalwPaidClaimAuthorityV2 = borsh::from_slice(v).map_err(|e| refuse(e.to_string()))?;
                            if a.job.id() == j.id() {
                                admitted = admitted.checked_add(1).ok_or_else(|| refuse("job admission overflow".into()))?;
                            }
                        }
                    }
                    if admitted > j.maximum_consumers {
                        return Err(refuse("job lifetime admission budget exceeded".into()));
                    }
                }
                PAID_CLAIM_ENROLLMENTS_V2 => {
                    let a: PalwPaidClaimAuthorityV2 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    a.validate(&l).map_err(&refuse)?;
                    let c: PaidClaimClassV2 = route
                        .aux_row(PAID_CLAIM_CLASSES_V2, &borsh::to_vec(&a.class.id()).unwrap())
                        .ok_or_else(|| refuse("enrollment class absent".into()))?;
                    let j: PaidClaimJobV2 = route
                        .aux_row(PAID_CLAIM_JOBS_V2, &borsh::to_vec(&a.job.id()).unwrap())
                        .ok_or_else(|| refuse("enrollment job absent".into()))?;
                    if id != a.context.id()
                        || a.class != c
                        || a.job != j
                        || !fence.activation.is_active(a.authorized_daa)
                        || a.authorized_daa > now
                        || a.enrollment.round != 0
                        || a.enrollment.prior_transcript != [0; 64]
                        || a.authorized_daa > j.last_commit_daa
                    {
                        return Err(refuse("enrollment copied policy/job/activation/signed interval substituted".into()));
                    }
                    if now <= a.enrollment.lifetime.liability_until {
                        live = live.checked_add(1).ok_or_else(|| refuse("live admission overflow".into()))?;
                    }
                }
                PAID_CLAIM_LOCKS_V2 => {
                    let lock: PalwPaidClaimLockV2 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    let a = route.paid_claim_enrollment_v3(&lock.enrollment).map_err(&refuse)?;
                    if id != a.enrollment.id()
                        || lock.context != a.context.id()
                        || lock.locked_daa > now
                        || !a.enrollment.lifetime.receipt_open(lock.locked_daa)
                        || lock.locked_daa < a.authorized_daa
                        || lock.quorum == 0
                        || lock.seats.len() < lock.quorum as usize
                        || lock.seats.len() > crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_SEATS_V1
                        || a.class.source_policy.interactive_mode == misaka_palw_challenge::InteractiveModeV1::StagedBeacon
                    {
                        return Err(refuse("initial lock replaces enrollment/context/clock/committee".into()));
                    }
                }
                PAID_CLAIM_RECEIPTS_V2 => {
                    let receipt: PaidClaimReceiptV2 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    let a = route.paid_claim_enrollment_v3(&receipt.enrollment).map_err(&refuse)?;
                    if id != object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(receipt.enrollment, receipt.signer))
                        || a.class.interactive_plan.is_some()
                        || receipt.signed_daa > now
                    {
                        return Err(refuse("receipt key/time/interactive routing substituted".into()));
                    }
                    // Full assignment/seed/statement/unique signer projection was
                    // repeated by route.ledger() above, including cold restore.
                }
                PAID_CLAIM_EVIDENCE_V2 => {
                    let e: PalwPaidClaimEvidenceV3 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    e.validate().map_err(&refuse)?;
                    if id != e.id() {
                        return Err(refuse("evidence key/kind/version identity mismatch".into()));
                    }
                }
                PAID_CLAIM_REVIEW_SCOPES_V2 => {
                    let scope: misaka_palw_challenge::approval_scope::LifetimeReviewScopeV3 =
                        borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    scope.validate().map_err(&refuse)?;
                    if id != scope.id()
                        || id != p.lifetime_review_scope
                        || scope.chain != route.header.policy.network_domain
                        || scope.ruleset != route.header.policy.ruleset_digest
                        || scope.source_independence_root != p.source_independence_evidence
                        || scope.economic_evidence_root != p.economic_evidence
                    {
                        return Err(refuse("review scope replaces compiled chain/evidence authority".into()));
                    }
                }
                PAID_CLAIM_REVIEW_COUNTERS_V2 => {
                    let counter: misaka_palw_challenge::approval_scope::LifetimeCounterV3 =
                        borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    if id != object_id(b"MISAKA/PALW/REVIEW-LIFETIME-COUNTER/V3", &route.header.policy.network_domain)
                        || counter.admissions != counter.admitted_events.len() as u128
                    {
                        return Err(refuse("lifetime counter key/admission history substituted".into()));
                    }
                }
                PAID_CLAIM_ROUNDS_V2 => {
                    let row: crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3 =
                        borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    let a = route.paid_claim_enrollment_v3(&row.enrollment).map_err(&refuse)?;
                    row.validate(&a, &l).map_err(&refuse)?;
                    if id != row.id()
                        || row.accepted_daa > now
                        || row.lock.as_ref().is_some_and(|lock| lock.locked_daa > now)
                        || row.receipts.iter().any(|r| r.signed_daa > now)
                    {
                        return Err(refuse("round key/native timeline mismatch".into()));
                    }
                }
                PAID_CLAIM_FINALS_V2 => {
                    let row: PalwPaidClaimFinalV2 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    let a = route.paid_claim_enrollment_v3(&row.enrollment).map_err(&refuse)?;
                    let claim = l.claims.get(&a.enrollment.base_claim).ok_or_else(|| refuse("Final base claim missing".into()))?;
                    if id != row.enrollment
                        || row.base_claim != a.enrollment.base_claim
                        || row.verification_class != a.class.id()
                        || row.signed_lifetime != a.enrollment.lifetime
                        || row.logical_final_daa > now
                        || row.logical_final_daa > a.enrollment.lifetime.liability_until
                        || row.logical_final_daa != a.enrollment.lifetime.dispute_until.max(claim.life.final_hold_until)
                        || !matches!(claim.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. })
                    {
                        return Err(refuse("Final canonical payment/statement/clock mismatch".into()));
                    }
                    let (lock, rounds, transcript) = if let Some(plan) = &a.class.interactive_plan {
                        if !route.paid_claim_interactive_complete_v3(&a, &l).map_err(&refuse)? {
                            return Err(refuse("Final has incomplete interactive plan".into()));
                        }
                        let last = route.paid_claim_round_v3(&a.enrollment.id(), plan.rounds - 1).map_err(&refuse)?;
                        (last.lock.ok_or_else(|| refuse("Final round lock absent".into()))?, plan.rounds, last.transcript_root)
                    } else {
                        (
                            route
                                .aux_row::<PalwPaidClaimLockV2>(PAID_CLAIM_LOCKS_V2, &borsh::to_vec(&a.enrollment.id()).unwrap())
                                .ok_or_else(|| refuse("Final initial lock absent".into()))?,
                            1,
                            [0; 64],
                        )
                    };
                    if row.completed_rounds != rounds
                        || row.transcript_root != transcript
                        || row.sample_seed != lock.seed
                        || row.source_context != lock.context
                        || row.complete_receipts != lock.quorum
                        || !lock.source_valid
                    {
                        return Err(refuse("Final seed/transcript/full-plan coverage mismatch".into()));
                    }
                }
                _ => unreachable!("owned namespace match"),
            }
        }
        if live > p.maximum_live_claims {
            return Err(refuse("live paid capacity exceeded".into()));
        }
        Ok(())
    }
}
fn is_segmented_descriptor_v3(id: &Digest) -> bool {
    *id == misaka_palw_kernel::descriptor::k2_tir_v4_descriptor().digest()
        || *id == misaka_palw_kernel::descriptor::k2_tir_v5_descriptor().digest()
}
