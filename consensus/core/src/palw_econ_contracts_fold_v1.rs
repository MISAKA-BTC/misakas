//! **The fold of the RFC07/RFC08 signed economic contracts** (inner 67–69 into kernel route aux 125–127; `crate::palw_econ_contracts_v1`
//! holds the policies, pure rules and the restore fence). A child module of `palw_state_v2`, like the kernel route's own fold. Every
//! entry refuses below its own dormant fence (the processor's A-2 gate drops the object first), and every refusal writes nothing: the
//! checks run against a local ledger copy before the first journaled write.

use super::*;
use crate::palw_bond_budget_v1::PalwAllocationEpochV1;
use crate::palw_econ_contracts_v1::*;
use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
use misaka_palw_kernel::route::{KernelRouteObjectV1, SaltedCommitV1};

type Digest = [u8; 64];

fn refuse(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}

fn encode<T: borsh::BorshSerialize>(row: &T) -> Vec<u8> {
    borsh::to_vec(row).expect("an econ contract row serializes")
}

/// Aux 125's rows of one namespace: `None` the tallies (64-byte keys), `Some(prefix)` the 65-byte prefixed rows. A tally id may begin
/// with any byte, so a namespace is told by its key length, never by a key range alone.
fn verification_rows(
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    prefix: Option<u8>,
) -> impl Iterator<Item = (&Vec<u8>, &Vec<u8>)> {
    route
        .aux
        .range((PALW_MODEL_VERIFICATION_TABLE_V1, Vec::new())..(PALW_MODEL_VERIFICATION_TABLE_V1 + 1, Vec::new()))
        .filter(move |((_, key), _)| match prefix {
            None => key.len() == 64,
            Some(p) => key.len() == 65 && key[0] == p,
        })
        .map(|((_, key), bytes)| (key, bytes))
}

// ---- A. the verification-attestation gate ----------------------------------------------------------------------------------------

/// **Consumer hook: open a model-epoch tally** from a VERIFIED beacon output (OPVB's v3 `ClaimVerification` beacon at the allocation
/// epoch's close), the model's canonical Final claims of that epoch and the canonical global verifier pool. Called by
/// [`tick_model_verification_gate_v1`] once the frozen context's beacon locks. A tally opens once; capacity bounds the undecided ones.
#[allow(clippy::too_many_arguments)]
pub(super) fn open_model_verification_tally_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    model: &Digest,
    epoch: u64,
    beacon: &Digest,
    model_root: &Digest,
    final_claims: &[Digest],
    pool: &[(Digest, u64)],
) -> Result<PalwModelVerificationTallyV1, PalwStateV2Error> {
    let policy = *builder
        .params
        .econ_contracts_v1()
        .model_verification_gate_at(ctx.daa_score)
        .ok_or_else(|| refuse("palw_model_verification_gate_v1 is not in force"))?;
    policy.validate().map_err(refuse)?;
    if [*model, *beacon, *model_root].contains(&[0; 64]) {
        return Err(refuse("a verification tally needs a typed model, beacon output and root"));
    }
    let id = palw_model_verification_tally_id_v1(model, epoch);
    if let Some(route) = builder.state.kernel_route.as_ref() {
        if route.aux.contains_key(&(PALW_MODEL_VERIFICATION_TABLE_V1, id.to_vec())) {
            return Err(refuse("the model-epoch tally is already open"));
        }
        let mut open = 0u32;
        for (_, bytes) in verification_rows(route, None) {
            let row: PalwModelVerificationTallyV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
            if row.outcome(policy.threshold, ctx.daa_score) == PalwModelVerificationOutcomeV1::Pending {
                open = open.checked_add(1).ok_or_else(|| refuse("tally count overflow"))?;
            }
        }
        if open >= policy.maximum_open_tallies {
            return Err(refuse("the verification gate's open tally capacity is exhausted"));
        }
    }
    let slots = palw_model_verification_draw_v1(beacon, &id, pool, policy.slots).map_err(refuse)?;
    let sampled_claim = palw_model_verification_sample_v1(policy.sample_rule, beacon, &id, final_claims).map_err(refuse)?;
    let deadline_daa =
        ctx.daa_score.checked_add(policy.attestation_window_daa).ok_or_else(|| refuse("verification deadline overflow"))?;
    let row = PalwModelVerificationTallyV1 {
        model: *model,
        epoch,
        beacon: *beacon,
        model_root: *model_root,
        sampled_claim,
        slots: slots.into_iter().map(|s| (s, ModelVerificationVerdictV1::Unset)).collect(),
        opened_daa: ctx.daa_score,
        deadline_daa,
        policy: policy.id(),
    };
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    builder.write_kernel_row(PALW_MODEL_VERIFICATION_TABLE_V1, id.to_vec(), Some(encode(&row)))?;
    Ok(row)
}

/// **Inner 67**: a drawn holder's attestation. It fills every open slot the signer holds (draws are with replacement, so a stake drawn
/// twice counts twice) with its verdict, for exactly the tally's sampled claim and root, before the deadline.
pub(super) fn attest_model_verification_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    attestation: &ModelVerificationAttestationV1,
) -> Result<(), PalwStateV2Error> {
    let policy = *builder
        .params
        .econ_contracts_v1()
        .model_verification_gate_at(ctx.daa_score)
        .ok_or_else(|| refuse("palw_model_verification_gate_v1 is not in force"))?;
    attestation.validate().map_err(refuse)?;
    if attestation.verifier != palw_kernel_bond_id_v1(signer) {
        return Err(refuse("the attestation's verifier is not its signer"));
    }
    let mut row = {
        let route = builder.state.kernel_route.as_ref().ok_or_else(|| refuse("the kernel route is missing"))?;
        if attestation.network != route.header.policy.network_domain {
            return Err(refuse("the attestation names another network"));
        }
        let row: PalwModelVerificationTallyV1 =
            route.aux_row(PALW_MODEL_VERIFICATION_TABLE_V1, &attestation.tally).ok_or_else(|| refuse("no such model-epoch tally"))?;
        row
    };
    row.validate(&policy).map_err(refuse)?;
    if attestation.tally != palw_model_verification_tally_id_v1(&row.model, row.epoch)
        || (attestation.model, attestation.epoch) != (row.model, row.epoch)
        || Some(attestation.sampled_claim) != row.sampled_claim
        || attestation.model_root != row.model_root
    {
        return Err(refuse("the attestation names another subject than the tally sampled"));
    }
    if row.outcome(policy.threshold, ctx.daa_score) != PalwModelVerificationOutcomeV1::Pending {
        return Err(refuse("the model-epoch tally is already decided"));
    }
    let mut filled = false;
    for (holder, verdict) in row.slots.iter_mut() {
        if *holder == attestation.verifier && *verdict == ModelVerificationVerdictV1::Unset {
            *verdict = attestation.verdict;
            filled = true;
        }
    }
    if !filled {
        return Err(refuse("the signer holds no open drawn slot of this tally"));
    }
    builder.write_kernel_row(PALW_MODEL_VERIFICATION_TABLE_V1, attestation.tally.to_vec(), Some(encode(&row)))?;
    Ok(())
}

/// **Consumer hook: take a decided tally's outcome** for the allocation payout ([`palw_model_verification_subsidy_v1`]). A pending
/// tally stays (and answers `Pending`); a decided one is replaced by its compact decided row (aux 125, `0x03 ‖ tally id`) — every
/// claim of the model-epoch is paid by the same answer, so the answer outlives the slots. Called at the Final payout
/// ([`gate_final_reward_v1`]) and by the tick.
pub(super) fn take_model_verification_outcome_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    tally: &Digest,
) -> Result<Option<PalwModelVerificationOutcomeV1>, PalwStateV2Error> {
    let Some(policy) = builder.params.econ_contracts_v1().model_verification_gate_at(ctx.daa_score).copied() else { return Ok(None) };
    let Some(row) = builder
        .state
        .kernel_route
        .as_ref()
        .and_then(|route| route.aux_row::<PalwModelVerificationTallyV1>(PALW_MODEL_VERIFICATION_TABLE_V1, tally))
    else {
        return Ok(None);
    };
    let outcome = row.outcome(policy.threshold, ctx.daa_score);
    if outcome != PalwModelVerificationOutcomeV1::Pending {
        take_decided_v1(builder, &row, outcome, ctx.daa_score)?;
    }
    Ok(Some(outcome))
}

/// Replace a decided tally by its decided row (journaled).
fn take_decided_v1(
    builder: &mut TransitionBuilder<'_>,
    row: &PalwModelVerificationTallyV1,
    outcome: PalwModelVerificationOutcomeV1,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    let id = palw_model_verification_tally_id_v1(&row.model, row.epoch);
    let decided = PalwModelVerificationDecidedV1 { model: row.model, epoch: row.epoch, outcome, decided_daa: now };
    builder.write_kernel_row(PALW_MODEL_VERIFICATION_TABLE_V1, id.to_vec(), None)?;
    builder.write_kernel_row(PALW_MODEL_VERIFICATION_TABLE_V1, palw_model_verification_decided_key_v1(&id), Some(encode(&decided)))?;
    Ok(())
}

// ---- A′. the gate's wiring: OPVB beacon → tally, Final payout → outcome (2026-10-15 round 3) -----------------------------------

/// The gate's policy and activation where it is in force at `daa`.
fn gate_at(builder: &TransitionBuilder<'_>, daa: u64) -> Option<(u64, PalwModelVerificationGatePolicyV1)> {
    let mirror = builder.params.econ_contracts_v1();
    let policy = *mirror.model_verification_gate_at(daa)?;
    mirror.model_verification_gate.as_ref().map(|(from, _)| (*from, policy))
}

/// **What the gate answers for `(model, epoch)` at `now`, without writing**: the tally's outcome while it is open, the decided row's
/// once taken, `Pending` while the frozen context waits for its beacon, `Unattested` once that beacon became unavailable or vetoed.
/// With no row at all: `Pending` while the allocation epoch is not yet closed; a closed epoch with no row was never verifiable (its
/// context could not be frozen) and is `Unattested` — the subsidy is withheld, never minted on an unverified epoch.
fn model_epoch_outcome_v1(
    builder: &TransitionBuilder<'_>,
    policy: &PalwModelVerificationGatePolicyV1,
    model: &Digest,
    epoch: u64,
    now: u64,
) -> PalwModelVerificationOutcomeV1 {
    use PalwModelVerificationOutcomeV1 as O;
    let id = palw_model_verification_tally_id_v1(model, epoch);
    if let Some(route) = builder.state.kernel_route.as_ref() {
        if let Some(tally) = route.aux_row::<PalwModelVerificationTallyV1>(PALW_MODEL_VERIFICATION_TABLE_V1, &id) {
            return tally.outcome(policy.threshold, now);
        }
        if let Some(decided) = route
            .aux_row::<PalwModelVerificationDecidedV1>(PALW_MODEL_VERIFICATION_TABLE_V1, &palw_model_verification_decided_key_v1(&id))
        {
            return decided.outcome;
        }
        if let Some(context) = route
            .aux_row::<PalwModelVerificationContextV1>(PALW_MODEL_VERIFICATION_TABLE_V1, &palw_model_verification_context_key_v1(&id))
        {
            return if context.unavailable_daa.is_some() { O::Unattested } else { O::Pending };
        }
    }
    let open = builder.state.bond_budget.as_ref().and_then(|b| b.header.allocation).is_none_or(|current| current.index <= epoch);
    if open { O::Pending } else { O::Unattested }
}

/// **R7-ECON at the allocation payout** (`finalize_claim`, right after the budget's grant and before the export cap, the buyback and
/// every split, so each leg derives from the gated amount). A claim the model allocation reserved for — its budget row names `(model,
/// epoch)`, accepted at or past the gate's fence — is paid by its model-epoch's verification ([`palw_model_verification_subsidy_v1`]):
/// `Attested` or `NoFinalClaim` pays it whole; `Unattested` withholds it (never named, never minted); undecided defers it whole into a
/// deferred row the tick releases to the producer's payload or drops once the model-epoch decides. Every other claim — below the fence,
/// unallocated, unbudgeted — is returned unchanged (A-2: the payout is byte-identical below the fence). Claims, user escrow, Final,
/// weight and slashing are untouched: only this amount is read.
pub(super) fn gate_final_reward_v1(
    builder: &mut TransitionBuilder<'_>,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
    escrow: u64,
    final_daa: u64,
) -> Result<u64, PalwStateV2Error> {
    if escrow == 0 {
        return Ok(0);
    }
    let Some((from, policy)) = gate_at(builder, final_daa) else { return Ok(escrow) };
    let Some(row) = builder.state.bond_budget.as_ref().and_then(|b| b.claim_row(claim_id)).copied() else { return Ok(escrow) };
    let Some((model, epoch)) = row.model else { return Ok(escrow) };
    if row.accepted_daa < from {
        return Ok(escrow);
    }
    let model = model.as_bytes();
    let id = palw_model_verification_tally_id_v1(&model, epoch);
    // A decided tally is taken here (its answer outlives its slots; every claim of the model-epoch reads the same one).
    if let Some(tally) = builder
        .state
        .kernel_route
        .as_ref()
        .and_then(|route| route.aux_row::<PalwModelVerificationTallyV1>(PALW_MODEL_VERIFICATION_TABLE_V1, &id))
    {
        let outcome = tally.outcome(policy.threshold, final_daa);
        if outcome != PalwModelVerificationOutcomeV1::Pending {
            take_decided_v1(builder, &tally, outcome, final_daa)?;
        }
    }
    let gated = palw_model_verification_subsidy_v1(model_epoch_outcome_v1(builder, &policy, &model, epoch, final_daa), escrow);
    if gated.deferred > 0 {
        if builder.state.kernel_route.is_none() {
            // Unreachable on a valid ruleset: the tick creates the route header at every block the gate is in force.
            return Err(refuse("a deferred verification reward needs the kernel route"));
        }
        let payload = builder.state.bonds.get(&claim.bond).ok_or(PalwStateV2Error::MissingBond(claim.bond))?.payout_payload;
        let deferred = PalwModelVerificationDeferredV1 {
            model,
            epoch,
            claim: claim_id.as_bytes(),
            bond: claim.bond,
            payload,
            amount: gated.deferred,
            deferred_daa: final_daa,
        };
        builder.write_kernel_row(
            PALW_MODEL_VERIFICATION_TABLE_V1,
            palw_model_verification_deferred_key_v1(&claim_id.as_bytes()),
            Some(encode(&deferred)),
        )?;
    }
    Ok(gated.payable)
}

/// The registered root a model's verifiers authenticate their copy against: the class's artifact root (a model line's founding
/// class's).
fn model_root_v1(state: &PalwChainStateV2, model: &Hash64) -> Option<Digest> {
    let class = state.model_lines.get(model).map(|line| line.class_id).unwrap_or(*model);
    state.classes.get(&class).map(|c| c.artifact_root.as_bytes()).filter(|root| *root != [0; 64])
}

/// **Step 1f′ — the gate's clock** (end of [`super::palw_bond_budget_fold_v1::tick_bond_budget_v1`], so the fold and the pre-object
/// base run it alike, before the sweeps that Final claims). A no-op below `palw_model_verification_gate_v1`. In order:
///
/// 1. at the block that rolls the allocation epoch, every closed `(model, epoch)` with a live gated claim and no row gets its frozen
///    OPVB context (v3 sealed-source `ClaimVerification`, commitment = the tally id at this position, the OPV classes eligible now
///    minus the model);
/// 2. each waiting context whose beacon locked opens its tally from the VERIFIED beacon output (canonical order, while capacity
///    lasts): the sample is one Final claim of the model-epoch, the pool every Active bond at the collateral floor except the
///    sampled claim's producer operator, drawn by stake; a beacon that is unavailable, vetoed or refused, or an empty pool, marks
///    the context unavailable (verification can no longer be attested);
/// 3. a decided tally is taken (its decided row replaces it);
/// 4. deferred rewards whose model-epoch decided are released (`Attested` / `NoFinalClaim`: one payout to the producer's payload,
///    while the payout queue has room) or dropped (`Unattested`: withheld, never minted);
/// 5. decided rows and contexts no live gated claim and no deferred reward reads any more retire.
pub(super) fn tick_model_verification_gate_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let Some((from, policy)) = gate_at(builder, now) else { return Ok(()) };
    if builder.extras.kernel_route.is_none() {
        // The gate's rows are the kernel route's; its validation requires the route at or below it.
        return Ok(());
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    let table = PALW_MODEL_VERIFICATION_TABLE_V1;
    // The live gated `(model, epoch)` pairs of closed epochs (`live`: the claim can still be paid).
    let (current, live): (Option<PalwAllocationEpochV1>, BTreeSet<(Hash64, u64)>) = match builder.state.bond_budget.as_ref() {
        Some(budget) => (
            budget.header.allocation,
            budget.claims.values().filter(|row| row.live && row.accepted_daa >= from).filter_map(|row| row.model).collect(),
        ),
        None => (None, BTreeSet::new()),
    };
    // 1. Freeze the contexts of the epochs this block closed.
    if let Some(epoch) = current.filter(|e| e.start_daa == now) {
        let extras = builder.extras.kernel_route.clone().expect("checked above");
        let route = builder.state.kernel_route.as_ref().expect("created above");
        let ledger = route
            .ledger_native_projection_checked_v9()
            .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refuse))?;
        let eligible: BTreeSet<Hash64> = match extras.opv.as_ref() {
            Some(opv) => route
                .opv_eligible_set_v1(&ledger, now, &crate::palw_opv_bootstrap_v1::OpvEligibilityViewV1::of(opv))
                .into_iter()
                .collect(),
            None => BTreeSet::new(),
        };
        let mut frozen = Vec::new();
        for (model, e) in live.iter().filter(|(_, e)| *e < epoch.index) {
            let m = model.as_bytes();
            let id = palw_model_verification_tally_id_v1(&m, *e);
            if route.aux.contains_key(&(table, id.to_vec()))
                || route.aux.contains_key(&(table, palw_model_verification_context_key_v1(&id)))
                || route.aux.contains_key(&(table, palw_model_verification_decided_key_v1(&id)))
            {
                continue;
            }
            // No registered root: nothing can be authenticated, so no context; the model-epoch answers `Unattested`.
            let Some(model_root) = model_root_v1(&builder.state, model) else { continue };
            let context = misaka_palw_challenge::BeaconContextV1 {
                chain_genesis: extras.chain_genesis.as_bytes(),
                ruleset_id: route.header.policy.ruleset_digest,
                policy: crate::palw_conformance_evidence_v1::palw_onboarding_sealed_policy_v1(),
                subject_kind: misaka_palw_challenge::SubjectKindV1::ClaimVerification,
                commitment_root: id,
                commitment_position: now,
                challenge_epoch: *e,
                eligible_profiles: eligible.iter().filter(|c| *c != model).map(|c| c.as_bytes()).collect(),
                excluded_profiles: BTreeSet::from([m]),
                candidate_profile_id: misaka_palw_challenge::RootV1::Absent,
            };
            let row = PalwModelVerificationContextV1 {
                model: m,
                epoch: *e,
                model_root,
                context,
                frozen_daa: now,
                unavailable_daa: None,
                policy: policy.id(),
            };
            frozen.push((palw_model_verification_context_key_v1(&id), encode(&row)));
        }
        for (key, bytes) in frozen {
            builder.write_kernel_row(table, key, Some(bytes))?;
        }
    }
    // 2. Open the tallies whose beacon locked.
    let contexts: Vec<(Vec<u8>, PalwModelVerificationContextV1)> = {
        let route = builder.state.kernel_route.as_ref().expect("created above");
        verification_rows(route, Some(MODEL_VERIFICATION_CONTEXT_PREFIX_V1))
            .map(|(key, bytes)| borsh::from_slice(bytes).map(|row| (key.clone(), row)).map_err(|e| refuse(e.to_string())))
            .collect::<Result<_, _>>()?
    };
    if contexts.iter().any(|(_, c)| c.unavailable_daa.is_none()) {
        let sources = builder.state.kernel_route.as_ref().expect("created above").beacon_sealed_sources_v1().map_err(refuse)?;
        for (key, mut context) in contexts.into_iter().filter(|(_, c)| c.unavailable_daa.is_none()) {
            use misaka_palw_challenge::SealedBeaconStateV3 as B;
            let beacon = match misaka_palw_challenge::collect_sealed_work_beacon_v3(&context.context, &sources, now) {
                Ok(B::Locked(beacon)) => beacon.output,
                Ok(B::Unavailable { .. } | B::Vetoed { .. }) | Err(_) => {
                    context.unavailable_daa = Some(now);
                    builder.write_kernel_row(table, key, Some(encode(&context)))?;
                    continue;
                }
                Ok(_) => continue,
            };
            let id = context.context.commitment_root;
            let model = Hash64::from_bytes(context.model);
            let final_claims: Vec<Digest> = {
                let mut finals: Vec<Digest> = builder
                    .state
                    .bond_budget
                    .as_ref()
                    .map(|budget| {
                        budget
                            .claims
                            .iter()
                            .filter(|(_, row)| row.model == Some((model, context.epoch)) && row.accepted_daa >= from)
                            .filter(|(claim, _)| {
                                builder.state.claims.get(*claim).is_some_and(|c| matches!(c.phase, PalwClaimPhaseV2::Final { .. }))
                            })
                            .map(|(claim, _)| claim.as_bytes())
                            .collect()
                    })
                    .unwrap_or_default();
                finals.sort();
                finals.dedup();
                finals
            };
            let sampled = palw_model_verification_sample_v1(policy.sample_rule, &beacon, &id, &final_claims).map_err(refuse)?;
            // A producer never verifies its own sampled claim: its operator's bonds leave the pool.
            let excluded_operator = sampled
                .and_then(|c| builder.state.claims.get(&Hash64::from_bytes(c)))
                .and_then(|c| builder.state.bonds.get(&c.bond))
                .map(|b| b.operator_id);
            let floor = builder.params.min_collateral_sompi();
            let mut pool: Vec<(Digest, u64)> = builder
                .state
                .bonds
                .iter()
                .filter(|(_, b)| matches!(b.status, PalwBondStatusV2::Active) && b.collateral > 0 && b.collateral >= floor)
                .filter(|(_, b)| Some(b.operator_id) != excluded_operator)
                .map(|(key, b)| (palw_kernel_bond_id_v1(key), b.collateral))
                .collect();
            pool.sort();
            pool.dedup_by(|a, b| a.0 == b.0);
            if pool.is_empty() {
                context.unavailable_daa = Some(now);
                builder.write_kernel_row(table, key, Some(encode(&context)))?;
                continue;
            }
            // Capacity: the undecided tallies held at once. A context past capacity waits (its beacon stays locked).
            let pending = {
                let route = builder.state.kernel_route.as_ref().expect("created above");
                verification_rows(route, None)
                    .filter_map(|(_, bytes)| borsh::from_slice::<PalwModelVerificationTallyV1>(bytes).ok())
                    .filter(|t| t.outcome(policy.threshold, now) == PalwModelVerificationOutcomeV1::Pending)
                    .count()
            };
            if pending >= policy.maximum_open_tallies as usize {
                break;
            }
            open_model_verification_tally_v1(
                builder,
                ctx,
                &context.model,
                context.epoch,
                &beacon,
                &context.model_root,
                &final_claims,
                &pool,
            )?;
            builder.write_kernel_row(table, key, None)?;
        }
    }
    // 3. Take the decided tallies.
    let decided: Vec<Digest> = {
        let route = builder.state.kernel_route.as_ref().expect("created above");
        verification_rows(route, None)
            .filter_map(|(k, bytes)| borsh::from_slice::<PalwModelVerificationTallyV1>(bytes).ok().map(|t| (k, t)))
            .filter(|(_, t)| t.outcome(policy.threshold, now) != PalwModelVerificationOutcomeV1::Pending)
            .map(|(k, _)| k.as_slice().try_into().expect("64 bytes"))
            .collect()
    };
    for id in decided {
        take_model_verification_outcome_v1(builder, ctx, &id)?;
    }
    // 4. Release or drop the deferred rewards whose model-epoch decided.
    let deferred: Vec<(Vec<u8>, PalwModelVerificationDeferredV1)> = {
        let route = builder.state.kernel_route.as_ref().expect("created above");
        verification_rows(route, Some(MODEL_VERIFICATION_DEFERRED_PREFIX_V1))
            .map(|(key, bytes)| borsh::from_slice(bytes).map(|row| (key.clone(), row)).map_err(|e| refuse(e.to_string())))
            .collect::<Result<_, _>>()?
    };
    for (key, row) in &deferred {
        match model_epoch_outcome_v1(builder, &policy, &row.model, row.epoch, now) {
            PalwModelVerificationOutcomeV1::Pending => {}
            PalwModelVerificationOutcomeV1::Unattested => builder.write_kernel_row(table, key.clone(), None)?,
            PalwModelVerificationOutcomeV1::Attested | PalwModelVerificationOutcomeV1::NoFinalClaim => {
                let queue = palw_model_verification_deferred_payout_key_v1(&row.claim);
                if builder.state.pending_payouts.len() >= PALW_V2_MAX_PENDING_PAYOUTS
                    || builder.state.pending_payouts.contains_key(&queue)
                {
                    continue; // the queue drains first; the release waits (never dropped, never doubled)
                }
                // RFC-0008 once-only settlement (dormant): the deferral of this claim is released once across every consumer.
                {
                    use crate::palw_settlement_once_v1::{
                        PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
                    };
                    // RFC08-2: a reward drawn at the claim's Final and held by the gate — no new R draw; paid at once now.
                    let claim = Hash64::from_bytes(row.claim);
                    let final_daa = builder.state.claims.get(&claim).and_then(|c| match c.phase {
                        PalwClaimPhaseV2::Final { final_daa } => Some(final_daa),
                        _ => None,
                    });
                    let once = builder.settle_once_liable_v1(
                        now,
                        PalwSettlementObligationV1::of(PalwSettlementLegV1::GateDeferredRelease, row.claim),
                        PalwSettlementConsumerV1::VerificationGate,
                        row.amount,
                        PalwSettlementDebitV1::Held { amount: row.amount },
                        super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::V2Claim {
                            claim,
                            final_daa,
                            horizon: None,
                            draw: 0,
                            paid_now: row.amount,
                            exported_before: None,
                            vest_until: None,
                        },
                    )?;
                    if once.pays() {
                        builder.write_payout(queue, Some(PalwPayoutV2 { payload: row.payload, amount: row.amount }))?;
                    }
                }
                builder.write_kernel_row(table, key.clone(), None)?;
            }
        }
    }
    // 5. Retire what nothing reads any more.
    let waiting: BTreeSet<Digest> = {
        let route = builder.state.kernel_route.as_ref().expect("created above");
        verification_rows(route, Some(MODEL_VERIFICATION_DEFERRED_PREFIX_V1))
            .filter_map(|(_, bytes)| borsh::from_slice::<PalwModelVerificationDeferredV1>(bytes).ok())
            .map(|row| palw_model_verification_tally_id_v1(&row.model, row.epoch))
            .collect()
    };
    let retired: Vec<Vec<u8>> = {
        let route = builder.state.kernel_route.as_ref().expect("created above");
        let read = |model: &Digest, epoch: u64| {
            live.contains(&(Hash64::from_bytes(*model), epoch)) || waiting.contains(&palw_model_verification_tally_id_v1(model, epoch))
        };
        let mut out = Vec::new();
        for (key, bytes) in verification_rows(route, Some(MODEL_VERIFICATION_CONTEXT_PREFIX_V1)) {
            let row: PalwModelVerificationContextV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
            if !read(&row.model, row.epoch) {
                out.push(key.clone());
            }
        }
        for (key, bytes) in verification_rows(route, Some(MODEL_VERIFICATION_DECIDED_PREFIX_V1)) {
            let row: PalwModelVerificationDecidedV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
            if !read(&row.model, row.epoch) {
                out.push(key.clone());
            }
        }
        out
    };
    for key in retired {
        builder.write_kernel_row(table, key, None)?;
    }
    Ok(())
}

// ---- B. the consented horizon extension ------------------------------------------------------------------------------------------

/// **Inner 68**: one affected party's consent. The extension applies, atomically, when the last required party signs: the kernel claim's
/// liability lengthens (never past the claim-fixed deadline, never reopened, never shortened), the executor's EXEC income hold lengthens
/// with it, and a transferred claim's live provider leases must already cover the new horizon. No cash is cancelled or created.
pub(super) fn consent_horizon_extension_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    terms: &HorizonExtensionTermsV1,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let policy = *builder
        .params
        .econ_contracts_v1()
        .horizon_extension_at(now)
        .ok_or_else(|| refuse("palw_horizon_extension_v1 is not in force"))?;
    policy.validate().map_err(refuse)?;
    terms.validate().map_err(refuse)?;
    let me = palw_kernel_bond_id_v1(signer);
    let required = terms.required_parties();
    if !required.contains(&me) {
        return Err(refuse("the signer is not a party the extension affects"));
    }
    if now > terms.consent_by_daa || terms.consent_by_daa - now > policy.maximum_consent_window_daa {
        return Err(refuse("the consent lands outside its signed consent window"));
    }
    if terms.new_liability_until_daa - terms.current_liability_until_daa > policy.maximum_extension_daa {
        return Err(refuse("the extension lengthens the horizon more than the policy allows"));
    }
    let id = terms.id();
    let key = palw_horizon_extension_key_v1(&id);
    let claim_hash = Hash64::from_bytes(terms.claim);
    let (mut row, before, ledger) = {
        let route = builder.state.kernel_route.as_ref().ok_or_else(|| refuse("the kernel route is missing"))?;
        if terms.network != route.header.policy.network_domain {
            return Err(refuse("the extension names another network"));
        }
        let existing: Option<PalwHorizonExtensionRowV1> = route.aux_row(PALW_HORIZON_EXTENSION_TABLE_V1, &key);
        if existing.is_none() {
            let mut open = 0u32;
            for (_, bytes) in
                route.aux.range((PALW_HORIZON_EXTENSION_TABLE_V1, Vec::new())..(PALW_HORIZON_EXTENSION_TABLE_V1 + 1, Vec::new()))
            {
                let other: PalwHorizonExtensionRowV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                if other.applied_daa.is_none() && other.terms.consent_by_daa >= now {
                    open = open.checked_add(1).ok_or_else(|| refuse("extension count overflow"))?;
                }
            }
            if open >= policy.maximum_open_extensions {
                return Err(refuse("the open horizon extension capacity is exhausted"));
            }
        }
        let mut ledger = route
            .ledger_checked_v1()
            .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refuse))?;
        ledger.set_daa_v1(now);
        {
            let claim = ledger.claims.get(&terms.claim).ok_or_else(|| refuse("the extended claim is missing"))?;
            let poster =
                ledger.job_posters.get(&claim.job_id).copied().or_else(|| ledger.job_escrows.get(&claim.job_id).map(|e| e.poster));
            if claim.producer != terms.worker
                || poster != Some(terms.sponsor)
                || ledger.exec_reward_holds.get(&terms.claim).is_none_or(|hold| hold.owner_root != terms.root)
            {
                return Err(refuse("the extension names another worker, sponsor or EXEC root than the claim has"));
            }
            if claim.liability_until != Some(terms.current_liability_until_daa) || now > terms.current_liability_until_daa {
                return Err(refuse("the extension names a horizon the claim no longer has, or one already expired"));
            }
            // X3: the kernel claim's own claim-fixed bound, further capped (`min`) by the EXEC root native claim's X3
            // `dispute_hard_deadline` where `palw_claim_hard_deadline_v1` governs that claim (REQ-X3-2's `effective = min(proposed, hard)`).
            let root_x3 = builder
                .state
                .claims
                .get(&Hash64::from_bytes(terms.root))
                .and_then(|root| crate::palw_ruling_fences_v1::palw_claim_hard_deadline_of_v1(builder.params, root));
            let hard = palw_horizon_extension_hard_deadline_v1(claim.committed_daa, &policy)
                .map(|own| crate::palw_ruling_fences_v1::palw_claim_effective_deadline_v1(own, root_x3));
            if hard != Some(terms.hard_deadline_daa) {
                return Err(refuse("the extension's hard deadline is not the claim-fixed one"));
            }
            // §15.5: a transferred claim's providers consent through their own signed leases, which must already cover the new horizon.
            super::palw_kernel_route_fold_v1::require_exec_provider_horizon_v1(
                &builder.state,
                &claim_hash,
                claim,
                now,
                terms.new_liability_until_daa,
            )
            .map_err(refuse)?;
            if policy.require_price_contract || terms.price_contract != [0; 64] {
                let contract: PalwMinimumPriceRowV1 = route
                    .aux_row(PALW_MINIMUM_PRICE_TABLE_V1, &palw_minimum_price_contract_key_v1(&terms.price_contract))
                    .ok_or_else(|| refuse("the extension's minimum-price contract is missing"))?;
                if contract.signed_daa.is_none()
                    || contract.terms.worker != terms.worker
                    || contract.terms.poster != terms.sponsor
                    || contract.terms.job != claim.job_id
                    || contract.terms.retention_until_daa < terms.new_liability_until_daa
                {
                    return Err(refuse("the minimum-price contract does not price this extension's retention"));
                }
            }
        }
        let row = existing.unwrap_or(PalwHorizonExtensionRowV1 {
            terms: *terms,
            consented: Vec::new(),
            first_daa: now,
            applied_daa: None,
            policy: policy.id(),
        });
        if row.applied_daa.is_some() || row.consented.contains(&me) || row.policy != policy.id() {
            return Err(refuse("the extension is already applied, already signed by this party or under another policy"));
        }
        (row, route.rows.clone(), ledger)
    };
    row.consented.push(me);
    row.consented.sort();
    if row.consented == required {
        let mut ledger = ledger;
        ledger
            .extend_claim_liability_by_consent_v1(
                &terms.claim,
                terms.current_liability_until_daa,
                terms.new_liability_until_daa,
                terms.hard_deadline_daa,
            )
            .map_err(refuse)?;
        let bond = builder
            .state
            .kernel_route
            .as_ref()
            .and_then(|route| route.bond_key_of(&terms.worker))
            .ok_or_else(|| refuse("the extended claim's worker has no native bond"))?;
        // The hold first (it may refuse, journaling nothing); then the ledger rows.
        builder.extend_exec_income_hold_v1(Hash64::from_bytes(terms.root), bond, now, terms.new_liability_until_daa)?;
        super::palw_kernel_route_fold_v1::flush(builder, &ledger, &before)?;
        row.applied_daa = Some(now);
    }
    builder.write_kernel_row(PALW_HORIZON_EXTENSION_TABLE_V1, key, Some(encode(&row)))?;
    Ok(())
}

// ---- C. the minimum-price contract ------------------------------------------------------------------------------------------------

/// **Inner 69**: one party's signature on a minimum-price contract. Admission (both signatures) checks the job's actual funded escrow,
/// its class and poster, that no worker is reserved yet, and the derived minimum; the contract governs its job once both signed.
pub(super) fn sign_minimum_price_contract_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    terms: &MinimumPriceContractTermsV1,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let policy = builder
        .params
        .econ_contracts_v1()
        .minimum_price_at(now)
        .cloned()
        .ok_or_else(|| refuse("palw_minimum_price_v1 is not in force"))?;
    policy.validate().map_err(refuse)?;
    terms.validate().map_err(refuse)?;
    let me = palw_kernel_bond_id_v1(signer);
    let required = terms.required_parties();
    if !required.contains(&me) {
        return Err(refuse("the signer is not a party of the contract"));
    }
    let id = terms.id();
    let key = palw_minimum_price_contract_key_v1(&id);
    let job_key = palw_minimum_price_job_key_v1(&terms.job);
    let mut row = {
        let route = builder.state.kernel_route.as_ref().ok_or_else(|| refuse("the kernel route is missing"))?;
        if terms.network != route.header.policy.network_domain {
            return Err(refuse("the contract names another network"));
        }
        if route.aux.get(&(PALW_MINIMUM_PRICE_TABLE_V1, job_key.clone())).is_some_and(|other| other.as_slice() != id.as_slice()) {
            return Err(refuse("another minimum-price contract already governs the job"));
        }
        let existing: Option<PalwMinimumPriceRowV1> = route.aux_row(PALW_MINIMUM_PRICE_TABLE_V1, &key);
        if existing.is_none() {
            let count = route
                .aux
                .range((PALW_MINIMUM_PRICE_TABLE_V1, Vec::new())..(PALW_MINIMUM_PRICE_TABLE_V1 + 1, Vec::new()))
                .filter(|((_, k), _)| k.first() == Some(&MINIMUM_PRICE_CONTRACT_PREFIX_V1))
                .count();
            if count >= policy.maximum_contracts as usize {
                return Err(refuse("the minimum-price contract capacity is exhausted"));
            }
        }
        let ledger = route
            .ledger_checked_v1()
            .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refuse))?;
        let escrow = ledger.job_escrows.get(&terms.job).ok_or_else(|| refuse("the contracted job has no funded escrow"))?;
        let job = ledger.jobs.get(&terms.job).ok_or_else(|| refuse("a minimum-price contract covers a posted program job"))?;
        if escrow.poster != terms.poster || job.class_binding_id != terms.class {
            return Err(refuse("the contract names another poster or class than the job has"));
        }
        if ledger.job_claims.contains_key(&terms.job) {
            return Err(refuse("a worker is already reserved for the job: the contract must precede it"));
        }
        palw_minimum_price_admit_v1(&policy, terms, escrow.amount, now).map_err(|e| refuse(e.to_string()))?;
        let row = existing.unwrap_or(PalwMinimumPriceRowV1 {
            terms: *terms,
            consented: Vec::new(),
            first_daa: now,
            signed_daa: None,
            policy: policy.id(),
        });
        if row.signed_daa.is_some() || row.consented.contains(&me) {
            return Err(refuse("the contract is already signed by this party"));
        }
        row
    };
    row.consented.push(me);
    row.consented.sort();
    if row.consented == required {
        row.signed_daa = Some(now);
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    builder.write_kernel_row(PALW_MINIMUM_PRICE_TABLE_V1, key, Some(encode(&row)))?;
    builder.write_kernel_row(PALW_MINIMUM_PRICE_TABLE_V1, job_key, Some(id.to_vec()))?;
    Ok(())
}

/// **R8-ECON at claim admission**: while a minimum-price contract names a job, only the contracted worker's claim is admitted — and
/// only once every party signed, inside the contract's acceptance deadline, with the job's escrow still funding the minimum. Below the
/// fence nothing is read (A-2: the claim path is byte-identical).
pub(super) fn minimum_price_commit_gate_v1(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    object: &KernelRouteObjectV1,
) -> Result<(), PalwStateV2Error> {
    if builder.params.econ_contracts_v1().minimum_price_at(ctx.daa_score).is_none() {
        return Ok(());
    }
    let (job, producer) = match object {
        KernelRouteObjectV1::CommitClaim { claim, .. } | KernelRouteObjectV1::CommitSegmentedClaim { claim, .. } => {
            (claim.job_id, claim.producer_bond)
        }
        KernelRouteObjectV1::CommitClaimSalted { commit, .. } => match commit {
            SaltedCommitV1::Claim { claim, .. } | SaltedCommitV1::Segmented { claim, .. } => (claim.job_id, commit.producer()),
            _ => return Ok(()),
        },
        _ => return Ok(()),
    };
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let Some(id) = route.aux.get(&(PALW_MINIMUM_PRICE_TABLE_V1, palw_minimum_price_job_key_v1(&job))) else { return Ok(()) };
    let id: Digest = id.as_slice().try_into().map_err(|_| refuse("a malformed minimum-price job index"))?;
    let row: PalwMinimumPriceRowV1 = route
        .aux_row(PALW_MINIMUM_PRICE_TABLE_V1, &palw_minimum_price_contract_key_v1(&id))
        .ok_or_else(|| refuse("a minimum-price job index names no contract"))?;
    if row.signed_daa.is_none() {
        return Err(refuse("the job's minimum-price contract is not yet signed by every party"));
    }
    if row.terms.worker != producer {
        return Err(refuse("only the contracted worker's claim is admitted for this job"));
    }
    if ctx.daa_score > row.terms.accept_by_daa {
        return Err(refuse("the job's minimum-price contract is past its acceptance deadline"));
    }
    let ledger =
        route.ledger_checked_v1().map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refuse))?;
    if ledger.job_escrows.get(&job).is_none_or(|escrow| escrow.amount < row.terms.funded_minimum_sompi) {
        return Err(refuse("the job's escrow no longer funds the contracted minimum"));
    }
    Ok(())
}

// ---- C′. R8-ECON at settlement and the contracts' retirement (2026-10-15 round 3) ------------------------------------------------

/// **R8-ECON at settlement: the payout floors** `load_ledger` injects into the kernel ledger (`consumer_minimum_payouts_v1`), one per
/// signed contract whose worker's claim holds its job ([`palw_minimum_price_floor_v1`]). The ledger then pays a contracted Final at
/// least the funded minimum out of the job's escrow, or nothing (neutral: the escrow returns whole to the poster) — never less. Empty
/// below `palw_minimum_price_v1` (the ledger's payout is byte-identical).
pub(super) fn minimum_price_payout_floors_v1(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
) -> BTreeMap<Digest, u64> {
    let mut floors = BTreeMap::new();
    if builder.params.econ_contracts_v1().minimum_price_at(ctx.daa_score).is_none() {
        return floors;
    }
    let Some(route) = builder.state.kernel_route.as_ref() else { return floors };
    for ((_, key), bytes) in route.aux.range((PALW_MINIMUM_PRICE_TABLE_V1, Vec::new())..(PALW_MINIMUM_PRICE_TABLE_V1 + 1, Vec::new()))
    {
        if key.len() != 65 || key[0] != MINIMUM_PRICE_CONTRACT_PREFIX_V1 {
            continue;
        }
        // The restore fence refuses an undecodable row; a row that does not decode here sets no floor.
        let Ok(row) = borsh::from_slice::<PalwMinimumPriceRowV1>(bytes) else { continue };
        if let Some((claim, minimum)) = palw_minimum_price_floor_v1(&row, ledger) {
            floors.insert(claim, minimum);
        }
    }
    floors
}

/// **The contracts' deterministic sweep** (first in the kernel route's closing step, after the block's objects, at the block's DAA;
/// journaled through the route's aux rows): a minimum-price row retires by [`palw_minimum_price_retirement_v1`] (stale consent,
/// expired, unused, settled) with its job index; an extension row by [`palw_horizon_extension_retired_v1`] (stale consent, or applied
/// and past its consented horizon). Retiring a row frees its fence's capacity; it never moves cash, a claim or a horizon. A no-op below
/// both fences.
pub(super) fn sweep_econ_contracts_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let mirror = builder.params.econ_contracts_v1();
    let (price, horizon) = (mirror.minimum_price_at(now).is_some(), mirror.horizon_extension_at(now).is_some());
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let rows_of = |table: u8| route.aux.range((table, Vec::new())..(table + 1, Vec::new()));
    let price = price && rows_of(PALW_MINIMUM_PRICE_TABLE_V1).next().is_some();
    let horizon = horizon && rows_of(PALW_HORIZON_EXTENSION_TABLE_V1).next().is_some();
    if !price && !horizon {
        return Ok(());
    }
    let mut retire: Vec<(u8, Vec<u8>)> = Vec::new();
    if price {
        let ledger = route
            .ledger_checked_v1()
            .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refuse))?;
        for ((_, key), bytes) in rows_of(PALW_MINIMUM_PRICE_TABLE_V1) {
            if key.len() != 65 || key[0] != MINIMUM_PRICE_CONTRACT_PREFIX_V1 {
                continue;
            }
            let row: PalwMinimumPriceRowV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
            if palw_minimum_price_retirement_v1(&row, &ledger, now).is_some() {
                let id = row.terms.id();
                retire.push((PALW_MINIMUM_PRICE_TABLE_V1, key.clone()));
                let index = palw_minimum_price_job_key_v1(&row.terms.job);
                if route.aux.get(&(PALW_MINIMUM_PRICE_TABLE_V1, index.clone())).is_some_and(|named| named.as_slice() == id.as_slice())
                {
                    retire.push((PALW_MINIMUM_PRICE_TABLE_V1, index));
                }
            }
        }
    }
    if horizon {
        for ((_, key), bytes) in rows_of(PALW_HORIZON_EXTENSION_TABLE_V1) {
            let row: PalwHorizonExtensionRowV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
            if palw_horizon_extension_retired_v1(&row, now) {
                retire.push((PALW_HORIZON_EXTENSION_TABLE_V1, key.clone()));
            }
        }
    }
    for (table, key) in retire {
        builder.write_kernel_row(table, key, None)?;
    }
    Ok(())
}
