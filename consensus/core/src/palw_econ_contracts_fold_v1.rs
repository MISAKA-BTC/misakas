//! **The fold of the RFC07/RFC08 signed economic contracts** (inner 67–69 into kernel route aux 125–127; `crate::palw_econ_contracts_v1`
//! holds the policies, pure rules and the restore fence). A child module of `palw_state_v2`, like the kernel route's own fold. Every
//! entry refuses below its own dormant fence (the processor's A-2 gate drops the object first), and every refusal writes nothing: the
//! checks run against a local ledger copy before the first journaled write.

use super::*;
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

// ---- A. the verification-attestation gate ----------------------------------------------------------------------------------------

/// **Consumer hook: open a model-epoch tally** from a VERIFIED beacon output (OPVB's v3 `ClaimVerification` beacon at the allocation
/// epoch's close), the model's canonical Final claims of that epoch and the canonical global verifier pool. Nothing in this tree calls it
/// yet: the beacon wiring is the remaining item (as M\*-49's `assign_drawn_v1`). A tally opens once; capacity bounds the undecided ones.
#[allow(dead_code)]
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
        for (_, bytes) in
            route.aux.range((PALW_MODEL_VERIFICATION_TABLE_V1, Vec::new())..(PALW_MODEL_VERIFICATION_TABLE_V1 + 1, Vec::new()))
        {
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
    builder.write_kernel_row(PALW_MODEL_VERIFICATION_TABLE_V1, id.to_vec(), Some(encode(&row)));
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
    builder.write_kernel_row(PALW_MODEL_VERIFICATION_TABLE_V1, attestation.tally.to_vec(), Some(encode(&row)));
    Ok(())
}

/// **Consumer hook: take a decided tally's outcome** for the allocation payout ([`palw_model_verification_subsidy_v1`]). A pending
/// tally stays (and answers `Pending`); a decided one is removed with its answer. The payout integration is the remaining item.
#[allow(dead_code)]
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
        builder.write_kernel_row(PALW_MODEL_VERIFICATION_TABLE_V1, tally.to_vec(), None);
    }
    Ok(Some(outcome))
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
        let mut ledger = route.ledger().map_err(refuse)?;
        ledger.daa = now;
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
        super::palw_kernel_route_fold_v1::flush(builder, &ledger, &before);
        row.applied_daa = Some(now);
    }
    builder.write_kernel_row(PALW_HORIZON_EXTENSION_TABLE_V1, key, Some(encode(&row)));
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
        let ledger = route.ledger().map_err(refuse)?;
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
    builder.write_kernel_row(PALW_MINIMUM_PRICE_TABLE_V1, key, Some(encode(&row)));
    builder.write_kernel_row(PALW_MINIMUM_PRICE_TABLE_V1, job_key, Some(id.to_vec()));
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
    let ledger = route.ledger().map_err(refuse)?;
    if ledger.job_escrows.get(&job).is_none_or(|escrow| escrow.amount < row.terms.funded_minimum_sompi) {
        return Err(refuse("the job's escrow no longer funds the contracted minimum"));
    }
    Ok(())
}
