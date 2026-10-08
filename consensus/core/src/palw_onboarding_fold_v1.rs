//! **The fold arms of the onboarding objects (tags 104–107, 109)** — child module of `palw_state_v2`, like the kernel route's fold.
//! See [`crate::palw_onboarding_v1`] for the design and [`crate::palw_conformance_evidence_v1`] for the evidence's judgement. Rows live
//! in the kernel route's aux tables 36–40 (written through the route's one journaled writer), the reservation is mirrored into V2's
//! committed-collateral ledger, and the closing tick releases a binding's reservation when its liability horizon ends and moves
//! every open conformance attempt on (unavailable beacon, withheld evidence, a challenge window that closed unrefuted).
//!
//! A structural refusal is an `Err`: the acceptance walk's rehearsal drops the object and the block stands, as for every V2 object.
//! Tag 109's judgement spends the block's adjudication budget first; a refusal after that is a DISMISSAL (`Ok`, only the charge
//! written), so junk evidence is never judged for free.

use super::palw_kernel_route_fold_v1::{charge_route_budget_v1, ensure_route_header};
use super::*;
use crate::palw_conformance_evidence_v1::{
    ConformanceEvidenceActionV1, ConformanceFaultV1, PALW_CONFORMANCE_CHALLENGE_WINDOW_DAA_V1,
    PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1, PostVerdictV1, SelectedCheckV1, derive_selection_v1, judge_leaf_fault_v1,
    judge_posted_evidence_v1, judge_vector_fault_v1, palw_onboarding_challenge_policy_v1, selected_check_v1,
};
use crate::palw_onboarding_v1::*;
use misaka_palw_challenge::{
    ConformanceCommitmentV1, OnboardingFailureV1, OnboardingRecordV1, OnboardingStateV1, OnboardingStepV1, WorkBeaconStateV1,
    collect_work_beacon_v1,
};

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}

fn key2(a: &Hash64, b: &Hash64) -> Vec<u8> {
    borsh::to_vec(&(*a, *b)).expect("two digests serialize")
}

impl PalwChainStateV2 {
    /// What the onboarding bindings hold against `bond` (0 with no kernel route): V2's committed-collateral ledger and both
    /// withdrawal gates add it beside the kernel ledger's own reservation, so no V2 gate counts a binding's slice as free.
    pub fn onboarding_reserved(&self, bond: &PalwBondKeyV2) -> u128 {
        self.kernel_route.as_ref().map(|k| k.onboarding_reserved_v1(bond) as u128).unwrap_or(0)
    }
}

/// The V2 class `class` as a binding needs it: live (Registered or Active), an IR class, and `signer` its registrant.
fn registrant_class<'a>(
    builder: &'a TransitionBuilder<'_>,
    class: &Hash64,
    signer: &PalwBondKeyV2,
) -> Result<(&'a PalwClassStateV2, &'a crate::palw_tir_admission_v1::PalwTirClassRecordV1), PalwStateV2Error> {
    let state = builder.state.classes.get(class).ok_or_else(|| refused("no such V2 class"))?;
    if !matches!(state.status, PalwClassStatusV2::Registered { .. } | PalwClassStatusV2::Active) {
        return Err(refused("only a Registered or Active class can be bound"));
    }
    if state.registrant_bond != Some(*signer) {
        return Err(refused("only the class's registrant can speak for its artifact"));
    }
    let tir = builder
        .state
        .tir_class_v1(class)
        .ok_or_else(|| refused("only an IR class has an inventory a binding can be challenged against"))?;
    let active = builder.state.bonds.get(signer).is_some_and(|b| matches!(b.status, PalwBondStatusV2::Active));
    if !active {
        return Err(refused("the signer is no Active bond"));
    }
    Ok((state, tir))
}

fn route_of<'a>(
    builder: &'a TransitionBuilder<'_>,
) -> Result<&'a crate::palw_kernel_route_v1::PalwKernelRouteStateV1, PalwStateV2Error> {
    builder.state.kernel_route.as_ref().ok_or_else(|| refused("the kernel route has no state"))
}

/// **Tag 104.** See the module doc of [`crate::palw_onboarding_v1`].
pub(super) fn apply_artifact_bound_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    v2_class: &Hash64,
    kernel_param_root: &Hash64,
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    registrant_class(builder, v2_class, signer)?;
    // One live binding per class: an artifact is one set of bytes, so two different kernel roots cannot both be true of it.
    let live = route_of(builder)?.artifact_bindings_of_v1(v2_class);
    if live.iter().any(|(root, row)| !row.refuted && root == kernel_param_root) {
        return Err(refused("this artifact binding already exists"));
    }
    if live.iter().any(|(_, row)| !row.refuted) {
        return Err(refused("the class is already bound to another kernel root, and is not refuted"));
    }
    // The statement is bonded: a slice of the signer's FREE collateral is held until the refutation horizon ends.
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    let free = collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score));
    if free < PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1 as u128 {
        return Err(refused("the signer's free collateral does not cover the binding's reservation"));
    }
    let row = ArtifactBindingRowV1 {
        binder: *signer,
        bound_daa: ctx.daa_score,
        matures_daa: ctx.daa_score.saturating_add(PALW_ONBOARDING_BINDING_WINDOW_DAA_V1),
        final_daa: ctx.daa_score.saturating_add(PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1),
        reserved: PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
        refuted: false,
    };
    builder.write_kernel_row(
        PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1,
        key2(v2_class, kernel_param_root),
        Some(borsh::to_vec(&row).expect("a binding row serializes")),
    );
    Ok(())
}

/// **Tag 105.** A proven mismatch refutes the binding, slashes its reservation and pays the challenger its share.
pub(super) fn apply_artifact_challenged_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    v2_class: &Hash64,
    kernel_param_root: &Hash64,
    challenger: &PalwBondKeyV2,
    proof: &ArtifactMismatchProofV1,
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    let row =
        route_of(builder)?.artifact_binding_v1(v2_class, kernel_param_root).ok_or_else(|| refused("no such artifact binding"))?;
    if row.refuted {
        return Err(refused("the binding is already refuted"));
    }
    if ctx.daa_score >= row.final_daa {
        return Err(refused("the binding is past its refutation horizon"));
    }
    let (challenger_active, challenger_operator, payout) = match builder.state.bonds.get(challenger) {
        Some(b) => (matches!(b.status, PalwBondStatusV2::Active), b.operator_id, b.payout_payload),
        None => return Err(refused("the challenger is no bond")),
    };
    if !challenger_active {
        return Err(refused("the challenger is no Active bond"));
    }
    // A binder cannot refute its own statement for the bounty.
    if builder.state.bonds.get(&row.binder).is_some_and(|b| b.operator_id == challenger_operator) {
        return Err(refused("a binding is refuted by an operator other than its binder's"));
    }
    let class = builder.state.classes.get(v2_class).ok_or_else(|| refused("no such V2 class"))?;
    let tir = builder.state.tir_class_v1(v2_class).ok_or_else(|| refused("not an IR class"))?;
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(tir.program.as_slice())
        .map_err(|_| refused("the class's stored program does not decode"))?;
    verify_artifact_mismatch_v1(&program, class.artifact_root, *kernel_param_root, proof).map_err(refused)?;
    // The fraud is proven: the reservation is forfeit (the slash's burn at release takes its half), the rest pays the challenger.
    let slashed = builder.slash_bond(row.binder, row.reserved as u128)?;
    let reward = slashed.saturating_mul(PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1) / 1000;
    builder.add_kernel_payout(payout, reward)?;
    let refuted = ArtifactBindingRowV1 { reserved: 0, refuted: true, ..row };
    builder.write_kernel_row(
        PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1,
        key2(v2_class, kernel_param_root),
        Some(borsh::to_vec(&refuted).expect("a binding row serializes")),
    );
    Ok(())
}

/// **Tag 106.**
pub(super) fn apply_kernel_bound_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    v2_class: &Hash64,
    kernel_class: &Hash64,
    challenge_policy_id: &Hash64,
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    let (_class, tir) = registrant_class(builder, v2_class, signer)?;
    let route = route_of(builder)?;
    if route.kernel_binding_v1(v2_class).is_some() {
        return Err(refused("the class is already kernel-bound: a plan or program cannot be substituted afterwards"));
    }
    // The kernel class stands only because the route registered it, after PUBLIC_PROSECUTION_COMPLETE(plan, profile).
    let record = route
        .kernel_class_record_v1(kernel_class)
        .ok_or_else(|| refused("the route holds no such kernel class (its registration IS the PUBLIC_PROSECUTION_COMPLETE gate)"))?;
    // The same program: byte for byte the canonical program the V2 class registered.
    if record.program_bytes.as_slice() != tir.program.as_slice() {
        return Err(refused("the kernel class runs another program than the V2 class registered"));
    }
    // The same artifact: the kernel class's commitments root is the root of a live (unrefuted) binding of THIS class.
    let kernel_param_root = Hash64::from_bytes(record.param_commitments.root());
    match route.artifact_binding_v1(v2_class, &kernel_param_root) {
        Some(binding) if !binding.refuted => {}
        _ => return Err(refused("the kernel class's artifact is not bound to this V2 class (or its binding is refuted)")),
    }
    // The network's post-commit challenge policy (RFC-0007 Part VI; `palw_onboarding_challenge_policy_v1`) — a class cannot choose a
    // lighter one, and the beacon and seed of its conformance are derived under exactly this descriptor.
    if challenge_policy_id.as_bytes() != palw_onboarding_challenge_policy_v1().id() {
        return Err(refused("not the network's challenge policy"));
    }
    let row = KernelBindingRowV1 {
        kernel_class: *kernel_class,
        kernel_param_root,
        plan_root: Hash64::from_bytes(record.plan.root()),
        challenge_policy_id: *challenge_policy_id,
        binder: *signer,
        bound_daa: ctx.daa_score,
    };
    builder.write_kernel_row(
        PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1,
        borsh::to_vec(v2_class).expect("a digest serializes"),
        Some(borsh::to_vec(&row).expect("a binding row serializes")),
    );
    Ok(())
}

/// **Tag 107.** Every bound root and policy of the RFC-0013 statement must be the chain's own, so a commitment made under another
/// policy, plan, program, kernel or artifact is refused here and not discovered when the beacon locks.
pub(super) fn apply_conformance_committed_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    commitment: &ConformanceCommitmentV1,
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    commitment.well_formed().map_err(refused)?;
    let class = Hash64::from_bytes(commitment.candidate_id);
    let (state, _tir) = registrant_class(builder, &class, signer)?;
    let route = route_of(builder)?;
    let binding = route
        .kernel_binding_v1(&class)
        .ok_or_else(|| refused("bind the class to a kernel class before committing a conformance statement"))?;
    let record = route.kernel_class_record_v1(&binding.kernel_class).ok_or_else(|| refused("the bound kernel class is gone"))?;
    let extras = builder.extras.kernel_route.as_ref().ok_or_else(|| refused("the kernel route is not in force"))?;
    let checks: [(bool, &str); 8] = [
        (commitment.chain_genesis == extras.chain_genesis.as_bytes(), "another chain's genesis"),
        (commitment.ruleset_id == route.header.policy.ruleset_digest, "another ruleset"),
        (commitment.challenge_policy_id == binding.challenge_policy_id.as_bytes(), "not the challenge policy the class is bound to"),
        (commitment.artifact_root == state.artifact_root.as_bytes(), "not the class's registered artifact root"),
        (
            commitment.program_root == misaka_palw_kernel::public::program_root_v1(&record.program_bytes),
            "not the bound kernel class's program root",
        ),
        (commitment.verification_plan_root == binding.plan_root.as_bytes(), "not the bound kernel class's plan root"),
        (commitment.kernel_descriptor_id == record.descriptor, "not the bound kernel class's kernel descriptor"),
        (
            matches!(
                commitment.subject_kind,
                misaka_palw_challenge::SubjectKindV1::ModelConformance | misaka_palw_challenge::SubjectKindV1::KernelConformance
            ),
            "not a conformance subject",
        ),
    ];
    if let Some((_, why)) = checks.iter().find(|(ok, _)| !ok) {
        return Err(refused(format!("conformance commitment: {why}")));
    }
    // One OPEN commitment per (class, artifact root): a different artifact is a different class and needs its own; an open or
    // passed attempt cannot be replaced by a statement that happens to suit a later beacon. Only an attempt that ENDED without a
    // pass (unavailable beacon, failed / refuted / withheld evidence) may be followed by a new one — counted (onboarding P0).
    let prior = route.conformance_attempt_v1(&class);
    match &prior {
        None if route.conformance_v1(&class, &state.artifact_root).is_some() => {
            return Err(refused("a conformance commitment is already on chain for this class and artifact"));
        }
        Some(a) if a.record.state != OnboardingStateV1::RegisteredDormant => {
            return Err(refused(
                "a conformance commitment is already on chain for this class and artifact (its attempt is open or passed)",
            ));
        }
        _ => {}
    }
    let policy = palw_onboarding_challenge_policy_v1();
    let statement_root = commitment.statement_root();
    let mut record = match &prior {
        Some(a) => a.record.clone(),
        // The chain's own facts carry the record to REGISTERED_DORMANT: the program the V2 class registered (its admission was the
        // frontend), the bound kernel class's plan (its registration was static admission and PUBLIC_PROSECUTION_COMPLETE), the class.
        None => {
            let mut r = OnboardingRecordV1::new(policy.retry_limit.saturating_add(1));
            for step in [
                OnboardingStepV1::Frontend(Ok(commitment.program_root)),
                OnboardingStepV1::StaticAdmission(Ok(commitment.verification_plan_root)),
                OnboardingStepV1::Registered { class_id: class.as_bytes() },
            ] {
                r.apply(step).map_err(|e| refused(e.to_string()))?;
            }
            r
        }
    };
    let epoch = record.attempts() as u64;
    // The policy's attempt limit: a further commitment is refused (`AttemptsExhausted`).
    record.apply(OnboardingStepV1::ConformanceCommitted { commitment_root: statement_root }).map_err(|e| refused(e.to_string()))?;
    // The beacon's sources, frozen now (RFC-0007 §VI.3): the route's Panel-independent (OPV) kernel classes, never the candidate nor
    // its own kernel class.
    let excluded: Vec<Hash64> = {
        let mut e = vec![class, binding.kernel_class];
        e.sort();
        e.dedup();
        e
    };
    let eligible: Vec<Hash64> = route
        .ledger()
        .map_err(refused)?
        .opv
        .classes
        .iter()
        .map(|c| Hash64::from_bytes(*c))
        .filter(|c| !excluded.contains(c))
        .collect();
    let row = ConformanceRowV1 { statement_root: Hash64::from_bytes(statement_root), signer: *signer, committed_daa: ctx.daa_score };
    let attempt = ConformanceAttemptRowV1 {
        record,
        commitment: commitment.clone(),
        committed_daa: ctx.daa_score,
        challenge_epoch: epoch,
        eligible_profiles: eligible,
        excluded_profiles: excluded,
        evidence: None,
        last_end: prior.and_then(|a| a.last_end),
    };
    let artifact_root = state.artifact_root;
    builder.write_kernel_row(
        PALW_ONBOARDING_TABLE_CONFORMANCE_V1,
        key2(&class, &artifact_root),
        Some(borsh::to_vec(&row).expect("a conformance row serializes")),
    );
    write_attempt(builder, &class, &attempt);
    // The previous attempt's material goes with it (its id stays in the record's history only as `last_end`).
    builder.write_kernel_row(PALW_ONBOARDING_TABLE_CONFORMANCE_EVIDENCE_V1, class_key(&class), None);
    Ok(())
}

fn class_key(class: &Hash64) -> Vec<u8> {
    borsh::to_vec(class).expect("a digest serializes")
}

fn write_attempt(builder: &mut TransitionBuilder<'_>, class: &Hash64, attempt: &ConformanceAttemptRowV1) {
    builder.write_kernel_row(
        PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1,
        class_key(class),
        Some(borsh::to_vec(attempt).expect("an attempt row serializes")),
    );
}

/// Close an open attempt without a pass: the contract's step for `end` (counted), the reason, the DAA.
fn end_attempt(attempt: &mut ConformanceAttemptRowV1, end: ConformanceAttemptEndV1, daa: u64) {
    let step = match end {
        ConformanceAttemptEndV1::BeaconUnavailable | ConformanceAttemptEndV1::BeaconChanged => OnboardingStepV1::BeaconUnavailable,
        _ => OnboardingStepV1::ConformanceChecked(Err(OnboardingFailureV1::ConformanceFailed)),
    };
    // An open attempt is ChallengePending, from which both steps are defined: the record cannot refuse them.
    let _ = attempt.record.apply(step);
    attempt.last_end = Some((end, daa));
}

// ---- tag 109: conformance evidence --------------------------------------------------------------------------------------------

/// **Tag 109.** See the module doc of [`crate::palw_conformance_evidence_v1`].
pub(super) fn apply_conformance_evidence_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    v2_class: &Hash64,
    action: &ConformanceEvidenceActionV1,
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    let attempt =
        route_of(builder)?.conformance_attempt_v1(v2_class).ok_or_else(|| refused("the class has no conformance attempt"))?;
    // ---- structural (free): who may act on which attempt ----
    match action {
        ConformanceEvidenceActionV1::Post(_) => {
            let (state, _tir) = registrant_class(builder, v2_class, signer)?;
            if !attempt.open() {
                return Err(refused("the class has no open conformance attempt"));
            }
            if attempt.evidence.is_some() {
                return Err(refused("the attempt's evidence is already posted (one evidence per attempt)"));
            }
            if attempt.commitment.artifact_root != state.artifact_root.as_bytes() {
                return Err(refused("the attempt is of another artifact"));
            }
        }
        ConformanceEvidenceActionV1::Refute { evidence_id, .. } => {
            let Some(posted) = attempt.evidence.filter(|_| attempt.open()) else {
                return Err(refused("no posted evidence is open to refutation"));
            };
            if posted.evidence_id != *evidence_id {
                return Err(refused("the refutation names other evidence than the attempt's"));
            }
            if ctx.daa_score >= posted.window_end_daa {
                return Err(refused("the evidence's challenge window has closed"));
            }
            let refuter = builder.state.bonds.get(signer).ok_or_else(|| refused("the refuter is no bond"))?;
            if !matches!(refuter.status, PalwBondStatusV2::Active) {
                return Err(refused("the refuter is no Active bond"));
            }
            let refuter_operator = refuter.operator_id;
            let registrant = builder.state.classes.get(v2_class).and_then(|c| c.registrant_bond);
            if registrant.and_then(|r| builder.state.bonds.get(&r)).is_some_and(|b| b.operator_id == refuter_operator) {
                return Err(refused("evidence is refuted by an operator other than the registrant's"));
            }
        }
    }
    // ---- the judgement spends the block's adjudication budget first (an over-budget object is dismissed, nothing charged) ----
    let work = borsh::to_vec(action).map(|b| b.len() as u64).unwrap_or(u64::MAX);
    if !charge_route_budget_v1(builder, ctx, work)? {
        return Ok(());
    }
    let policy = palw_onboarding_challenge_policy_v1();
    let route = route_of(builder)?;
    let Some(program) = builder
        .state
        .tir_class_v1(v2_class)
        .and_then(|tir| misaka_palw_tir::TirProgramV1::decode_canonical(tir.program.as_slice()).ok())
    else {
        return Ok(()); // dismissed: no program to judge against
    };
    let artifact_root = builder.state.classes.get(v2_class).map(|c| c.artifact_root).unwrap_or_default();
    let mut attempt = attempt;
    match action {
        ConformanceEvidenceActionV1::Post(post) => {
            let Ok(events) = route.beacon_events_v1() else { return Ok(()) };
            let bctx = attempt.beacon_context(&policy);
            let beacon = match collect_work_beacon_v1(&bctx, &events, ctx.daa_score) {
                Ok(WorkBeaconStateV1::Locked(b)) => b,
                // WAITING_RANDOMNESS (or unavailable): no seed exists yet, so no evidence can be about it — dismissed.
                _ => return Ok(()),
            };
            let (seed, verdict) = match judge_posted_evidence_v1(&attempt.commitment, &policy, &bctx, &beacon, &program, post) {
                Ok(judged) => judged,
                Err(_not_this_attempt) => return Ok(()), // dismissed: evidence about another commitment, policy, beacon, seed or scope
            };
            attempt.evidence = Some(PostedEvidenceRowV1 {
                evidence_id: Hash64::from_bytes(post.evidence.id()),
                seed: Hash64::from_bytes(seed),
                beacon_output: Hash64::from_bytes(beacon.output),
                lock_position: beacon.lock_position,
                posted_daa: ctx.daa_score,
                window_end_daa: ctx.daa_score.saturating_add(PALW_CONFORMANCE_CHALLENGE_WINDOW_DAA_V1),
            });
            if let PostVerdictV1::Fail { .. } = verdict {
                // Bound to the attempt and not a pass: the attempt fails, counted. The material stays readable (op 231).
                end_attempt(&mut attempt, ConformanceAttemptEndV1::EvidenceFailed, ctx.daa_score);
            }
            write_attempt(builder, v2_class, &attempt);
            builder.write_kernel_row(
                PALW_ONBOARDING_TABLE_CONFORMANCE_EVIDENCE_V1,
                class_key(v2_class),
                Some(borsh::to_vec(post.as_ref()).expect("a post serializes")),
            );
        }
        ConformanceEvidenceActionV1::Refute { fault, .. } => {
            let posted = attempt.evidence.expect("checked above");
            let Some(post) = route.conformance_evidence_post_v1(v2_class) else { return Ok(()) };
            let Ok(selection) = derive_selection_v1(&posted.seed.as_bytes(), &policy, &post.scope, &program) else { return Ok(()) };
            let proven = match fault.as_ref() {
                ConformanceFaultV1::LeafDecode { check, opening } => match selected_check_v1(&selection, &post, *check) {
                    Some(SelectedCheckV1::Leaf(leaf, outcome)) => {
                        judge_leaf_fault_v1(&program, artifact_root, leaf, outcome, opening).is_ok()
                    }
                    _ => false,
                },
                ConformanceFaultV1::VectorTokens { check, kernel_claim } => match selected_check_v1(&selection, &post, *check) {
                    Some(SelectedCheckV1::Vector(vector, outcome)) => {
                        vector_fault_proven(route, v2_class, kernel_claim, vector, outcome)
                    }
                    _ => false,
                },
            };
            if !proven {
                return Ok(()); // dismissed: the refutation proves nothing (it spent the block's budget)
            }
            end_attempt(&mut attempt, ConformanceAttemptEndV1::Refuted, ctx.daa_score);
            write_attempt(builder, v2_class, &attempt);
        }
    }
    Ok(())
}

/// A `VectorTokens` refutation: a Final, unconvicted claim of the class's BOUND kernel class whose job is the selected prompt under
/// the greedy rule, and whose delivered tokens contradict the posted reference tokens.
fn vector_fault_proven(
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    v2_class: &Hash64,
    kernel_claim: &Hash64,
    vector: &crate::palw_conformance_evidence_v1::SelectedVectorV1,
    outcome: &crate::palw_conformance_evidence_v1::CheckOutcomeV1,
) -> bool {
    let Some(binding) = route.kernel_binding_v1(v2_class) else { return false };
    let Ok(ledger) = route.ledger() else { return false };
    let Some(row) = ledger.claims.get(&kernel_claim.as_bytes()) else { return false };
    if row.class_binding_id != binding.kernel_class.as_bytes()
        || row.convicted
        || !matches!(row.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. })
    {
        return false;
    }
    let misaka_palw_kernel::ledger::ClaimBodyV1::Program { claim, .. } = &row.body else { return false };
    let Some(job) = ledger.jobs.get(&row.job_id) else { return false };
    judge_vector_fault_v1(vector, outcome, &job.prompt, job.decode == misaka_palw_kernel::job::DecodeRuleV1::Greedy, &claim.generated)
        .is_ok()
}

/// **A gated class activated** (`activate_due_classes`): its conformance record, at `G14_ELIGIBLE`, becomes `ACTIVE_REWARDABLE`
/// (availability: the artifact binding is Final, which the gate required). A no-op for a class with no record.
pub(super) fn note_class_activated_v1(builder: &mut TransitionBuilder<'_>, class: &Hash64) {
    let Some(mut attempt) = builder.state.kernel_route.as_ref().and_then(|r| r.conformance_attempt_v1(class)) else { return };
    if attempt.record.state == OnboardingStateV1::G14Eligible
        && attempt.record.apply(OnboardingStepV1::Activated { availability: true }).is_ok()
    {
        write_attempt(builder, class, &attempt);
    }
}

/// **The conformance part of the closing step**: every open attempt, in class order — a window that closed unrefuted over an unchanged
/// beacon passes (then the public-prosecution step), a beacon that cannot lock ends the attempt `BEACON_UNAVAILABLE`, a lock with no
/// evidence past the deadline is a default (withheld).
fn tick_conformance_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    let Some(route) = builder.state.kernel_route.as_ref() else { return };
    let rows: Vec<(Hash64, ConformanceAttemptRowV1)> = route
        .aux
        .range(
            (PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1, Vec::new())
                ..(PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1 + 1, Vec::new()),
        )
        .filter_map(|((_, key), row)| Some((borsh::from_slice(key).ok()?, borsh::from_slice::<ConformanceAttemptRowV1>(row).ok()?)))
        .filter(|(_, a)| a.open() || a.record.state == OnboardingStateV1::ConformancePassed)
        .collect();
    if rows.is_empty() {
        return;
    }
    let policy = palw_onboarding_challenge_policy_v1();
    let mut events: Option<Option<Vec<misaka_palw_challenge::WorkFinalEventV1>>> = None;
    for (class, mut attempt) in rows {
        let route = builder.state.kernel_route.as_ref().expect("read above");
        let kernel_class_stands =
            route.kernel_binding_v1(&class).is_some_and(|b| route.kernel_class_record_v1(&b.kernel_class).is_some());
        if attempt.record.state == OnboardingStateV1::ConformancePassed {
            if kernel_class_stands {
                let _ = attempt.record.apply(OnboardingStepV1::PublicProsecutionGate { complete: true });
                write_attempt(builder, &class, &attempt);
            }
            continue;
        }
        let bctx = attempt.beacon_context(&policy);
        let due = match attempt.evidence {
            Some(posted) => ctx.daa_score >= posted.window_end_daa,
            None => ctx.daa_score >= bctx.start(),
        };
        if !due {
            continue;
        }
        let events = events.get_or_insert_with(|| route.beacon_events_v1().ok());
        let Some(events) = events.as_ref() else { continue };
        let state = collect_work_beacon_v1(&bctx, events, ctx.daa_score);
        match (attempt.evidence, state) {
            (Some(posted), Ok(WorkBeaconStateV1::Locked(b))) if b.output == posted.beacon_output.as_bytes() => {
                // The window closed unrefuted, the beacon re-derives unchanged: CONFORMANCE_PASSED, then the public-prosecution step.
                let _ = attempt.record.apply(OnboardingStepV1::ConformanceChecked(Ok(posted.evidence_id.as_bytes())));
                let _ = attempt.record.apply(OnboardingStepV1::PublicProsecutionGate { complete: kernel_class_stands });
            }
            (Some(_), _) => end_attempt(&mut attempt, ConformanceAttemptEndV1::BeaconChanged, ctx.daa_score),
            (None, Ok(WorkBeaconStateV1::Unavailable { .. })) => {
                end_attempt(&mut attempt, ConformanceAttemptEndV1::BeaconUnavailable, ctx.daa_score)
            }
            (None, Ok(WorkBeaconStateV1::Locked(b)))
                if ctx.daa_score >= b.lock_position.saturating_add(PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1) =>
            {
                end_attempt(&mut attempt, ConformanceAttemptEndV1::Withheld, ctx.daa_score)
            }
            _ => continue,
        }
        write_attempt(builder, &class, &attempt);
    }
}

/// **The closing step**: a binding whose refutation horizon has ended releases its reservation (the row stays: a Final binding is
/// the record the activation gate reads). Deterministic, in key order.
pub(super) fn tick_onboarding_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    tick_conformance_v1(builder, ctx);
    let Some(route) = builder.state.kernel_route.as_ref() else { return };
    let due: Vec<(Vec<u8>, ArtifactBindingRowV1)> = route
        .aux
        .range((PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, Vec::new())..(PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1 + 1, Vec::new()))
        .filter_map(|((_, key), row)| Some((key.clone(), borsh::from_slice::<ArtifactBindingRowV1>(row).ok()?)))
        .filter(|(_, row)| row.reserved > 0 && !row.refuted && ctx.daa_score >= row.final_daa)
        .collect();
    for (key, row) in due {
        let released = ArtifactBindingRowV1 { reserved: 0, ..row };
        builder.write_kernel_row(
            PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1,
            key,
            Some(borsh::to_vec(&released).expect("a row serializes")),
        );
    }
}
