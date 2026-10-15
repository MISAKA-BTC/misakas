//! **The fold of RFC-0017's model eligibility** (inner 74/75 into kernel route aux 131; `crate::palw_model_eligibility_v1` holds the
//! policy, the rows and the pure rules). A child module of `palw_state_v2`, like the kernel route's own fold and N3's bootstrap fold.
//!
//! Every entry refuses below `palw_model_eligibility_v1` (the processor's A-2 gate drops inner 74/75 first, and every hook below asks
//! the mirror at the claim's acceptance DAA), and a refusal writes nothing. The rows record stages, counters and exams only: no
//! reward, block right or Final weight is GRANTED here — the hooks only withhold (claim gate), bound (probation grant) or pin
//! (probation Final weight off).
//!
//! ```text
//! eligibility_of_v1            the stage and rights of a class at a DAA (the one read every hook uses)
//! eligibility_claim_refusal_v1 REQ-ELIG-1: the claim gate's refusal for a Registered / Lapsed class (attempt and free-prompt lanes)
//! eligibility_probation_preview_v1 / _commit_v1
//!                              REQ-ELIG-2: a Probation class's claim admitted inside the lineage / network budgets, planned with F = 0
//!                              (`PalwBudgetPlanV1::without_final_weight_v1`) and pinned
//! eligibility_final_weight_v1  REQ-ELIG-2/6: a pinned probation claim's Final adds no weight; the pin retires at Final / void
//! apply_commit_eligibility_exam_v1   inner 74 (REQ-ELIG-3/4; GAP-E3: the attempt fee burned from free collateral)
//! apply_attest_eligibility_exam_v1   inner 75 (REQ-ELIG-3)
//! sweep_model_eligibility_v1   anchor → draw → verdict → outcome; lapse (REQ-ELIG-5); old exams retire
//! ```

use super::*;
use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
use crate::palw_model_eligibility_v1::*;

fn refuse(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}

/// The code the claim gate reports (`PalwStateV2Error::ClassNotRewardable`).
pub const PALW_ELIGIBILITY_NOT_ELIGIBLE_CODE_V1: &str = "ELIG_NOT_ELIGIBLE";
/// The code a probation claim over its budget is refused with.
pub const PALW_ELIGIBILITY_PROBATION_BUDGET_CODE_V1: &str = "ELIG_PROBATION_BUDGET";

/// How long a decided exam row is kept before it retires (its counters live on in the lineage / principal / network rows).
fn exam_retention_daa(policy: &PalwModelEligibilityPolicyV1) -> u64 {
    policy.attempt_epoch_daa
}

fn row_of<T: borsh::BorshDeserialize>(state: &PalwChainStateV2, key: &[u8]) -> Option<T> {
    state.kernel_route.as_ref()?.aux_row::<T>(PALW_MODEL_ELIGIBILITY_TABLE_V1, key)
}

fn write_row<T: borsh::BorshSerialize>(builder: &mut TransitionBuilder<'_>, key: Vec<u8>, row: Option<&T>) {
    builder.write_kernel_row(
        PALW_MODEL_ELIGIBILITY_TABLE_V1,
        key,
        row.map(|r| borsh::to_vec(r).expect("an eligibility row serializes")),
    );
}

/// **What the gates know about a class**: its facts, its lineage (artifact root), its registrant's operator id (all-zero for a
/// genesis class) and its row. `None` for a class the chain does not have.
pub(super) struct PalwEligibilityClassViewV1 {
    pub facts: PalwEligibilityClassFactsV1,
    pub lineage: EligDigestV1,
    pub principal: EligDigestV1,
    pub row: Option<PalwEligibilityModelRowV1>,
}

pub(super) fn eligibility_class_view_v1(state: &PalwChainStateV2, class_id: &Hash64) -> Option<PalwEligibilityClassViewV1> {
    let class = state.class(class_id)?;
    let principal = class
        .registrant_bond
        .as_ref()
        .and_then(|bond| state.bonds.get(bond))
        .map(|record| record.operator_id.as_bytes())
        .unwrap_or([0u8; 64]);
    let registry_admits = palw_eligibility_registry_admits_v1(state.model_lifecycles.get(class_id).map(|row| row.state));
    Some(PalwEligibilityClassViewV1 {
        facts: PalwEligibilityClassFactsV1 {
            genesis: class.registrant_bond.is_none(),
            registered_daa: class.registered_daa,
            registry_admits,
        },
        lineage: class.artifact_root.as_bytes(),
        principal,
        row: row_of(state, &eligibility_model_key_v1(&class_id.as_bytes())),
    })
}

/// **The one read every hook uses**: `(stage, rights)` of `class_id` at `daa`; `None` = not governed (below the fence, no mirror,
/// a genesis class, or a class the chain does not have) — the caller keeps the old rule.
pub(super) fn eligibility_of_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    class_id: &Hash64,
    daa: u64,
) -> Option<(PalwEligibilityStageV1, PalwEligibilityRightsV1)> {
    let mirror = params.model_eligibility_v1_at(daa)?;
    let view = eligibility_class_view_v1(state, class_id)?;
    let stage = palw_eligibility_effective_stage_v1(Some(mirror), &view.facts, view.row.as_ref(), daa)?;
    Some((stage, palw_eligibility_rights_of_stage_v1(stage, &mirror.policy)))
}

/// **REQ-ELIG-1, the claim gate's refusal** (attempt and free-prompt lanes — both carry a reward-bearing block right): a class whose
/// stage carries no block right (`Registered`, `Lapsed`) takes no such claim. Its user-paid kernel / funded jobs are not on this
/// gate (B = 0, F = 0, escrow-only) and complete as before. `None` below the fence.
pub(super) fn eligibility_claim_refusal_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    class_id: &Hash64,
    now_daa: u64,
) -> Option<(PalwEligibilityStageV1, String)> {
    let (stage, rights) = eligibility_of_v1(state, params, class_id, now_daa)?;
    (!rights.block_right).then(|| {
        (
            stage,
            format!(
                "RFC-0017: the class is {} — registration and lapse carry no reward-bearing block right (user-paid jobs only)",
                stage.name()
            ),
        )
    })
}

/// **A probation claim's admission, previewed** — what [`eligibility_probation_commit_v1`] will write. Read-only, so a lane calls it
/// before its first write (the ADR-0176 plan's rule: a refusal leaves the builder as it found it).
#[derive(Clone, Debug)]
pub(super) struct PalwEligibilityProbationTicketV1 {
    model_key: Vec<u8>,
    model: PalwEligibilityModelRowV1,
    lineage_key: Vec<u8>,
    lineage: PalwEligibilityLineageRowV1,
    principal_key: Vec<u8>,
    principal: PalwEligibilityPrincipalRowV1,
    network: PalwEligibilityNetworkRowV1,
    pin: PalwEligibilityProbationClaimRowV1,
}

impl PalwEligibilityProbationTicketV1 {
    /// Whether the claim must be planned with no Final weight (F = 0): the policy keeps probation Finals weightless.
    pub(super) fn weightless(&self) -> bool {
        !self.pin.final_weight
    }
}

/// **REQ-ELIG-2, the probation grant** — called by the attempt and free-prompt lanes right before their ADR-0176 budget plan, with
/// the claim's block-unit ask. Below the fence, or for a class that is not in `Probation`, `Ok(None)`. For a `Probation` class the
/// ask is admitted whole inside the lineage's per-stage cap and the network's fixed budget, or the claim is refused whole
/// (`ELIG_PROBATION_BUDGET`). The per-principal common cap (ADR-0176 `B`) is the budget plan's own refusal, which runs after this:
/// together, `min(principal cap, model cap, network budget)`.
pub(super) fn eligibility_probation_preview_v1(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    class_id: &Hash64,
    claim_bond: &PalwBondKeyV2,
    request_units: u64,
) -> Result<Option<PalwEligibilityProbationTicketV1>, PalwStateV2Error> {
    let now = ctx.daa_score;
    let Some(mirror) = builder.params.model_eligibility_v1_at(now).copied() else { return Ok(None) };
    let Some(view) = eligibility_class_view_v1(&builder.state, class_id) else { return Ok(None) };
    let Some(stage) = palw_eligibility_effective_stage_v1(Some(&mirror), &view.facts, view.row.as_ref(), now) else {
        return Ok(None);
    };
    if stage != PalwEligibilityStageV1::Probation {
        return Ok(None);
    }
    let policy = mirror.policy;
    let lineage_key = eligibility_lineage_key_v1(&view.lineage);
    let mut lineage: PalwEligibilityLineageRowV1 = row_of(&builder.state, &lineage_key).unwrap_or_default();
    let mut network: PalwEligibilityNetworkRowV1 = row_of(&builder.state, &eligibility_network_key_v1()).unwrap_or_default();
    let units = palw_probation_admit_v1(&policy, &lineage, &network, now, request_units).map_err(|(model_left, network_left)| {
        PalwStateV2Error::ClassNotRewardable {
            class: *class_id,
            code: PALW_ELIGIBILITY_PROBATION_BUDGET_CODE_V1,
            why: format!(
                "RFC-0017: a probation claim asks {request_units} block units; the model's stage cap has {model_left} and the \
                 network's probation budget {network_left} left this epoch"
            ),
        }
    })?;
    let operator = builder.state.bonds.get(claim_bond).map(|record| record.operator_id.as_bytes()).unwrap_or([0u8; 64]);
    let principal_key = eligibility_principal_key_v1(&operator);
    let mut principal: PalwEligibilityPrincipalRowV1 = row_of(&builder.state, &principal_key).unwrap_or_default();
    let mut model = view.row.clone().unwrap_or_else(|| {
        palw_eligibility_new_probation_row_v1(class_id.as_bytes(), view.lineage, view.principal, view.facts.registered_daa, now)
    });
    palw_probation_charge_v1(&policy, now, units, &mut model, &mut lineage, &mut principal, &mut network);
    Ok(Some(PalwEligibilityProbationTicketV1 {
        model_key: eligibility_model_key_v1(&class_id.as_bytes()),
        model,
        lineage_key,
        lineage,
        principal_key,
        principal,
        network,
        pin: PalwEligibilityProbationClaimRowV1 {
            class_id: class_id.as_bytes(),
            accepted_daa: now,
            units,
            final_weight: policy.probation_final_weight,
        },
    }))
}

/// **Commit a previewed probation admission** (after the claim is written): the charge on the model, lineage, principal and
/// network rows, and the claim's pin (`'C' ‖ claim`).
pub(super) fn eligibility_probation_commit_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: &Hash64,
    ticket: PalwEligibilityProbationTicketV1,
) -> Result<(), PalwStateV2Error> {
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    write_row(builder, ticket.model_key, Some(&ticket.model));
    write_row(builder, ticket.lineage_key, Some(&ticket.lineage));
    write_row(builder, ticket.principal_key, Some(&ticket.principal));
    write_row(builder, eligibility_network_key_v1(), Some(&ticket.network));
    write_row(builder, eligibility_probation_claim_key_v1(&claim_id.as_bytes()), Some(&ticket.pin));
    Ok(())
}

/// **The Final hook** (REQ-ELIG-2/6): a pinned probation claim's Final. Its weight is already 0 through its ADR-0176 reservation
/// (planned with F = 0, [`PalwEligibilityProbationTicketV1::weightless`]) — the number every re-derivation (`bond_budget_weight_or`,
/// `retire_claim`, the consistency check) reads back — so this returns `contribution` unchanged (which is that 0) and only retires
/// the pin. Its Final completes the job and settles its escrow; it adds no chain-choice weight. Below the fence there are no pins.
pub(super) fn eligibility_final_weight_v1(builder: &mut TransitionBuilder<'_>, claim_id: &Hash64, contribution: u128) -> u128 {
    let key = eligibility_probation_claim_key_v1(&claim_id.as_bytes());
    if row_of::<PalwEligibilityProbationClaimRowV1>(&builder.state, &key).is_some() {
        write_row::<PalwEligibilityProbationClaimRowV1>(builder, key, None);
    }
    contribution
}

/// **Inner 74**: commit an exam. The committer is any Active bond; the attempt is counted against the class's lineage, the
/// committer's principal and the network at once, before any randomness exists, and never refunded.
pub(super) fn apply_commit_eligibility_exam_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    commit: &PalwEligibilityExamCommitV1,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let mirror = *builder.params.model_eligibility_v1_at(now).ok_or_else(|| refuse("palw_model_eligibility_v1 is not in force"))?;
    let policy = mirror.policy;
    policy.validate().map_err(refuse)?;
    commit.validate().map_err(refuse)?;
    if commit.committer != palw_kernel_bond_id_v1(signer) {
        return Err(refuse("the exam's committer is not its signer"));
    }
    let record = builder.state.bonds.get(signer).ok_or(PalwStateV2Error::MissingBond(*signer))?;
    if !matches!(record.status, PalwBondStatusV2::Active) {
        return Err(refuse("an eligibility exam commit needs an Active bond"));
    }
    let committer_principal = record.operator_id.as_bytes();
    let collateral = record.collateral;
    // The route's header, created on first use (a refusal below drops the whole object, header included: the processor folds each
    // object alone before the block's fold).
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    {
        let route = builder.state.kernel_route.as_ref().ok_or_else(|| refuse("the kernel route is missing"))?;
        if commit.network != route.header.policy.network_domain {
            return Err(refuse("the exam names another network"));
        }
    }
    let class_id = Hash64::from_bytes(commit.class_id);
    let view = eligibility_class_view_v1(&builder.state, &class_id).ok_or_else(|| refuse("the exam names no registered class"))?;
    let stage = palw_eligibility_effective_stage_v1(Some(&mirror), &view.facts, view.row.as_ref(), now)
        .ok_or_else(|| refuse("a genesis class takes no eligibility exam"))?;
    let mut model = match view.row.clone() {
        Some(row) => row,
        // A pre-fence class's first renewal: its grandfathered standing becomes a row.
        None if view.facts.registered_daa < mirror.from_daa => {
            palw_eligibility_grandfathered_row_v1(&mirror, commit.class_id, view.lineage, view.principal, view.facts.registered_daa)
        }
        None => return Err(refuse("the class has no probation history yet: a promotion exam follows admitted probation claims")),
    };
    // The row's stored stage may be an `Eligible` past its grace the sweep has not written yet: judge the effective one.
    model.stage = stage;
    let exam_id = commit.id();
    let exam_key = eligibility_exam_key_v1(&exam_id);
    if row_of::<PalwEligibilityExamRowV1>(&builder.state, &exam_key).is_some() {
        return Err(refuse("this exam was already committed"));
    }
    let lineage_key = eligibility_lineage_key_v1(&view.lineage);
    let principal_key = eligibility_principal_key_v1(&committer_principal);
    let mut lineage: PalwEligibilityLineageRowV1 = row_of(&builder.state, &lineage_key).unwrap_or_default();
    let mut principal: PalwEligibilityPrincipalRowV1 = row_of(&builder.state, &principal_key).unwrap_or_default();
    let mut network: PalwEligibilityNetworkRowV1 = row_of(&builder.state, &eligibility_network_key_v1()).unwrap_or_default();
    palw_eligibility_commit_admits_v1(&policy, commit, &model, stage, &lineage, &principal, &network, now)
        .map_err(|why| refuse(format!("RFC-0017: the exam commit is refused: {why:?}")))?;
    // GAP-E3: the attempt fee, from the committer's FREE collateral (what its claims, courts and accusations do not hold), burned
    // whole at the commit — the same write as a registration burn (`collateral` falls, `slashed` rises, the release destroys it).
    let fee = policy.attempt_fee_sompi;
    let free = (collateral as u128)
        .saturating_sub(builder.committed_at(signer, now))
        .saturating_sub(builder.read().accuser_ledger_v1(signer, now));
    if !palw_eligibility_fee_affordable_v1(free, fee) {
        return Err(refuse("RFC-0017: the committing bond's free collateral does not cover the exam-attempt fee"));
    }
    palw_eligibility_count_attempt_v1(&policy, now, &mut lineage, &mut principal, &mut network);
    palw_eligibility_record_fee_v1(fee, &mut lineage, &mut principal, &mut network);
    model.open_exam = Some(exam_id);
    // The anchor floor: the latest seed anchor recorded when the exam was committed — the draw waits for a strictly later one.
    let committed_span = builder.state.round_seed_anchor.as_ref().map(|anchor| anchor.span).unwrap_or(0);
    let exam = PalwEligibilityExamRowV1 {
        commit: *commit,
        committer_principal,
        committed_daa: now,
        committed_span,
        anchor: None,
        draws: Vec::new(),
        examined: Vec::new(),
        drawn_daa: 0,
        deadline_daa: 0,
        attestations: Vec::new(),
        outcome: None,
    };
    builder.burn_registration_fee(*signer, fee)?;
    write_row(builder, exam_key, Some(&exam));
    write_row(builder, eligibility_model_key_v1(&commit.class_id), Some(&model));
    write_row(builder, lineage_key, Some(&lineage));
    write_row(builder, principal_key, Some(&principal));
    write_row(builder, eligibility_network_key_v1(), Some(&network));
    Ok(())
}

/// **Inner 75**: a drawn verifier's attestation, once per drawn bond, between the draw and its deadline. Its model-level verdict is
/// recorded; its per-claim mismatch list is the verifier's own accusation material for the existing per-claim courts and is never
/// counted here.
pub(super) fn apply_attest_eligibility_exam_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    attestation: &PalwEligibilityAttestationV1,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    builder.params.model_eligibility_v1_at(now).ok_or_else(|| refuse("palw_model_eligibility_v1 is not in force"))?;
    attestation.validate().map_err(refuse)?;
    if attestation.verifier != palw_kernel_bond_id_v1(signer) {
        return Err(refuse("the attestation's verifier is not its signer"));
    }
    let record = builder.state.bonds.get(signer).ok_or(PalwStateV2Error::MissingBond(*signer))?;
    if !matches!(record.status, PalwBondStatusV2::Active) {
        return Err(refuse("an eligibility attestation needs an Active bond"));
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    {
        let route = builder.state.kernel_route.as_ref().ok_or_else(|| refuse("the kernel route is missing"))?;
        if attestation.network != route.header.policy.network_domain {
            return Err(refuse("the attestation names another network"));
        }
    }
    let key = eligibility_exam_key_v1(&attestation.exam);
    let mut exam: PalwEligibilityExamRowV1 = row_of(&builder.state, &key).ok_or_else(|| refuse("no such eligibility exam"))?;
    if exam.outcome.is_some() || exam.anchor.is_none() {
        return Err(refuse("the exam is not drawn yet, or already decided"));
    }
    if now > exam.deadline_daa {
        return Err(refuse("the exam's attestation deadline has passed"));
    }
    if !exam.draws.contains(&attestation.verifier) {
        return Err(refuse("the attesting bond was not drawn for this exam"));
    }
    if exam.attestations.iter().any(|a| a.verifier == attestation.verifier) {
        return Err(refuse("the drawn bond already attested this exam"));
    }
    if attestation.claim_mismatches.iter().any(|i| !exam.examined.contains(i)) {
        return Err(refuse("a mismatch names a claim the exam did not examine"));
    }
    exam.attestations.push(PalwEligibilityAttestationRowV1 {
        verifier: attestation.verifier,
        verdict: attestation.verdict,
        transcript: attestation.transcript,
        attested_daa: now,
    });
    write_row(builder, key, Some(&exam));
    Ok(())
}

/// The exam's verifier population at its commit's cut, from the bond registry (canonicalized by the draw).
fn population_v1(
    state: &PalwChainStateV2,
    policy: &PalwModelEligibilityPolicyV1,
    exam: &PalwEligibilityExamRowV1,
) -> Vec<PalwEligibilityVerifierV1> {
    state
        .bonds
        .iter()
        .filter(|(_, record)| {
            palw_eligibility_population_admits_v1(
                policy,
                exam.committed_daa,
                &exam.committer_principal,
                matches!(record.status, PalwBondStatusV2::Active),
                record.registered_daa,
                record.collateral,
                &record.operator_id.as_bytes(),
            )
        })
        .map(|(key, record)| PalwEligibilityVerifierV1 {
            bond: palw_kernel_bond_id_v1(key),
            operator: record.operator_id.as_bytes(),
            stake: record.collateral,
        })
        .collect()
}

/// **The sweep** (the kernel route's closing step, at the block's DAA; journaled): (1) every committed exam whose anchor is now
/// recorded strictly after its commit is drawn — seed from that anchor, `m` stake draws over the population cut at the commit,
/// examined claims, deadline; (2) every drawn exam is decided once `k` positive positions exist or its deadline passed, and its
/// model takes the outcome; (3) `Eligible` rows past their grace lapse; (4) decided exams past their retention retire; (5) pins of
/// claims that ended without a Final retire. A no-op below the fence.
pub(super) fn sweep_model_eligibility_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let Some(mirror) = builder.params.model_eligibility_v1_at(now).copied() else { return Ok(()) };
    let policy = mirror.policy;
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let anchor = builder.state.round_seed_anchor;
    let mut exam_writes: Vec<(Vec<u8>, Option<PalwEligibilityExamRowV1>)> = Vec::new();
    let mut model_writes: Vec<(Vec<u8>, PalwEligibilityModelRowV1)> = Vec::new();
    let mut pin_retirements: Vec<Vec<u8>> = Vec::new();
    let table = PALW_MODEL_ELIGIBILITY_TABLE_V1;
    let rows: Vec<(Vec<u8>, Vec<u8>)> =
        route.aux.range((table, Vec::new())..(table + 1, Vec::new())).map(|((_, key), bytes)| (key.clone(), bytes.clone())).collect();
    for (key, bytes) in &rows {
        match key.first().copied() {
            Some(ELIGIBILITY_EXAM_PREFIX_V1) => {
                let mut exam: PalwEligibilityExamRowV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                if let Some((_, decided)) = exam.outcome {
                    if now > decided.saturating_add(exam_retention_daa(&policy)) {
                        exam_writes.push((key.clone(), None));
                    }
                    continue;
                }
                let mut changed = false;
                if exam.anchor.is_none()
                    && let Some(a) = anchor.as_ref()
                    && palw_eligibility_anchor_admissible_v1(&policy, exam.committed_span, a.span)
                {
                    let exam_id = exam.commit.id();
                    let execution_key = a.execution_key.as_bytes();
                    let seed = palw_eligibility_exam_seed_v1(&exam_id, a.span, &execution_key);
                    exam.anchor = Some((a.span, execution_key));
                    exam.drawn_daa = now;
                    exam.deadline_daa = now.saturating_add(policy.exam_attest_window_daa);
                    let population = population_v1(&builder.state, &policy, &exam);
                    match palw_eligibility_draw_v1(&seed, &exam_id, &population, policy.exam_m) {
                        Some(draws) => {
                            exam.draws = draws;
                            exam.examined = palw_eligibility_examined_claims_v1(&seed, exam.commit.claims_count, policy.exam_claims);
                        }
                        None => exam.outcome = Some((PalwEligibilityExamOutcomeV1::NoPopulation, now)),
                    }
                    changed = true;
                }
                if exam.outcome.is_none()
                    && let Some(outcome) = palw_eligibility_exam_verdict_v1(&policy, &exam, now)
                {
                    exam.outcome = Some((outcome, now));
                    changed = true;
                }
                if let Some((outcome, _)) = exam.outcome
                    && changed
                {
                    let model_key = eligibility_model_key_v1(&exam.commit.class_id);
                    let pending = model_writes.iter().position(|(k, _)| *k == model_key);
                    let current = match pending {
                        Some(i) => Some(model_writes[i].1.clone()),
                        None => route.aux_row::<PalwEligibilityModelRowV1>(table, &model_key),
                    };
                    if let Some(mut model) = current {
                        palw_eligibility_apply_outcome_v1(&policy, &mut model, exam.commit.purpose, outcome, now);
                        match pending {
                            Some(i) => model_writes[i].1 = model,
                            None => model_writes.push((model_key, model)),
                        }
                    }
                }
                if changed {
                    exam_writes.push((key.clone(), Some(exam)));
                }
            }
            Some(ELIGIBILITY_PROBATION_CLAIM_PREFIX_V1) if key.len() == 65 => {
                // A pin whose claim is gone or terminal without passing the Final hook (a void, a retirement) retires; its units stay
                // charged (consumption is history).
                let mut id = [0u8; 64];
                id.copy_from_slice(&key[1..]);
                let live = builder.state.claims.get(&Hash64::from_bytes(id)).is_some_and(|claim| !claim.phase.is_terminal());
                if !live {
                    pin_retirements.push(key.clone());
                }
            }
            Some(ELIGIBILITY_MODEL_PREFIX_V1) => {
                if model_writes.iter().any(|(k, _)| k == key) {
                    continue;
                }
                let mut model: PalwEligibilityModelRowV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                if model.open_exam.is_none() && palw_eligibility_lapse_v1(&policy, &mut model, now) {
                    model_writes.push((key.clone(), model));
                }
            }
            _ => {}
        }
    }
    for (key, exam) in exam_writes {
        write_row(builder, key, exam.as_ref());
    }
    for (key, model) in model_writes {
        write_row(builder, key, Some(&model));
    }
    for key in pin_retirements {
        write_row::<PalwEligibilityProbationClaimRowV1>(builder, key, None);
    }
    Ok(())
}

/// **The registry RPC's read** (`GetPalwModelRegistry`): a class's eligibility status at `daa` — "not-governed" (with the old rule's
/// rights) below the fence, for a genesis class and for a class the chain does not have.
pub fn palw_eligibility_status_of_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    class_id: &Hash64,
    daa: u64,
) -> PalwEligibilityStatusReadV1 {
    let Some(view) = eligibility_class_view_v1(state, class_id) else { return PalwEligibilityStatusReadV1::default() };
    let mirror = params.model_eligibility_v1();
    let (stage, next_renewal_daa, subsidy, block_right, final_weight) =
        palw_eligibility_status_v1(mirror, &view.facts, view.row.as_ref(), daa);
    let lineage: PalwEligibilityLineageRowV1 = row_of(state, &eligibility_lineage_key_v1(&view.lineage)).unwrap_or_default();
    PalwEligibilityStatusReadV1 {
        stage: stage.to_string(),
        next_renewal_daa,
        subsidy,
        block_right,
        final_weight,
        exams_passed: view.row.map(|r| r.exams_passed).unwrap_or(0),
        lineage_attempts: lineage.attempts_total,
    }
}
