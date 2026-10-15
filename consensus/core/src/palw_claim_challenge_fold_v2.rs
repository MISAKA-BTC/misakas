//! Native signed consent and atomic paid source funding for every claim body.
use super::palw_kernel_route_fold_v1 as route_fold;
use super::*;
use crate::palw_claim_challenge_v2::*;
use crate::palw_kernel_route_v1::{
    PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1, PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1, palw_kernel_bond_id_v1,
};
use misaka_palw_kernel::claim_paid_v2::*;
use misaka_palw_kernel::hash::{Digest, object_id};
use misaka_palw_kernel::route::{KernelRouteObjectV1, SaltedCommitV1};
fn refused(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(s.into())
}
fn key(id: &Digest) -> Vec<u8> {
    borsh::to_vec(id).unwrap()
}
fn put<T: borsh::BorshSerialize>(b: &mut TransitionBuilder<'_>, table: u8, id: &Digest, v: &T) {
    b.write_kernel_row(table, key(id), Some(borsh::to_vec(v).unwrap()));
}
fn count(b: &TransitionBuilder<'_>, table: u8) -> usize {
    b.state.kernel_route.as_ref().map_or(0, |r| r.aux.keys().filter(|(t, _)| *t == table).count())
}
fn authority(b: &TransitionBuilder<'_>, id: &Digest) -> Result<PalwPaidClaimAuthorityV2, PalwStateV2Error> {
    let r = b.state.kernel_route.as_ref().ok_or_else(|| refused("paid claim route missing"))?;
    let a = r.paid_claim_enrollment_v3(id).map_err(refused)?;
    a.validate(&*r.ledger().map_err(refused)?).map_err(refused)?;
    Ok(a)
}

fn source_fences(
    b: &TransitionBuilder<'_>,
    now: u64,
) -> Result<
    (crate::palw_beacon_source_opt_in_v1::PalwBeaconSourceFenceV1, crate::palw_beacon_source_fee_v1::PalwBeaconSourceFeeFenceV1),
    PalwStateV2Error,
> {
    let e = b.extras.kernel_route.as_ref().ok_or_else(|| refused("paid claim native route missing"))?;
    Ok((
        e.beacon_source.filter(|f| f.activation.is_active(now)).ok_or_else(|| refused("paid claim source fence missing"))?,
        e.beacon_source_fee.filter(|f| f.activation.is_active(now)).ok_or_else(|| refused("paid claim fee fence missing"))?,
    ))
}
fn class_target(l: &misaka_palw_kernel::ledger::KernelLedgerV1, class: &Digest) -> Option<PaidClaimTargetV2> {
    if let Some(c) = l.classes.get(class) {
        Some(
            if c.descriptor.digest() == misaka_palw_kernel::descriptor::k2_tir_v4_descriptor().digest()
                || c.descriptor.digest() == misaka_palw_kernel::descriptor::k2_tir_v5_descriptor().digest()
            {
                PaidClaimTargetV2::Segmented
            } else {
                PaidClaimTargetV2::Program
            },
        )
    } else if l.pipeline_classes.contains_key(class) {
        Some(PaidClaimTargetV2::Pipeline)
    } else if l.typed.classes.contains_key(class) {
        Some(PaidClaimTargetV2::TypedSpec)
    } else {
        None
    }
}
fn commit_id(c: &SaltedCommitV1) -> Digest {
    match c {
        SaltedCommitV1::Claim { claim, .. } | SaltedCommitV1::Segmented { claim, .. } => claim.id(),
        SaltedCommitV1::Pipeline { claim, .. } => claim.id(),
        SaltedCommitV1::Spec { claim } => claim.id(),
    }
}
fn commit_job(c: &SaltedCommitV1) -> Digest {
    match c {
        SaltedCommitV1::Claim { claim, .. } | SaltedCommitV1::Segmented { claim, .. } => claim.job_id,
        SaltedCommitV1::Pipeline { claim, .. } => claim.job_id,
        SaltedCommitV1::Spec { claim } => claim.job_id(),
    }
}
fn current_policy(b: &TransitionBuilder<'_>, now: u64) -> Result<PalwClaimChallengePolicyV2, PalwStateV2Error> {
    let f = b
        .params
        .palw_claim_challenge_v2
        .filter(|f| f.activation.is_active(now))
        .ok_or_else(|| refused("paid claim capability is dormant"))?;
    f.policy.validate().map_err(refused)?;
    Ok(f.policy)
}
/// All mutations share a checkpoint, including a nested original monetary fold.
/// A refused quote/consent/receipt never leaves funding or old claim state behind.
pub(super) fn apply_paid_claim_v2(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    o: &PaidClaimObjectV2,
) -> Result<(), PalwStateV2Error> {
    let checkpoint = b.try_checkpoint()?;
    let result = apply_inner(b, ctx, signer, o);
    if result.is_err() {
        b.restore(checkpoint);
    }
    result
}
fn apply_inner(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    o: &PaidClaimObjectV2,
) -> Result<(), PalwStateV2Error> {
    let p = current_policy(b, ctx.daa_score)?;
    route_fold::ensure_route_header(b, ctx)?;
    let signer_id = palw_kernel_bond_id_v1(signer);
    // New proof/registration data consumes the existing native block work
    // admission once; old V3 objects preserve their prior accounting.
    let admission_work =
        if matches!(o, PaidClaimObjectV2::RegisterCompactContractV4 { .. } | PaidClaimObjectV2::BindCompactProofV4 { .. }) {
            u64::try_from(borsh::to_vec(o).map_err(|e| refused(e.to_string()))?.len())
                .map_err(|_| refused("compact native work length overflow"))?
        } else {
            0
        };
    if !route_fold::charge_route_budget_v1(b, ctx, admission_work, false)? {
        return Err(refused("paid claim block admission capacity exhausted"));
    }
    match o {
        PaidClaimObjectV2::RegisterCompactContractV4 { registration } => {
            use crate::palw_claim_compact_v4::*;
            let fence = b
                .params
                .palw_claim_compact_v4
                .filter(|f| f.activation.is_active(ctx.daa_score))
                .ok_or_else(|| refused("compact V4 own fence is dormant"))?;
            fence.policy.validate().map_err(refused)?;
            let r = b.state.kernel_route.as_ref().unwrap();
            if registration.registrant != signer_id
                || r.aux.contains_key(&(PAID_CLAIM_COMPACT_V4, key(&registration.id())))
                || r.aux.keys().filter(|(t, k)| *t == PAID_CLAIM_COMPACT_V4 && k.len() == 64).count()
                    >= fence.policy.max_contracts as usize
            {
                return Err(refused("compact contract registrant/duplicate/capacity mismatch"));
            }
            let l = r.ledger().map_err(refused)?;
            registration.validate(&l).map_err(refused)?;
            if registration.plan.maximum_material_bytes > fence.policy.maximum_opening_bytes
                || registration.contract.maximum_proof_bytes > u128::from(fence.policy.maximum_proof_bytes)
                || registration.contract.maximum_verifier_work > fence.policy.maximum_verifier_work
            {
                return Err(refused("compact contract exceeds native policy capacities"));
            }
            let empty = misaka_palw_kernel::claim_compact_v1::native_v4::NativeCompactProofV4 {
                version: 4,
                plan: registration.plan.id(),
                seed: [0; 64],
                proofs: Vec::new(),
            };
            let framing = KernelRouteObjectV1::PaidClaimV2 {
                object: PaidClaimObjectV2::BindCompactProofV4 { enrollment: [0; 64], round: 0, proof: empty.clone() },
            }
            .encode()
            .len()
            .checked_sub(borsh::to_vec(&empty).unwrap().len())
            .ok_or_else(|| refused("proof carrier framing underflow"))?;
            if registration.contract.maximum_proof_bytes.checked_add(framing as u128).is_none_or(|n| {
                n > crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 as u128
                    || n > misaka_palw_kernel::route::max_encoded_bytes_of_tag(misaka_palw_kernel::route::TAG_PAID_CLAIM_V2)
                        .unwrap_or(0) as u128
            }) {
                return Err(refused("whole-native compact proof does not fit actual object carriage"));
            }
            put(
                b,
                PAID_CLAIM_COMPACT_V4,
                &registration.id(),
                &PalwPaidCompactContractV4 { registration: registration.clone(), registered_daa: ctx.daa_score },
            );
        }
        PaidClaimObjectV2::BindCompactProofV4 { enrollment, round, proof } => {
            use crate::palw_claim_compact_v4::*;
            let a = authority(b, enrollment)?;
            if !b.params.palw_claim_compact_v4.is_some_and(|f| f.activation.is_active(ctx.daa_score)) {
                return Err(refused("compact V4 own fence is dormant"));
            }
            let r = b.state.kernel_route.as_ref().unwrap();
            let plan = a
                .class
                .interactive_plan
                .as_ref()
                .filter(|p| p.version == 4)
                .ok_or_else(|| refused("proof binding needs V4 enrolled plan"))?;
            let row = r.paid_claim_round_v3(enrollment, *round).map_err(refused)?;
            let lock = row.lock.as_ref().ok_or_else(|| refused("proof binding precedes source lock"))?;
            let proof_root = proof.root();
            let registration = r.paid_compact_contract_v4(&plan.id()).map_err(refused)?.registration;
            let policy = b.params.palw_claim_compact_v4.unwrap().policy;
            proof.validate_shape(plan.id(), lock.seed, policy.maximum_proof_bytes).map_err(refused)?;
            if borsh::to_vec(proof).map_err(|e| refused(e.to_string()))?.len() as u128 > registration.contract.maximum_proof_bytes {
                return Err(refused("proof bytes exceed signed whole-native contract"));
            }
            if a.enrollment.producer != signer_id
                || proof_root == [0; 64]
                || !row.receipts.is_empty()
                || ctx.daa_score < lock.locked_daa
                || !a.enrollment.lifetime.receipt_open(ctx.daa_score)
                || r.paid_compact_proof_binding_v4(*enrollment, *round).map_err(refused)?.is_some()
            {
                return Err(refused("compact producer proof binding signer/order/lifetime/duplicate mismatch"));
            }
            let binding = PalwPaidCompactProofBindingV4 {
                version: 4,
                enrollment: *enrollment,
                round: *round,
                plan: plan.id(),
                seed: lock.seed,
                proof_root,
                proof: proof.clone(),
                producer: signer_id,
                bound_daa: ctx.daa_score,
            };
            let mut bound = row.clone();
            bound.transcript_root =
                misaka_palw_kernel::claim_compact_v1::native_v4::compact_transcript_root_v4(row.transcript_root, proof_root);
            bound.validate_with_compact_v4(&a, &r.ledger().map_err(refused)?, Some(proof_root)).map_err(refused)?;
            b.write_kernel_row(
                PAID_CLAIM_COMPACT_V4,
                compact_proof_binding_key_v4(*enrollment, *round),
                Some(borsh::to_vec(&binding).unwrap()),
            );
            put(b, PAID_CLAIM_ROUNDS_V2, &bound.id(), &bound);
        }
        PaidClaimObjectV2::RegisterClass { class } => {
            class.validate().map_err(refused)?;
            let l = route_fold::load_ledger(b, ctx)?;
            if let Some(plan) = class.interactive_plan.as_ref().filter(|p| p.version == 4) {
                let fence = b
                    .params
                    .palw_claim_compact_v4
                    .filter(|f| f.activation.is_active(ctx.daa_score))
                    .ok_or_else(|| refused("compact V4 own fence is dormant"))?;
                let registration = b.state.kernel_route.as_ref().unwrap().paid_compact_contract_v4(&plan.id()).map_err(refused)?;
                registration.registration.validate_for_class(class, &l).map_err(refused)?;
                if fence.policy.require_independent_review {
                    misaka_palw_challenge::approved_v2(
                        &misaka_palw_challenge::shipped_registry_v2(),
                        &registration.registration.plan.compiler,
                        &class.source_policy,
                    )
                    .map_err(|e| refused(e.to_string()))?;
                }
            }
            let minimum_liability = l.classes.get(&class.base_class).map_or(l.policy.liability_daa, |c| c.liability_daa);
            let minimum_challenge = if l.mode_of_class(&class.base_class).is_optimistic() {
                l.opv.policy.map_or(l.policy.challenge_window_daa, |p| p.window_daa())
            } else {
                l.policy.challenge_window_daa
            };
            if class.registrant != signer_id
                || class.checker_contract != p.checker_contract
                || class.maximum_stream_blocks > p.maximum_stream_blocks
                || class.maximum_rounds > p.maximum_rounds
                || class_target(&l, &class.base_class) != Some(class.target)
                || class.check_window_daa < l.policy.check_window_daa
                || class.challenge_window_daa < minimum_challenge
                || class.court_and_grace_daa
                    < l.policy
                        .court_deadline_daa
                        .checked_add(l.policy.proof_grace_daa)
                        .ok_or_else(|| refused("claim floor overflow"))?
                || class.liability_daa < minimum_liability
                || count(b, PAID_CLAIM_CLASSES_V2) >= p.maximum_classes as usize
                || class.review_scope != ((p.lifetime_review_scope != [0; 64]).then_some(p.lifetime_review_scope))
            {
                return Err(refused("paid class has substituted registrant/target/checker/review or inadequate clocks/capacity"));
            }
            let opv = l.mode_of_class(&class.base_class).is_optimistic();
            if opv != matches!(class.checking, PaidClaimCheckingV2::ProspectiveOpv) {
                return Err(refused("OPV additional checking needs an explicitly enrolled prospective class"));
            }
            if b.state.kernel_route.as_ref().unwrap().aux.contains_key(&(PAID_CLAIM_CLASSES_V2, key(&class.id()))) {
                return Err(refused("paid class already enrolled"));
            }
            put(b, PAID_CLAIM_CLASSES_V2, &class.id(), class);
        }
        PaidClaimObjectV2::EnrollJob { job } => {
            let l = route_fold::load_ledger(b, ctx)?;
            let r = b.state.kernel_route.as_ref().unwrap();
            let c: PaidClaimClassV2 =
                r.aux_row(PAID_CLAIM_CLASSES_V2, &key(&job.class)).ok_or_else(|| refused("paid job class not enrolled"))?;
            let baseclass = l
                .jobs
                .get(&job.base_job)
                .map(|j| j.class_binding_id)
                .or_else(|| l.pipeline_jobs.get(&job.base_job).map(|j| j.class_binding_id))
                .or_else(|| l.tiled_jobs.get(&job.base_job).map(|j| j.job.class_binding_id))
                .or_else(|| l.typed.jobs.get(&job.base_job).map(|j| j.class()));
            if job.version != 2
                || job.payer != signer_id
                || l.job_poster(&job.base_job) != Some(signer_id)
                || baseclass != Some(c.base_class)
                || job.last_commit_daa <= ctx.daa_score
                || job.maximum_consumers == 0
                || job.maximum_consumers > p.maximum_enrollments_per_job
                || count(b, PAID_CLAIM_JOBS_V2) >= p.maximum_jobs as usize
                || !misaka_palw_kernel::claim_paid_v2::paid_job_enrollment_is_prospective_v3(&l, &job.base_job, ctx.daa_score)
                || r_has_job(b, &job.base_job)?
            {
                return Err(refused("paid job needs its actual payer's prospective consent before any claim"));
            }
            put(b, PAID_CLAIM_JOBS_V2, &job.id(), job);
        }
        PaidClaimObjectV2::Commit { enrollment: e, commit, salt } => {
            let l = route_fold::load_ledger(b, ctx)?;
            let r = b.state.kernel_route.as_ref().unwrap();
            let j: PaidClaimJobV2 =
                r.aux_row(PAID_CLAIM_JOBS_V2, &key(&e.job)).ok_or_else(|| refused("paid claim payer enrollment missing"))?;
            let c: PaidClaimClassV2 =
                r.aux_row(PAID_CLAIM_CLASSES_V2, &key(&j.class)).ok_or_else(|| refused("paid claim registrant consent missing"))?;
            let used = r
                .aux
                .iter()
                .filter(|((t, key), _)| *t == PAID_CLAIM_ENROLLMENTS_V2 && key.len() == 64)
                .map(|(_, v)| borsh::from_slice::<PalwPaidClaimAuthorityV2>(v))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| refused(e.to_string()))?;
            let live = used.iter().filter(|a| ctx.daa_score <= a.enrollment.lifetime.liability_until).count();
            if e.producer != signer_id
                || commit.producer() != signer_id
                || commit_id(commit) != e.base_claim
                || commit_job(commit) != j.base_job
                || ctx.daa_score > j.last_commit_daa
                || e.class != c.id()
                || e.lifetime != PaidClaimLifetimeV2::of(&c, e.lifetime.committed_daa).map_err(refused)?
                || ctx.daa_score < e.lifetime.committed_daa
                || ctx.daa_score > e.accepted_until_daa
                || e.lifetime.committed_daa.checked_add(c.maximum_commit_delay_daa) != Some(e.accepted_until_daa)
                || e.round != 0
                || e.prior_transcript != [0; 64]
                || live >= p.maximum_live_claims as usize
                || used.iter().filter(|a| a.job.id() == j.id()).count() >= j.maximum_consumers as usize
                || l.claims.contains_key(&e.base_claim)
                || l.beacon_sources.values().any(|source| source.job == j.base_job && source.outcome.is_none())
            {
                return Err(refused("paid producer enrollment is late, replayed, unsigned or exceeds capacity"));
            }
            b.state.require_monetary_material_enrollment_v1(b.params, Hash64::from_bytes(e.base_claim), ctx.daa_score, None)?;
            let monetary = b
                .state
                .monetary_material_row_v1(Hash64::from_bytes(e.base_claim))
                .ok_or_else(|| refused("paid claim signed monetary material consent missing"))?;
            if monetary.consent.as_bytes() != e.material_consent {
                return Err(refused("paid claim names another signed material consent"));
            }
            let (sf, _) = source_fences(b, ctx.daa_score)?;
            let eligible = match b.extras.kernel_route.as_ref().and_then(|x| x.opv.as_ref()) {
                Some(opv) => r
                    .opv_eligible_set_v1(&l, ctx.daa_score, &crate::palw_opv_bootstrap_v1::OpvEligibilityViewV1::of(opv))
                    .into_iter()
                    .filter(|id| id.as_bytes() != c.base_class)
                    .map(|id| id.as_bytes())
                    .collect(),
                None => Default::default(),
            };
            let inner = KernelRouteObjectV1::CommitClaimSalted { commit: commit.clone(), salt: *salt };
            route_fold::apply_kernel_route_object_authorized_v2(b, ctx, &inner.encode(), signer, true)?;
            let mut l = route_fold::load_ledger(b, ctx)?;
            let base = l.claim_challenge_subject(&e.base_claim).ok_or_else(|| refused("native base claim admission refused"))?;
            let producer_operator = b.state.bonds.get(signer).map(|v| v.operator_id);
            let checker_candidates: Vec<_> = b
                .state
                .bonds
                .iter()
                .filter(|(key, bond)| {
                    **key != *signer
                        && Some(bond.operator_id) != producer_operator
                        && matches!(bond.status, PalwBondStatusV2::Active)
                        && bond.collateral >= b.params.min_collateral_sompi()
                })
                .map(|(bond, v)| PalwPaidCheckerCandidateV2 {
                    bond: *bond,
                    operator: v.operator_id,
                    weight: (v.collateral / crate::constants::SOMPI_PER_KASPA).clamp(1, 1_000_000),
                })
                .collect();
            if checker_candidates.len() > 65_536 || checker_candidates.is_empty() {
                return Err(refused("paid checker snapshot has no finite eligible committee"));
            }
            let subject = paid_subject_with_checker_snapshot_v2(&base, &c, e, paid_checker_snapshot_root_v2(&checker_candidates))
                .map_err(refused)?;
            let context = paid_context_v2(&subject, &c, e, eligible, ctx.daa_score);
            let target = claim_source_target_v2(sf.policy, context.clone(), e.lifetime, &l.policy).map_err(refused)?;
            let a = PalwPaidClaimAuthorityV2 {
                version: 2,
                enrollment: e.clone(),
                class: c,
                job: j,
                subject,
                context,
                source_target: target,
                source_policy: sf.policy,
                authorized_daa: ctx.daa_score,
                producer: *signer,
                sponsor: None,
                checker_candidates,
            };
            a.validate(&l).map_err(refused)?;
            let before = l.to_rows();
            let row = l.claims.get_mut(&e.base_claim).unwrap();
            row.life.state = misaka_palw_kernel::lifecycle::ClaimStateV1::Checking {
                anchor_daa: ctx.daa_score,
                deadline_daa: e.lifetime.checking_until,
            };
            row.life.final_hold_until = e.lifetime.dispute_until;
            row.liability_until = Some(e.lifetime.liability_until);
            extend_signed_kernel_income_v2(b, ctx, e.base_claim, *signer, e.lifetime.liability_until)?;
            put(b, PAID_CLAIM_ENROLLMENTS_V2, &a.context.id(), &a);
            for (prefix, kind, owner) in [
                (PAID_CLAIM_ID_INDEX_PREFIX_V3, PalwPaidClaimIndexKindV3::Enrollment, e.id()),
                (PAID_CLAIM_BASE_INDEX_PREFIX_V3, PalwPaidClaimIndexKindV3::BaseClaim, e.base_claim),
            ] {
                b.write_kernel_row(
                    PAID_CLAIM_ENROLLMENTS_V2,
                    paid_claim_index_key_v3(prefix, &owner),
                    Some(borsh::to_vec(&PalwPaidClaimIndexV3 { version: 3, kind, owner, target: a.context.id() }).unwrap()),
                );
            }
            // Interim V1 receipts cannot cover this new verification identity.
            b.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1, key(&e.base_claim), None);
            b.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1, key(&e.base_claim), None);
            route_fold::flush(b, &l, &before);
            admit_review(
                b,
                ctx,
                &p,
                &a,
                e.id(),
                u32::from(a.class.source_policy.interactive_mode != misaka_palw_challenge::InteractiveModeV1::StagedBeacon),
            )?;
        }
        PaidClaimObjectV2::Fund { enrollment, terms } => {
            let mut a = authority(b, enrollment)?;
            if a.class.source_policy.interactive_mode == misaka_palw_challenge::InteractiveModeV1::StagedBeacon {
                return Err(refused("staged funding requires a signed round message/future target"));
            }
            let (sf, ff) = source_fences(b, ctx.daa_score)?;
            let cap = ff.policy.max_funded_sources.min(a.source_target.terms.max_total);
            let minimum = ff.policy.minimum_fee_for(&a.source_target).map_err(refused)?;
            if a.sponsor.is_some()
                || ctx.daa_score >= a.source_target.terms.seal_start
                || sf.policy != a.source_policy
                || terms.sponsor != signer_id
                || terms.source != a.source_target.terms
                || terms.fee_per_source < minimum
                || terms.max_sources < a.class.source_policy.work_count_k
                || terms.max_sources > cap
            {
                return Err(refused("paid claim source funding is late, replayed, substituted or inadequate"));
            }
            a.sponsor = Some(*signer);
            let id = a.context.id();
            put(b, PAID_CLAIM_ENROLLMENTS_V2, &id, &a);
            put(b, crate::palw_beacon_source_opt_in_v1::PALW_BEACON_SOURCE_TABLE_TARGETS_V4, &id, &a.source_target);
            put(
                b,
                crate::palw_beacon_source_fee_v1::PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5,
                &id,
                &misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTargetV1 {
                    target: a.source_target.clone(),
                    sponsor: signer_id,
                    minimum_fee: minimum,
                    max_sources: cap,
                },
            );
            put(
                b,
                crate::palw_beacon_source_fee_v1::PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5,
                &id,
                &ff.policy.cost_bounds.ok_or_else(|| refused("paid claim cost scope missing"))?,
            );
            let inner = KernelRouteObjectV1::FundBeaconSourceFee { terms: *terms };
            route_fold::apply_kernel_route_object_v1(b, ctx, &inner.encode(), signer)?;
            let l = b.state.kernel_route.as_ref().unwrap().ledger().map_err(refused)?;
            if !l.beacon_source_fees.get(&id).is_some_and(|f| f.terms == *terms) {
                return Err(refused("strict native prepaid source debit failed"));
            }
        }
        PaidClaimObjectV2::Lock { enrollment } => lock(b, ctx, enrollment)?,
        PaidClaimObjectV2::Receipt { receipt } => receipt_fold(b, ctx, signer, receipt)?,
        PaidClaimObjectV2::ReviewScope { scope } => {
            scope.validate().map_err(refused)?;
            if signer_id != p.review_authority
                || scope.id() != p.lifetime_review_scope
                || scope.chain != b.state.kernel_route.as_ref().unwrap().header.policy.network_domain
                || scope.ruleset != b.state.kernel_route.as_ref().unwrap().header.policy.ruleset_digest
                || scope.source_independence_root != p.source_independence_evidence
                || scope.economic_evidence_root != p.economic_evidence
            {
                return Err(refused("review is not authenticated by the release scope/authority"));
            }
            let r = b.state.kernel_route.as_ref().unwrap();
            let independent: PalwPaidClaimEvidenceV3 = r
                .aux_row(PAID_CLAIM_EVIDENCE_V2, &key(&p.source_independence_evidence))
                .ok_or_else(|| refused("review independence evidence not admitted"))?;
            let economic: PalwPaidClaimEvidenceV3 = r
                .aux_row(PAID_CLAIM_EVIDENCE_V2, &key(&p.economic_evidence))
                .ok_or_else(|| refused("review economic evidence not admitted"))?;
            independent.validate().map_err(refused)?;
            economic.validate().map_err(refused)?;
            if !matches!(independent, PalwPaidClaimEvidenceV3::Independence { .. })
                || independent.id() != p.source_independence_evidence
                || !matches!(economic, PalwPaidClaimEvidenceV3::Economic { .. })
                || economic.id() != p.economic_evidence
            {
                return Err(refused("review substitutes evidence envelope kind/identity"));
            }
            if r.aux.contains_key(&(PAID_CLAIM_REVIEW_SCOPES_V2, key(&scope.id()))) {
                return Err(refused("immutable review scope cannot be replaced/reset"));
            }
            put(b, PAID_CLAIM_REVIEW_SCOPES_V2, &scope.id(), scope);
        }
        PaidClaimObjectV2::IndependenceEvidence { observation } => {
            observation.validate().map_err(refused)?;
            if signer_id != p.review_authority
                || observation.id() != p.source_independence_evidence
                || observation.chain != b.state.kernel_route.as_ref().unwrap().header.policy.network_domain
                || observation.until_daa > ctx.daa_score
            {
                return Err(refused("independence evidence lacks the trusted observed release scope"));
            }
            put(
                b,
                PAID_CLAIM_EVIDENCE_V2,
                &observation.id(),
                &PalwPaidClaimEvidenceV3::Independence { version: 3, observation: observation.clone() },
            );
        }
        PaidClaimObjectV2::EconomicEvidence { observation, payment } => {
            observation.net_receipt().map_err(refused)?;
            if signer_id != p.review_authority
                || observation.id() != p.economic_evidence
                || observation.chain != b.state.kernel_route.as_ref().unwrap().header.policy.network_domain
                || observation.measured_until_daa > ctx.daa_score
            {
                return Err(refused("economic report lacks the exact trusted release measurement scope"));
            }
            let bytes = payment.transaction_bytes.iter().try_fold(payment.header_bytes.len(), |total, tx| {
                total.checked_add(tx.len()).ok_or_else(|| refused("payment proof byte overflow"))
            })?;
            if payment.header_bytes.len() > 64 << 10
                || bytes > 8 << 20
                || payment.transaction_bytes.is_empty()
                || payment.transaction_bytes.len() > 4096
            {
                return Err(refused("native economic proof exceeds header/body/count bound"));
            }
            if !route_fold::charge_route_budget_v1(b, ctx, bytes as u64, false)? {
                return Err(refused("native economic proof court-work budget exhausted"));
            }
            verify_source_payment_v3(b, observation, payment)?;
            let r = b.state.kernel_route.as_ref().unwrap();
            if r.aux.contains_key(&(PAID_CLAIM_EVIDENCE_V2, key(&observation.id()))) {
                return Err(refused("economic evidence is immutable and cannot be replayed"));
            }
            put(
                b,
                PAID_CLAIM_EVIDENCE_V2,
                &observation.id(),
                &PalwPaidClaimEvidenceV3::Economic { version: 3, observation: observation.clone() },
            );
        }
        PaidClaimObjectV2::StageMessage { enrollment, round, prior_transcript, message_root } => {
            round_message(b, ctx, signer, &p, enrollment, *round, *prior_transcript, *message_root)?
        }
        PaidClaimObjectV2::FundRound { enrollment, round, terms } => round_fund(b, ctx, signer, enrollment, *round, terms)?,
        PaidClaimObjectV2::LockRound { enrollment, round } => round_lock(b, ctx, enrollment, *round)?,
        PaidClaimObjectV2::RoundReceipt { receipt } => round_receipt(b, ctx, signer, receipt)?,
    }
    b.note_kernel_bond(signer);
    Ok(())
}
fn r_has_job(b: &TransitionBuilder<'_>, base: &Digest) -> Result<bool, PalwStateV2Error> {
    b.state.kernel_route.as_ref().unwrap().paid_claim_job_is_enrolled_v3(base).map_err(refused)
}

fn admit_review(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &PalwClaimChallengePolicyV2,
    a: &PalwPaidClaimAuthorityV2,
    event: Digest,
    beacons: u32,
) -> Result<(), PalwStateV2Error> {
    let Some(id) = a.class.review_scope else { return Ok(()) };
    let r = b.state.kernel_route.as_ref().unwrap();
    let scope: misaka_palw_challenge::approval_scope::LifetimeReviewScopeV3 =
        r.aux_row(PAID_CLAIM_REVIEW_SCOPES_V2, &key(&id)).ok_or_else(|| refused("authenticated review scope missing"))?;
    // One lineage counter for all approved suites/policies on this chain. An
    // upgraded review cannot reset Q merely by obtaining another review id.
    let tuple = scope
        .registry
        .iter()
        .find(|t| t.algebraic.checker_suite_id == a.class.verification_contract_v3() && t.policy == a.class.source_policy)
        .ok_or_else(|| refused("review does not cover this actual checker contract"))?;
    let mut arithmetic = tuple.algebraic.effective_input(&tuple.policy);
    arithmetic.retry_limit = 0;
    arithmetic.adaptive_queries = 1;
    arithmetic.grinding_choices_per_beacon = 1;
    arithmetic.beacons_per_attempt = 1;
    let claimed = misaka_palw_challenge::effective_false_accept_bits_v1(&arithmetic).map_err(|e| refused(e.to_string()))?;
    let actual = if let Some(plan) = a.class.interactive_plan.as_ref().filter(|p| p.version == 4) {
        let registration = r.paid_compact_contract_v4(&plan.id()).map_err(refused)?.registration;
        let bits = registration.validate_for_class(&a.class, &r.ledger().map_err(refused)?).map_err(refused)?.effective_bits;
        let bits = u16::try_from(bits).map_err(|_| refused("compact union bound is nonpositive/unrepresentable"))?;
        misaka_palw_challenge::EffectiveBitsV1::Bits(bits)
    } else {
        paid_claim_actual_algebraic_bound_v2(&*r.ledger().map_err(refused)?, &a.enrollment.base_claim).map_err(refused)?
    };
    if claimed > actual {
        return Err(refused("review arithmetic overstates the actual registered math plan/repetitions/typed composition"));
    }
    let counter_id = object_id(b"MISAKA/PALW/REVIEW-LIFETIME-COUNTER/V3", &a.context.chain_genesis);
    let mut counter: misaka_palw_challenge::approval_scope::LifetimeCounterV3 =
        r.aux_row(PAID_CLAIM_REVIEW_COUNTERS_V2, &key(&counter_id)).unwrap_or_default();
    counter
        .admit(
            &scope,
            &std::collections::BTreeSet::from([p.lifetime_review_scope]),
            a.context.chain_genesis,
            a.context.ruleset_id,
            ctx.daa_score,
            a.class.verification_contract_v3(),
            &a.class.source_policy,
            event,
            beacons,
        )
        .map_err(refused)?;
    put(b, PAID_CLAIM_REVIEW_COUNTERS_V2, &counter_id, &counter);
    Ok(())
}
fn lock(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, enrollment: &Digest) -> Result<(), PalwStateV2Error> {
    let a = authority(b, enrollment)?;
    if a.class.source_policy.interactive_mode == misaka_palw_challenge::InteractiveModeV1::StagedBeacon {
        return Err(refused("staged lock requires a fresh round source"));
    }
    let r = b.state.kernel_route.as_ref().unwrap();
    if r.aux.contains_key(&(PAID_CLAIM_LOCKS_V2, key(enrollment))) {
        return Err(refused("paid claim lock already consumed"));
    }
    let check = r.paid_claim_check_authority_v2(&a.context.id(), ctx.daa_score).map_err(refused)?;
    let (contract, sources, promises) =
        r.paid_beacon_material_v5(&a.context.id()).map_err(refused)?.ok_or_else(|| refused("paid source backing missing"))?;
    let misaka_palw_challenge::SealedBeaconStateV3::Locked(beacon) =
        misaka_palw_challenge::collect_paid_opt_in_work_beacon_v5(&a.context, &contract, &sources, &promises, ctx.daa_score)
            .map_err(|e| refused(e.to_string()))?
    else {
        return Err(refused("future paid beacon unavailable"));
    };
    let mut entries = std::collections::BTreeMap::new();
    for candidate in &a.checker_candidates {
        let rank = crate::palw_kernel_route_v1::palw_kernel_interim_key_v1(
            &Hash64::from_bytes(check.seed()),
            &candidate.operator,
            u128::from(candidate.weight),
        );
        let entry = entries.entry(candidate.operator).or_insert((rank, candidate.bond));
        if (rank, candidate.bond) < *entry {
            *entry = (rank, candidate.bond);
        }
    }
    let mut ranked: Vec<_> = entries.into_values().collect();
    ranked.sort();
    let seats: Vec<_> =
        ranked.into_iter().take(crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_SEATS_V1).map(|(_, k)| k).collect();
    let quorum = u32::from(crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_QUORUM_V1);
    if seats.len() < quorum as usize {
        return Err(refused("paid claim lacks distinct eligible checker operators"));
    }
    let assignment = object_id(b"MISAKA/PALW/PAID-CLAIM-ASSIGNMENT/V2", &(enrollment, check.seed(), &seats, quorum));
    let row = PalwPaidClaimLockV2 {
        enrollment: *enrollment,
        context: a.context.id(),
        anchor: beacon.challenge_anchor,
        seed: check.seed(),
        locked_daa: ctx.daa_score,
        assignment,
        seats,
        quorum,
        source_valid: true,
    };
    put(b, PAID_CLAIM_LOCKS_V2, enrollment, &row);
    Ok(())
}
fn receipt_fold(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    receipt: &PaidClaimReceiptV2,
) -> Result<(), PalwStateV2Error> {
    let a = authority(b, &receipt.enrollment)?;
    if a.class.interactive_plan.is_some() || receipt.round != 0 || receipt.message_root != [0; 64] {
        return Err(refused("interactive receipt needs its round consumer"));
    }
    let r = b.state.kernel_route.as_ref().unwrap();
    let lock: PalwPaidClaimLockV2 =
        r.aux_row(PAID_CLAIM_LOCKS_V2, &key(&receipt.enrollment)).ok_or_else(|| refused("paid receipt future lock missing"))?;
    let current = r.paid_claim_check_authority_v2(&a.context.id(), ctx.daa_score).map_err(refused)?;
    if !lock.source_valid
        || current.seed() != lock.seed
        || receipt.version != 2
        || receipt.signer != palw_kernel_bond_id_v1(signer)
        || !lock.seats.contains(signer)
        || !receipt.complete
        || receipt.signed_daa > ctx.daa_score
        || receipt.signed_daa < lock.locked_daa
        || !a.enrollment.lifetime.receipt_open(ctx.daa_score)
        || receipt.verification_class != a.class.id()
        || receipt.assignment != lock.assignment
        || receipt.source_context != lock.context
        || receipt.sample_seed != lock.seed
        || receipt.evidence_root != a.enrollment.commit_root
        || receipt.transcript_root != a.enrollment.prior_transcript
    {
        return Err(refused("paid receipt does not bind assigned signer/full target/future seed/statement/signed lifetime"));
    }
    let id = object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(receipt.enrollment, receipt.signer));
    if r.aux.contains_key(&(PAID_CLAIM_RECEIPTS_V2, key(&id))) {
        return Err(refused("paid receipt signer already counted"));
    }
    put(b, PAID_CLAIM_RECEIPTS_V2, &id, receipt);
    let mut l = route_fold::load_ledger(b, ctx)?;
    let before = l.to_rows();
    if l.paid_claim_consumers_v2.get(&a.enrollment.base_claim).is_some_and(|c| c.complete) {
        l.try_apply_paid_claim_tally_v2(&a.enrollment.base_claim).map_err(refused)?;
    }
    route_fold::flush(b, &l, &before);
    Ok(())
}
pub(super) fn tick_paid_claim_final_projection_v2(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let Some(r) = b.state.kernel_route.as_ref() else { return Ok(()) };
    let l = r.ledger().map_err(refused)?;
    let mut projections = Vec::new();
    for ((t, row_key), bytes) in &r.aux {
        if *t != PAID_CLAIM_ENROLLMENTS_V2 || row_key.len() != 64 {
            continue;
        }
        let a: PalwPaidClaimAuthorityV2 = borsh::from_slice(bytes).map_err(|e| refused(e.to_string()))?;
        let Some(row) = l.claims.get(&a.enrollment.base_claim) else {
            return Err(refused("paid claim lost its canonical ledger row"));
        };
        if matches!(row.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. })
            && !r.aux.contains_key(&(PAID_CLAIM_FINALS_V2, key(&a.enrollment.id())))
        {
            if !l.paid_claim_consumers_v2.get(&a.enrollment.base_claim).is_some_and(|c| c.complete) {
                return Err(refused("paid Final lacks approvals of the randomly selected assigned verifiers"));
            }
            let (lock, completed_rounds, transcript_root) = if let Some(plan) = &a.class.interactive_plan {
                if !r.paid_claim_interactive_complete_v3(&a, &l).map_err(refused)? {
                    return Err(refused("interactive Final lacks complete full-material rounds"));
                }
                let last = r.paid_claim_round_v3(&a.enrollment.id(), plan.rounds - 1).map_err(refused)?;
                (last.lock.ok_or_else(|| refused("last round lock missing"))?, plan.rounds, last.transcript_root)
            } else {
                (
                    r.aux_row::<PalwPaidClaimLockV2>(PAID_CLAIM_LOCKS_V2, &key(&a.enrollment.id()))
                        .ok_or_else(|| refused("paid Final has no locked seed"))?,
                    1,
                    [0; 64],
                )
            };
            projections.push(PalwPaidClaimFinalV2 {
                enrollment: a.enrollment.id(),
                base_claim: a.enrollment.base_claim,
                verification_class: a.class.id(),
                sample_seed: lock.seed,
                source_context: lock.context,
                signed_lifetime: a.enrollment.lifetime,
                complete_receipts: lock.quorum,
                logical_final_daa: a.enrollment.lifetime.dispute_until.max(row.life.final_hold_until),
                completed_rounds,
                transcript_root,
            });
        }
    }
    for p in projections {
        put(b, PAID_CLAIM_FINALS_V2, &p.enrollment, &p);
    }
    let _ = ctx;
    Ok(())
}

/// Membership in the exact actual paid block plus the native drain observation
/// authenticates economic income. Measurement costs still need the trusted
/// external review root fixed in network identity; this verifier invents none.
fn verify_source_payment_v3(
    b: &TransitionBuilder<'_>,
    o: &misaka_palw_challenge::approval_scope::SourceEconomicObservationV3,
    p: &NativeSourcePaymentProofV3,
) -> Result<(), PalwStateV2Error> {
    let payment = b
        .state
        .monetary_material_row_v1(Hash64::from_bytes(o.source_claim))
        .and_then(|r| r.last_payment)
        .ok_or_else(|| refused("actual native source payment observation missing"))?;
    let header: crate::header::Header =
        borsh::from_slice(&p.header_bytes).map_err(|e| refused(format!("native payment header: {e}")))?;
    if borsh::to_vec(&header).unwrap() != p.header_bytes
        || crate::hashing::header::hash(&header) != payment.paid_block
        || header.daa_score != payment.paid_daa
        || payment.root.as_bytes() != o.source_claim
        || payment.amount != o.income_sompi
        || payment.paid_daa < o.measured_from_daa
        || payment.paid_daa > o.measured_until_daa
    {
        return Err(refused("economic proof does not bind the actual native paid root/block/DAA/amount"));
    }
    let txs: Vec<crate::tx::Transaction> = p
        .transaction_bytes
        .iter()
        .map(|bytes| {
            let tx: crate::tx::Transaction =
                borsh::from_slice(bytes).map_err(|e| refused(format!("native payment transaction: {e}")))?;
            if borsh::to_vec(&tx).unwrap() != *bytes {
                return Err(refused("noncanonical payment transaction"));
            }
            Ok(tx)
        })
        .collect::<Result<_, PalwStateV2Error>>()?;
    // Both historical mass encodings bind every output and value. The header
    // commits the actual selected encoding; mass is not an economic payout field.
    if crate::merkle::calc_hash_merkle_root(txs.iter()) != header.hash_merkle_root
        && crate::merkle::calc_hash_merkle_root_pre_crescendo(txs.iter()) != header.hash_merkle_root
    {
        return Err(refused("native payment body is not the actual paid block's Merkle commitment"));
    }
    let cb = &txs[0];
    if !cb.is_coinbase() || crate::hashing::tx::id(cb).as_bytes() != o.confirmed_income_transaction {
        return Err(refused("economic evidence names another actual coinbase transaction"));
    }
    let output = cb.outputs.get(p.coinbase_output as usize).ok_or_else(|| refused("economic payment output index missing"))?;
    let r = b.state.kernel_route.as_ref().unwrap();
    let ledger = r.ledger().map_err(refused)?;
    let mut bound = false;
    for ((context, producer), promise) in &ledger.beacon_source_fee_promises {
        if promise.terms_id != o.prepaid_fee_terms || promise.fee != o.income_sompi {
            continue;
        }
        let Some(source) = ledger.beacon_sources.get(&(*context, *producer)) else { continue };
        let Some((claim, _)) = source.revealed else { continue };
        if claim != o.source_claim {
            continue;
        }
        let bond = r.bond_key_of(producer).ok_or_else(|| refused("source payment recipient bond missing"))?;
        let native = b.state.bonds.get(&bond).ok_or_else(|| refused("source payment recipient native authority missing"))?;
        let expected_income = super::palw_bond_budget_fold_v1::native_queued_income_id_v1(payment.queue, &bond);
        let subject = route_fold::source_income_subject_v1(&source.job, &source.seal);
        let entitlement = super::palw_bond_budget_fold_v1::source_income_id_v1(Hash64::from_bytes(*context), subject, &bond);
        let Some(misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeOutcomeV1::Paid { daa, .. }) = promise.outcome else {
            continue;
        };
        let material_receipt = crate::palw_bond_budget_v1::material_queue_receipt_id_v1(entitlement, expected_income, daa);
        if payment.entitlement == Some(entitlement)
            && payment.material_receipt_id == material_receipt
            && expected_income == payment.income
            && native.operator_id.as_bytes() == o.source_operator
            && output.script_public_key == crate::mldsa87_primitives::p2pkh_mldsa87_spk(&native.payout_payload.as_bytes())
            && output.value == payment.coinbase_queue_amount
            && payment.amount <= payment.coinbase_queue_amount
        {
            bound = true;
            break;
        }
    }
    if !bound {
        return Err(refused("economic output does not bind the exact source fee/recipient/queued native income"));
    }
    Ok(())
}

/// The original claim admission transferred its R to the native income identity.
/// Extending the new signed lifetime reuses that same R; the original kernel
/// credit is already zero and cannot be reserved a second time.
fn extend_signed_kernel_income_v2(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Digest,
    producer: PalwBondKeyV2,
    until: u64,
) -> Result<(), PalwStateV2Error> {
    let Some(mirror) = b.bond_budget_mirror_at(ctx.daa_score) else { return Ok(()) };
    let income = super::palw_bond_budget_fold_v1::native_income_id_v1(Hash64::from_bytes(claim), &producer, 3);
    let Some(row) = b.state.bond_budget.as_ref().and_then(|book| book.claim_row(&income)).copied() else { return Ok(()) };
    let capital = b.state.bonds.get(&producer).map_or(0, |v| v.collateral);
    b.bond_budget_op(|book, _, journal| {
        book.hold_native_income_v1(
            &mirror.policy,
            capital,
            income,
            producer,
            row.accepted_daa,
            until,
            row.reserved.reward_sompi,
            &[],
            false,
            journal,
        )
    })
    .ok_or_else(|| refused("paid kernel native income budget missing"))?
    .map_err(|e| refused(e.to_string()))?;
    Ok(())
}

/// A source which loses its standing Final before the dependent claim is decided
/// cannot leave a cached success receipt behind. Failure is neutral timeout, not
/// producer fraud. Earned cash from already decided claims is never erased here.
pub(super) fn refresh_pending_paid_source_locks_v2(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let Some(r) = b.state.kernel_route.as_ref() else { return Ok(()) };
    let l = r.ledger().map_err(refused)?;
    let mut invalid = Vec::new();
    for ((_, key), bytes) in r.aux.range((PAID_CLAIM_LOCKS_V2, Vec::new())..(PAID_CLAIM_LOCKS_V2 + 1, Vec::new())) {
        let mut lock: PalwPaidClaimLockV2 = borsh::from_slice(bytes).map_err(|e| refused(e.to_string()))?;
        if !lock.source_valid {
            continue;
        }
        let a = r.paid_claim_authority_v2(&lock.context).map_err(refused)?;
        if l.claims.get(&a.enrollment.base_claim).is_none_or(|row| row.life.state.is_terminal()) {
            continue;
        }
        let Some((contract, sources, promises)) = r.paid_beacon_material_v5(&lock.context).map_err(refused)? else {
            return Err(refused("pending paid consumer lost source funding"));
        };
        let valid = matches!(misaka_palw_challenge::collect_paid_opt_in_work_beacon_v5(&a.context,&contract,&sources,&promises,ctx.daa_score).map_err(|e|refused(e.to_string()))?,misaka_palw_challenge::SealedBeaconStateV3::Locked(ref beacon) if beacon.challenge_anchor==lock.anchor);
        if !valid {
            lock.source_valid = false;
            invalid.push((key.clone(), lock));
        }
    }
    let mut invalid_rounds = Vec::new();
    for ((_, key), bytes) in r.aux.range((PAID_CLAIM_ROUNDS_V2, Vec::new())..(PAID_CLAIM_ROUNDS_V2 + 1, Vec::new())) {
        if key.len() != 64 {
            continue;
        }
        let mut row: crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3 =
            borsh::from_slice(bytes).map_err(|e| refused(e.to_string()))?;
        let Some(lock) = row.lock.as_mut() else {
            continue;
        };
        if !lock.source_valid {
            continue;
        }
        let a = r.paid_claim_enrollment_v3(&row.enrollment).map_err(refused)?;
        if l.claims.get(&a.enrollment.base_claim).is_none_or(|c| c.life.state.is_terminal()) {
            continue;
        }
        let Some((contract, sources, promises)) = r.paid_beacon_material_v5(&row.context.id()).map_err(refused)? else {
            return Err(refused("pending round lost paid source contract"));
        };
        let valid = matches!(misaka_palw_challenge::collect_paid_opt_in_work_beacon_v5(&row.context,&contract,&sources,&promises,ctx.daa_score)
            .map_err(|e|refused(e.to_string()))?,misaka_palw_challenge::SealedBeaconStateV3::Locked(ref beacon) if beacon.challenge_anchor==lock.anchor);
        if !valid {
            lock.source_valid = false;
            invalid_rounds.push((key.clone(), row));
        }
    }
    for (key, row) in invalid_rounds {
        b.write_kernel_row(PAID_CLAIM_ROUNDS_V2, key, Some(borsh::to_vec(&row).unwrap()));
    }
    for (key, row) in invalid {
        b.write_kernel_row(PAID_CLAIM_LOCKS_V2, key, Some(borsh::to_vec(&row).unwrap()));
    }
    Ok(())
}

fn round_message(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    policy: &PalwClaimChallengePolicyV2,
    enrollment: &Digest,
    round: u32,
    prior: Digest,
    message: Digest,
) -> Result<(), PalwStateV2Error> {
    use misaka_palw_kernel::claim_interactive_v3::*;
    let a = authority(b, enrollment)?;
    let plan = a.class.interactive_plan.as_ref().ok_or_else(|| refused("signed compiled interactive plan missing"))?;
    let r = b.state.kernel_route.as_ref().unwrap();
    if *signer != a.producer
        || round >= plan.rounds
        || message == [0; 64]
        || !a.enrollment.lifetime.receipt_open(ctx.daa_score)
        || r.aux.contains_key(&(PAID_CLAIM_ROUNDS_V2, key(&round_id_v3(*enrollment, round))))
    {
        return Err(refused("interactive message signer/round/lifetime/replay mismatch"));
    }
    if round == 0 {
        if prior != [0; 64] {
            return Err(refused("initial round has no previous transcript"));
        }
    } else {
        let previous = r.paid_claim_round_v3(enrollment, round - 1).map_err(refused)?;
        if !previous.complete() || previous.transcript_root != prior {
            return Err(refused("previous full-material round is incomplete/substituted"));
        }
    }
    let (subject, context, target, sponsor) = if plan.mode == misaka_palw_challenge::InteractiveModeV1::StagedBeacon {
        let subject = round_subject_v3(&a.subject, plan, *enrollment, round, prior, message);
        let mut context = a.context.clone();
        context.commitment_root = subject.commitment_root;
        context.commitment_position = ctx.daa_score;
        context.challenge_epoch = u64::from(round);
        let target =
            claim_source_target_v2(a.source_policy, context.clone(), a.enrollment.lifetime, &r.ledger().map_err(refused)?.policy)
                .map_err(refused)?;
        if target.terms.seal_start <= ctx.daa_score {
            return Err(refused("staged challenge window must open after its signed message"));
        }
        (subject, context, target, None)
    } else {
        let locked: PalwPaidClaimLockV2 = r
            .aux_row(PAID_CLAIM_LOCKS_V2, &key(enrollment))
            .ok_or_else(|| refused("FS needs the initial independently locked paid beacon"))?;
        if !locked.source_valid || locked.locked_daa > ctx.daa_score {
            return Err(refused("FS initial source invalid"));
        }
        // Revalidate initial source before accepting another adaptive query.
        r.paid_claim_check_authority_v2(&a.context.id(), ctx.daa_score).map_err(refused)?;
        (a.subject.clone(), a.context.clone(), a.source_target.clone(), a.sponsor)
    };
    let row = crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3 {
        enrollment: *enrollment,
        round,
        prior_transcript: prior,
        message_root: message,
        accepted_daa: ctx.daa_score,
        subject,
        context,
        target,
        sponsor,
        lock: None,
        transcript_root: [0; 64],
        receipts: vec![],
    };
    r.validate_paid_round_v4(&row, &a, &*r.ledger().map_err(refused)?).map_err(refused)?;
    admit_review(b, ctx, policy, &a, row.id(), u32::from(plan.mode == misaka_palw_challenge::InteractiveModeV1::StagedBeacon))?;
    put(b, PAID_CLAIM_ROUNDS_V2, &row.id(), &row);
    if plan.mode == misaka_palw_challenge::InteractiveModeV1::StagedBeacon {
        b.write_kernel_row(
            PAID_CLAIM_ROUNDS_V2,
            paid_claim_index_key_v3(PAID_CLAIM_ID_INDEX_PREFIX_V3, &row.context.id()),
            Some(
                borsh::to_vec(&PalwPaidClaimIndexV3 {
                    version: 3,
                    kind: PalwPaidClaimIndexKindV3::StagedContext,
                    owner: row.context.id(),
                    target: row.id(),
                })
                .unwrap(),
            ),
        );
    }
    Ok(())
}
fn round_fund(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    enrollment: &Digest,
    round: u32,
    terms: &misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTermsV1,
) -> Result<(), PalwStateV2Error> {
    let a = authority(b, enrollment)?;
    if a.class.source_policy.interactive_mode != misaka_palw_challenge::InteractiveModeV1::StagedBeacon {
        return Err(refused("FS uses its initial funding; no duplicate round debit"));
    }
    let mut row = b.state.kernel_route.as_ref().unwrap().paid_claim_round_v3(enrollment, round).map_err(refused)?;
    let (sf, ff) = source_fences(b, ctx.daa_score)?;
    let cap = ff.policy.max_funded_sources.min(row.target.terms.max_total);
    let minimum = ff.policy.minimum_fee_for(&row.target).map_err(refused)?;
    let sponsor = palw_kernel_bond_id_v1(signer);
    if row.sponsor.is_some()
        || row.lock.is_some()
        || ctx.daa_score >= row.target.terms.seal_start
        || sf.policy != a.source_policy
        || terms.sponsor != sponsor
        || terms.source != row.target.terms
        || terms.fee_per_source < minimum
        || terms.max_sources < a.class.source_policy.work_count_k
        || terms.max_sources > cap
    {
        return Err(refused("round funding is late/replayed/substituted/insufficient"));
    }
    row.sponsor = Some(*signer);
    let id = row.context.id();
    put(b, PAID_CLAIM_ROUNDS_V2, &row.id(), &row);
    put(b, crate::palw_beacon_source_opt_in_v1::PALW_BEACON_SOURCE_TABLE_TARGETS_V4, &id, &row.target);
    put(
        b,
        crate::palw_beacon_source_fee_v1::PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5,
        &id,
        &misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTargetV1 {
            target: row.target.clone(),
            sponsor,
            minimum_fee: minimum,
            max_sources: cap,
        },
    );
    put(
        b,
        crate::palw_beacon_source_fee_v1::PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5,
        &id,
        &ff.policy.cost_bounds.ok_or_else(|| refused("round cost scope missing"))?,
    );
    route_fold::apply_kernel_route_object_v1(b, ctx, &KernelRouteObjectV1::FundBeaconSourceFee { terms: *terms }.encode(), signer)?;
    let l = b.state.kernel_route.as_ref().unwrap().ledger().map_err(refused)?;
    if !l.beacon_source_fees.get(&id).is_some_and(|f| f.terms == *terms) {
        return Err(refused("strict same-R native round debit failed"));
    }
    Ok(())
}
fn round_lock(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    enrollment: &Digest,
    round: u32,
) -> Result<(), PalwStateV2Error> {
    let a = authority(b, enrollment)?;
    let r = b.state.kernel_route.as_ref().unwrap();
    let mut row = r.paid_claim_round_v3(enrollment, round).map_err(refused)?;
    if row.lock.is_some() {
        return Err(refused("round lock already consumed"));
    }
    let check = r.paid_claim_round_check_authority_v3(enrollment, round, ctx.daa_score).map_err(refused)?;
    let (contract, sources, promises) =
        r.paid_beacon_material_v5(&row.context.id()).map_err(refused)?.ok_or_else(|| refused("round funding missing"))?;
    let misaka_palw_challenge::SealedBeaconStateV3::Locked(beacon) =
        misaka_palw_challenge::collect_paid_opt_in_work_beacon_v5(&row.context, &contract, &sources, &promises, ctx.daa_score)
            .map_err(|e| refused(e.to_string()))?
    else {
        return Err(refused("round future source unavailable"));
    };
    let mut operators = std::collections::BTreeMap::new();
    for c in &a.checker_candidates {
        let rank = crate::palw_kernel_route_v1::palw_kernel_interim_key_v1(
            &Hash64::from_bytes(check.seed()),
            &c.operator,
            u128::from(c.weight),
        );
        let best = operators.entry(c.operator).or_insert((rank, c.bond));
        if (rank, c.bond) < *best {
            *best = (rank, c.bond);
        }
    }
    let mut ranked: Vec<_> = operators.into_values().collect();
    ranked.sort();
    let seats: Vec<_> =
        ranked.into_iter().take(crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_SEATS_V1).map(|(_, b)| b).collect();
    let quorum = u32::from(crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_QUORUM_V1);
    if seats.len() < quorum as usize {
        return Err(refused("round committee has too few independent native operators"));
    }
    let assignment = object_id(b"MISAKA/PALW/CLAIM-PROOF-ROUND-ASSIGNMENT/V3", &(row.id(), check.seed(), &seats, quorum));
    row.transcript_root = misaka_palw_kernel::claim_interactive_v3::transcript_root_v3(
        a.class.interactive_plan.as_ref().unwrap(),
        *enrollment,
        round,
        row.prior_transcript,
        row.message_root,
        row.context.id(),
        check.seed(),
    );
    row.lock = Some(PalwPaidClaimLockV2 {
        enrollment: *enrollment,
        context: row.context.id(),
        anchor: beacon.challenge_anchor,
        seed: check.seed(),
        locked_daa: ctx.daa_score,
        assignment,
        seats,
        quorum,
        source_valid: true,
    });
    r.validate_paid_round_v4(&row, &a, &*r.ledger().map_err(refused)?).map_err(refused)?;
    put(b, PAID_CLAIM_ROUNDS_V2, &row.id(), &row);
    Ok(())
}
fn round_receipt(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    receipt: &PaidClaimReceiptV2,
) -> Result<(), PalwStateV2Error> {
    let a = authority(b, &receipt.enrollment)?;
    let r = b.state.kernel_route.as_ref().unwrap();
    let mut row = r.paid_claim_round_v3(&receipt.enrollment, receipt.round).map_err(refused)?;
    let check = r.paid_claim_round_check_authority_v3(&receipt.enrollment, receipt.round, ctx.daa_score).map_err(refused)?;
    let locked = row.lock.as_ref().ok_or_else(|| refused("round future lock missing"))?;
    if !locked.source_valid
        || locked.seed != check.seed()
        || receipt.signer != palw_kernel_bond_id_v1(signer)
        || receipt.signed_daa > ctx.daa_score
        || !a.enrollment.lifetime.receipt_open(ctx.daa_score)
    {
        return Err(refused("round receipt signer/source/lifetime mismatch"));
    }
    row.receipts.push(receipt.clone());
    r.validate_paid_round_v4(&row, &a, &*r.ledger().map_err(refused)?).map_err(refused)?;
    put(b, PAID_CLAIM_ROUNDS_V2, &row.id(), &row);
    let mut l = route_fold::load_ledger(b, ctx)?;
    let before = l.to_rows();
    if l.paid_claim_consumers_v2.get(&a.enrollment.base_claim).is_some_and(|c| c.complete) {
        l.try_apply_paid_claim_tally_v2(&a.enrollment.base_claim).map_err(refused)?;
    }
    route_fold::flush(b, &l, &before);
    Ok(())
}
