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

pub(super) fn eligibility_class_view_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    class_id: &Hash64,
) -> Option<PalwEligibilityClassViewV1> {
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
            // U-6 (user decision 2026-10-15): the BASE-0 floor alone is exempt.
            base_floor: *class_id == params.base_class_id(),
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
    let view = eligibility_class_view_v1(state, params, class_id)?;
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
    let Some(view) = eligibility_class_view_v1(&builder.state, builder.params, class_id) else { return Ok(None) };
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
    // RFC-0016 V3 (user decision 2026-10-15): past V+ the Probation blocks draw on the ONE shared new-model pool (the entrant
    // reserve, netted against its entrant grants) — refused whole when the pool cannot take the ask.
    if let Some(pool_left) = super::palw_rfc0016_fold_v1::palw_rfc0016_shared_pool_left_v1(
        &builder.state,
        builder.params,
        builder.extras.artifact_root_ownership_active,
        now,
    ) {
        let blocks = request_units.div_ceil(crate::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1);
        if blocks > pool_left {
            return Err(PalwStateV2Error::ClassNotRewardable {
                class: *class_id,
                code: PALW_ELIGIBILITY_PROBATION_BUDGET_CODE_V1,
                why: format!(
                    "RFC-0017 / RFC-0016 V3: a probation claim asks {blocks} block(s); the shared new-model pool has {pool_left} left"
                ),
            });
        }
    }
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

/// **RFC-0016 V3: the Probation blocks admitted in the probation epoch of `daa`** (the network row; 0 below the fence). With both
/// fences armed the probation epoch IS the V+ rights epoch (validation), so this is the pool's Probation draw this epoch.
pub(super) fn eligibility_probation_used_blocks_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2, daa: u64) -> u64 {
    let Some(mirror) = params.model_eligibility_v1_at(daa) else { return 0 };
    let network: PalwEligibilityNetworkRowV1 = row_of(state, &eligibility_network_key_v1()).unwrap_or_default();
    if network.probation_epoch != palw_probation_epoch_v1(&mirror.policy, daa) {
        return 0;
    }
    network.probation_units_used.div_ceil(crate::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1)
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
/// `retire_claim`, the consistency check) reads back — so this returns `contribution` unchanged (which is that 0). Its Final completes
/// the job and settles its escrow; it adds no chain-choice weight. Below the fence there are no pins.
///
/// **U-2 (user decision 2026-10-15): the pin OUTLIVES the Final** — it retires only when the claim leaves the state or is voided
/// ([`sweep_model_eligibility_v1`]) — so [`palw_probation_security_credit_v1`] can still say, of a `Final` claim the safe frontier,
/// the settled-anchor depth or rule E reads, that it was a Probation claim with no credit until certified.
pub(super) fn eligibility_final_weight_v1(_builder: &mut TransitionBuilder<'_>, _claim_id: &Hash64, contribution: u128) -> u128 {
    // The contribution is the budget's F = 0 grant already (every re-derivation reads the same number); nothing is written here.
    contribution
}

/// **U-2 (P0, user decision 2026-10-15) — does RFC-0017 BAR this claim from `SecurityCertified` credit outright?** `true` for a claim
/// accepted under `Probation` (it holds a probation pin, which outlives its `Final` and retires only when the claim leaves the state or
/// is voided — so the answer is frozen over the claim's life, as every Gate C re-derivation needs). Registered / Lapsed classes are
/// refused at the claim gate (`ELIG_NOT_ELIGIBLE`), so no such claim exists to bar. Never asks Gate C (no recursion): RFC-0018's
/// single predicate `palw_independently_work_certified_v1` asks THIS, so every Gate C read site — safe frontier, settlement depth,
/// rule-E participation, settled anchors, provisional/final weight, the coinbase credit leg — inherits the bar. `false` below the
/// fence (no pins exist).
pub fn palw_eligibility_bars_security_credit_v1(state: &PalwChainStateV2, claim_id: &Hash64) -> bool {
    row_of::<PalwEligibilityProbationClaimRowV1>(state, &eligibility_probation_claim_key_v1(&claim_id.as_bytes())).is_some()
}

/// **U-2 / U-5 (user decisions 2026-10-15): may `claim` contribute security credit, given its class's eligibility at the claim's
/// acceptance?** `false` for a claim of a `Probation` class or of a pre-fence class in its grace
/// ([`palw_eligibility_security_credit_needs_certification_v1`]) unless RFC-0018 Gate C governs AND certifies it; `true` otherwise
/// (RFC-0017 adds nothing). The grace keeps the class's rights; it never issues uncertified security credit.
pub fn palw_eligibility_security_credit_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
) -> bool {
    if !palw_probation_security_credit_v1(state, params, claim_id, claim) {
        return false;
    }
    let needs = eligibility_class_view_v1(state, params, &claim.class_id).is_some_and(|view| {
        palw_eligibility_security_credit_needs_certification_v1(
            params.model_eligibility_v1(),
            &view.facts,
            view.row.as_ref(),
            claim.accepted_daa,
        )
    });
    !needs
        || (crate::palw_rfc0018_v1::palw_gate_c_governs_v1(params, claim)
            && crate::palw_rfc0018_v1::palw_independently_work_certified_v1(
                claim_id,
                claim,
                &crate::palw_rfc0018_v1::PalwGateCViewV1::of(params, state),
            ))
}

/// **U-2 (user decision 2026-10-15): may `claim` contribute SECURITY credit** — the safe frontier, the settled-anchor depth, rule E's
/// participation? (A Probation claim's safe weight and bounded-immature weight are 0 whatever this says: `F = 0`, immature 0.) A
/// claim RFC-0017 pinned as a weightless Probation claim only once
/// RFC-0018 Gate C both governs it and certifies it ([`crate::palw_rfc0018_v1::palw_independently_work_certified_v1`] — the
/// drawn-verifier m-of-k approval, premise-based certification, never a computation proof); below Gate C's fence it has NO such credit
/// (the legacy `true` of that predicate never reaches a Probation claim). Every other claim: `true` (RFC-0017 adds nothing). The
/// sites that read safe frontier / settlement depth / rule E are RFC-0018's: with this fence they are reached through Gate C itself
/// (validation arms Gate C at or below this fence), and this predicate is the narrow interface they may AND in.
pub fn palw_probation_security_credit_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
) -> bool {
    let key = eligibility_probation_claim_key_v1(&claim_id.as_bytes());
    let weightless_probation = row_of::<PalwEligibilityProbationClaimRowV1>(state, &key).is_some_and(|pin| !pin.final_weight);
    if !weightless_probation {
        return true;
    }
    crate::palw_rfc0018_v1::palw_gate_c_governs_v1(params, claim)
        && crate::palw_rfc0018_v1::palw_independently_work_certified_v1(
            claim_id,
            claim,
            &crate::palw_rfc0018_v1::PalwGateCViewV1::of(params, state),
        )
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
    let view = eligibility_class_view_v1(&builder.state, builder.params, &class_id)
        .ok_or_else(|| refuse("the exam names no registered class"))?;
    let stage = palw_eligibility_effective_stage_v1(Some(&mirror), &view.facts, view.row.as_ref(), now)
        .ok_or_else(|| refuse("the BASE-0 liveness floor takes no eligibility exam (U-6: its dedicated exception)"))?;
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
    // GAP-E3 / U-4 (user decision 2026-10-15): the attempt cost, from the committer's FREE collateral (what its claims, courts and
    // accusations do not hold), in two parts: (a) the spam-burn fee, burned whole at the commit — the same write as a registration
    // burn (`collateral` falls, `slashed` rises, the release destroys it) — and (b) the inspection escrow (fee per position × m),
    // taken the same way and paid out from the payout queue to the attesting positions at the decision (the rest returned): a
    // transfer from the applicant, never new issuance.
    let fee = policy.attempt_fee_sompi;
    let escrow = palw_eligibility_inspection_escrow_v1(&policy);
    let free = (collateral as u128)
        .saturating_sub(builder.committed_at(signer, now))
        .saturating_sub(builder.read().accuser_ledger_v1(signer, now));
    if !palw_eligibility_attempt_cost_affordable_v1(free, &policy) {
        return Err(refuse("RFC-0017: the committing bond's free collateral does not cover the burn fee and the inspection escrow"));
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
        inspection_escrow_sompi: escrow,
        inspection_settled: escrow == 0,
    };
    builder.burn_registration_fee(*signer, fee)?;
    if escrow > 0 {
        builder.burn_registration_fee(*signer, escrow)?;
    }
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
    // U-4 (b): the inspection settlements this sweep writes (payout key, payout).
    let mut payout_writes: Vec<(Hash64, PalwPayoutV2)> = Vec::new();
    let payload_of = |kernel_id: &EligDigestV1| {
        builder.state.bonds.iter().find(|(key, _)| palw_kernel_bond_id_v1(key) == *kernel_id).map(|(_, record)| record.payout_payload)
    };
    let mut pin_retirements: Vec<Vec<u8>> = Vec::new();
    let table = PALW_MODEL_ELIGIBILITY_TABLE_V1;
    let rows: Vec<(Vec<u8>, Vec<u8>)> =
        route.aux.range((table, Vec::new())..(table + 1, Vec::new())).map(|((_, key), bytes)| (key.clone(), bytes.clone())).collect();
    for (key, bytes) in &rows {
        match key.first().copied() {
            Some(ELIGIBILITY_EXAM_PREFIX_V1) => {
                let mut exam: PalwEligibilityExamRowV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                if let Some((_, decided)) = exam.outcome {
                    // U-4 (b): settle a decided exam's inspection escrow before anything retires it (retried while the payout queue
                    // lacks room; the row is kept until it is settled).
                    if !exam.inspection_settled {
                        let (paid, refund) = palw_eligibility_inspection_settlement_v1(&exam);
                        let exam_id = exam.commit.id();
                        let mut legs: Vec<(Hash64, PalwPayoutV2)> = Vec::new();
                        let mut returned = refund;
                        for (verifier, amount) in paid {
                            match payload_of(&verifier) {
                                Some(payload) => legs.push((
                                    Hash64::from_bytes(palw_eligibility_inspection_payout_key_v1(&exam_id, &verifier)),
                                    PalwPayoutV2 { payload, amount },
                                )),
                                // A verifier bond gone since it attested: its part returns to the applicant.
                                None => returned = returned.saturating_add(amount),
                            }
                        }
                        if returned > 0
                            && let Some(payload) = payload_of(&exam.commit.committer)
                        {
                            legs.push((
                                Hash64::from_bytes(palw_eligibility_inspection_payout_key_v1(&exam_id, &exam.commit.committer)),
                                PalwPayoutV2 { payload, amount: returned },
                            ));
                        }
                        // (An applicant bond gone too: its part stays destroyed with its collateral — never minted.)
                        let room = PALW_V2_MAX_PENDING_PAYOUTS
                            .saturating_sub(builder.state.pending_payouts.len())
                            .saturating_sub(payout_writes.len());
                        let fresh = legs.iter().all(|(k, _)| {
                            !builder.state.pending_payouts.contains_key(k) && !payout_writes.iter().any(|(w, _)| w == k)
                        });
                        if legs.len() <= room && fresh {
                            payout_writes.extend(legs);
                            exam.inspection_settled = true;
                            exam_writes.push((key.clone(), Some(exam)));
                        }
                        continue;
                    }
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
                // A pin whose claim is gone (retired from the state) or voided retires; its units stay charged (consumption is
                // history). U-2: a `Final` claim KEEPS its pin while it is in the state (it still carries no security credit until
                // certified — `palw_probation_security_credit_v1`).
                let mut id = [0u8; 64];
                id.copy_from_slice(&key[1..]);
                let kept = builder
                    .state
                    .claims
                    .get(&Hash64::from_bytes(id))
                    .is_some_and(|claim| !matches!(claim.phase, PalwClaimPhaseV2::Voided { .. }));
                if !kept {
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
    for (key, payout) in payout_writes {
        builder.write_payout(key, Some(payout));
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
    let Some(view) = eligibility_class_view_v1(state, params, class_id) else { return PalwEligibilityStatusReadV1::default() };
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
