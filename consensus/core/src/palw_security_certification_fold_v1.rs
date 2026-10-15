//! **The fold of RFC-0018's `SecurityCertified` (Gate C, adopted 2026-10-15)** — kernel-route aux 136; the policy, rows and pure rules
//! are `crate::palw_security_certification_v1`. A child module of `palw_state_v2`, like RFC-0017's eligibility fold.
//!
//! ```text
//! commit_at_final_v1                 at a governed claim's Final: the certification row (claim, producer, pool cut at this DAA)
//! hold_issuance_reward_at_final_v1   the issuance-funded reward leg is HELD in the row (paid only at SecurityCertified)
//! try_attest_claim_certification_v1  inner 75 naming a certification id: one drawn bond's verdict on the claim
//! sweep_security_certification_v1    anchor → draw (global pool, epoch capacity) → verdict → on SecurityCertified: the claim's
//!                                    consensus credit (safe weight, settled anchor, the held reward); on failure the held reward
//!                                    is never named; decided rows retire with their claim
//! ```
//!
//! Every entry is a no-op below `palw_independently_work_certified_v1` (keyed on the claim's acceptance DAA for the claim hooks, on the
//! block's DAA for the sweep and the attestation). Consensus credit is added in exactly one place — the sweep's certification — and
//! every re-derivation reads the same frozen row through `palw_independently_work_certified_v1`.

use super::*;
use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
use crate::palw_model_eligibility_v1::{PalwEligibilityAttestationRowV1, PalwEligibilityAttestationV1, PalwEligibilityVerdictV1, PalwEligibilityVerifierV1};
use crate::palw_security_certification_v1::*;

fn refuse(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}

fn row_of<T: borsh::BorshDeserialize>(state: &PalwChainStateV2, key: &[u8]) -> Option<T> {
    state.kernel_route.as_ref()?.aux_row::<T>(PALW_SECURITY_CERTIFICATION_TABLE_V1, key)
}

fn write_row<T: borsh::BorshSerialize>(builder: &mut TransitionBuilder<'_>, key: Vec<u8>, row: Option<&T>) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row(
        PALW_SECURITY_CERTIFICATION_TABLE_V1,
        key,
        row.map(|r| borsh::to_vec(r).expect("a certification row serializes")),
    )?;
    Ok(())
}

fn policy_governing_claim(builder: &TransitionBuilder<'_>, claim: &PalwClaimStateV2) -> Option<PalwSecurityCertificationPolicyV1> {
    let mirror = builder.params.rfc0018_v1()?;
    mirror.gate_c_governs(claim.accepted_daa).then_some(mirror.certification)
}

/// **The commit** at a governed claim's `Final` (idempotent): the claim, its producer and the pool cut are fixed here; the draw waits
/// for a seed anchor strictly later than the one recorded now. Counted against the class (one attempt per claim, never retried).
pub(super) fn commit_at_final_v1(
    builder: &mut TransitionBuilder<'_>,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
    final_daa: u64,
) -> Result<(), PalwStateV2Error> {
    if policy_governing_claim(builder, claim).is_none() {
        return Ok(());
    }
    let id = palw_certification_id_v1(claim_id);
    let key = certification_claim_key_v1(&id);
    if row_of::<PalwClaimCertificationRowV1>(&builder.state, &key).is_some() {
        return Ok(());
    }
    // The rows live in the kernel route: create its header if this is its first row (`ensure_route_header` reads the DAA only).
    let at = PalwBlockContextV2 { block: Hash64::default(), daa_score: final_daa, blue_score: 0, subsidy: 0 };
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, &at)?;
    let bond = builder.state.bonds.get(&claim.bond).ok_or(PalwStateV2Error::MissingBond(claim.bond))?;
    let row = PalwClaimCertificationRowV1 {
        claim: claim_id.as_bytes(),
        class_id: claim.class_id.as_bytes(),
        producer_bond: palw_kernel_bond_id_v1(&claim.bond),
        producer_principal: bond.operator_id.as_bytes(),
        attempt: matches!(claim.source, PalwClaimSourceV2::Attempt),
        final_daa,
        committed_daa: final_daa,
        committed_span: builder.state.round_seed_anchor.as_ref().map(|anchor| anchor.span).unwrap_or(0),
        anchor: None,
        draws: Vec::new(),
        drawn_daa: 0,
        deadline_daa: 0,
        attestations: Vec::new(),
        held_reward: 0,
        payout_payload: bond.payout_payload.as_bytes(),
        outcome: None,
    };
    write_row(builder, key, Some(&row))?;
    let class_key = certification_class_key_v1(&row.class_id);
    let mut counters = row_of::<PalwCertificationClassRowV1>(&builder.state, &class_key).unwrap_or_default();
    counters.committed = counters.committed.saturating_add(1);
    write_row(builder, class_key, Some(&counters))?;
    Ok(())
}

/// **The commit of an owning39 MINING claim** (RFC-0002 §II.14): the same row, pool cut, anchor floor and counters as a chain
/// claim's, under the `'V'` prefix. Refused (not skipped) where Gate C does not govern: a native mining claim has no uncertified
/// path to consensus credit, so committing it outside the fence would only create a claim that can never decide.
pub(super) fn commit_native_mining_v39(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    mining_claim: &Hash64,
    model: &Hash64,
    producer: &PalwBondKeyV2,
    final_daa: u64,
) -> Result<(), PalwStateV2Error> {
    let mirror = builder.params.rfc0018_v1().copied().ok_or_else(|| refuse("native39 mining requires RFC-0018 Gate C in force"))?;
    if !mirror.gate_c_governs(ctx.daa_score) {
        return Err(refuse("native39 mining requires RFC-0018 Gate C to govern its acceptance"));
    }
    let id = palw_native_certification_id_v39(mining_claim);
    let key = certification_native_key_v39(&id);
    if row_of::<PalwClaimCertificationRowV1>(&builder.state, &key).is_some() {
        return Err(refuse("native39 mining claim already committed for certification"));
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    let bond = builder.state.bonds.get(producer).ok_or(PalwStateV2Error::MissingBond(*producer))?;
    let row = PalwClaimCertificationRowV1 {
        claim: mining_claim.as_bytes(),
        class_id: model.as_bytes(),
        producer_bond: palw_kernel_bond_id_v1(producer),
        producer_principal: bond.operator_id.as_bytes(),
        attempt: false,
        final_daa,
        committed_daa: ctx.daa_score,
        committed_span: builder.state.round_seed_anchor.as_ref().map(|anchor| anchor.span).unwrap_or(0),
        anchor: None,
        draws: Vec::new(),
        drawn_daa: 0,
        deadline_daa: 0,
        attestations: Vec::new(),
        held_reward: 0,
        payout_payload: bond.payout_payload.as_bytes(),
        outcome: None,
    };
    write_row(builder, key, Some(&row))?;
    let class_key = certification_class_key_v1(&row.class_id);
    let mut counters = row_of::<PalwCertificationClassRowV1>(&builder.state, &class_key).unwrap_or_default();
    counters.committed = counters.committed.saturating_add(1);
    write_row(builder, class_key, Some(&counters))?;
    Ok(())
}

/// The native fold's collection of a decided mining claim retires its `'V'` row (its receipt no longer reads it).
pub(super) fn retire_native_mining_v39(builder: &mut TransitionBuilder<'_>, mining_claim: &Hash64) -> Result<(), PalwStateV2Error> {
    let key = certification_native_key_v39(&palw_native_certification_id_v39(mining_claim));
    if row_of::<PalwClaimCertificationRowV1>(&builder.state, &key).is_some_and(|row| row.outcome.is_some()) {
        write_row::<PalwClaimCertificationRowV1>(builder, key, None)?;
    }
    Ok(())
}

/// **The coinbase site at `Final`**: past the fence the issuance-funded reward leg (the attempt escrow carved from block subsidy)
/// represents consensus credit — it is held in the claim's row and `0` is returned (nothing is named now). Below the fence `escrow`.
pub(super) fn hold_issuance_reward_at_final_v1(
    builder: &mut TransitionBuilder<'_>,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
    escrow: u64,
) -> Result<u64, PalwStateV2Error> {
    if policy_governing_claim(builder, claim).is_none() || escrow == 0 {
        return Ok(escrow);
    }
    let key = certification_claim_key_v1(&palw_certification_id_v1(claim_id));
    let mut row: PalwClaimCertificationRowV1 =
        row_of(&builder.state, &key).ok_or_else(|| refuse("a governed Final has no certification row (commit_at_final_v1 runs first)"))?;
    row.held_reward = row.held_reward.checked_add(escrow).ok_or(PalwStateV2Error::Overflow("held certification reward"))?;
    write_row(builder, key, Some(&row))?;
    Ok(0)
}

/// **Inner 75 naming a certification id** — `None` where the id names no certification row (the attestation is RFC-0017's), else the
/// result of recording one drawn bond's verdict on the claim. A `Reproduced` verdict that names claim index 0 as mismatching is
/// recorded with an all-zero transcript (a negative position).
pub(super) fn try_attest_claim_certification_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    attestation: &PalwEligibilityAttestationV1,
) -> Option<Result<(), PalwStateV2Error>> {
    let mirror = builder.params.rfc0018_v1().copied()?;
    if !mirror.gate_c_at(ctx.daa_score) {
        return None;
    }
    // A chain claim's row, else an owning39 mining claim's (`'V'`): one attestation object, one rule.
    let key = Some(certification_claim_key_v1(&attestation.exam))
        .filter(|k| row_of::<PalwClaimCertificationRowV1>(&builder.state, k).is_some())
        .unwrap_or_else(|| certification_native_key_v39(&attestation.exam));
    let mut row: PalwClaimCertificationRowV1 = row_of(&builder.state, &key)?;
    Some((|| {
        let now = ctx.daa_score;
        attestation.validate().map_err(refuse)?;
        if attestation.verifier != palw_kernel_bond_id_v1(signer) {
            return Err(refuse("the attestation's verifier is not its signer"));
        }
        let record = builder.state.bonds.get(signer).ok_or(PalwStateV2Error::MissingBond(*signer))?;
        if !matches!(record.status, PalwBondStatusV2::Active) {
            return Err(refuse("a certification attestation needs an Active bond"));
        }
        if row.outcome.is_some() || row.anchor.is_none() {
            return Err(refuse("the certification is not drawn yet, or already decided"));
        }
        if now > row.deadline_daa {
            return Err(refuse("the certification's attestation deadline has passed"));
        }
        if !row.draws.contains(&attestation.verifier) {
            return Err(refuse("the attesting bond was not drawn for this certification"));
        }
        if row.attestations.iter().any(|a| a.verifier == attestation.verifier) {
            return Err(refuse("the drawn bond already attested this certification"));
        }
        let mismatch = attestation.claim_mismatches.contains(&0);
        if attestation.claim_mismatches.iter().any(|i| *i != 0) {
            return Err(refuse("a certification examines one claim: only index 0 may mismatch"));
        }
        let positive = matches!(attestation.verdict, PalwEligibilityVerdictV1::Reproduced) && !mismatch;
        if positive && attestation.transcript == [0u8; 64] {
            return Err(refuse("a positive certification attestation commits a transcript"));
        }
        row.attestations.push(PalwEligibilityAttestationRowV1 {
            verifier: attestation.verifier,
            verdict: attestation.verdict,
            transcript: if positive { attestation.transcript } else { [0u8; 64] },
            attested_daa: now,
        });
        write_row(builder, key.clone(), Some(&row))?;
        Ok(())
    })())
}

/// The global verifier pool at the commit's cut: every Active bond matured before the commit, never the producer's operator. NOT
/// filtered by model possession — the registrant cannot fill it with its own holders.
fn pool_v1(
    state: &PalwChainStateV2,
    policy: &PalwSecurityCertificationPolicyV1,
    row: &PalwClaimCertificationRowV1,
) -> Vec<PalwEligibilityVerifierV1> {
    state
        .bonds
        .iter()
        .filter(|(_, record)| {
            palw_certification_pool_admits_v1(
                policy,
                row.committed_daa,
                &row.producer_principal,
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

/// **The frontier floor's two writers** (journaled kernel rows): at the first block Gate C is in force, the floor is seeded with the
/// frontier the chain stood at (so activation never moves it back); afterwards a frontier-eligible claim that RETIRES raises it to
/// its own position (a retired claim can no longer be convicted). A no-op below the fence.
pub(super) fn ensure_frontier_floor_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    frontier: (u64, Hash64),
) -> Result<(), PalwStateV2Error> {
    if !builder.params.rfc0018_v1().is_some_and(|mirror| mirror.gate_c_at(ctx.daa_score)) {
        return Ok(());
    }
    if palw_certified_frontier_floor_v1(&builder.state).is_some() {
        return Ok(());
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    let floor = PalwCertifiedFrontierFloorV1 { blue_score: frontier.0, block: frontier.1.as_bytes() };
    write_row(builder, certification_frontier_floor_key_v1(), Some(&floor))?;
    Ok(())
}

/// `retire_claim`'s hook: a retiring frontier-eligible `Final` claim raises the floor (it is past every conviction window).
pub(super) fn note_retirement_frontier_floor_v1(
    builder: &mut TransitionBuilder<'_>,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
) -> Result<(), PalwStateV2Error> {
    let Some(floor) = palw_certified_frontier_floor_v1(&builder.state) else { return Ok(()) };
    if !matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
        || !crate::palw_rfc0018_v1::palw_gate_c_advances_frontier_v1(builder.params, &builder.state, claim_id, claim)
        || claim.accepted_blue_score <= floor.blue_score
    {
        return Ok(());
    }
    let raised = PalwCertifiedFrontierFloorV1 { blue_score: claim.accepted_blue_score, block: claim.accepted_block.as_bytes() };
    write_row(builder, certification_frontier_floor_key_v1(), Some(&raised))?;
    Ok(())
}

/// **The consensus credit of a claim that just certified** — exactly the expressions the re-derivations read (consistency, retirement,
/// reversal), so the state's accumulators and every re-derivation agree: an attempt's Final safe weight (canonical basis, F-W ceiling,
/// the budget's grant) and one settled anchor; a free-prompt claim's spends so far. Then the held issuance-funded reward is named.
fn credit_certified_v1(
    builder: &mut TransitionBuilder<'_>,
    now: u64,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
    row: &PalwClaimCertificationRowV1,
) -> Result<(), PalwStateV2Error> {
    let weight = match &claim.source {
        PalwClaimSourceV2::Attempt => {
            let canonical = builder.canonical_claim_weight(claim);
            let contribution =
                palw_claim_safe_contribution_v3(&builder.state.class_shares, claim, builder.uncertified_weightless, canonical);
            let contribution = crate::palw_weight_cap_v1::palw_weight_final_safe_v1(builder.params, claim, contribution);
            builder.state.bond_budget_weight_or(claim_id, contribution)
        }
        PalwClaimSourceV2::FreePrompt { quanta, spent } => {
            let per_quantum = palw_fp_spend_weight_v1(&builder.state, claim, *quanta, &builder.fp_pricing());
            let spent_weight =
                per_quantum.checked_mul(spent.len() as u128).ok_or(PalwStateV2Error::Overflow("certified free-prompt weight"))?;
            builder.state.bond_budget_weight_or(claim_id, spent_weight)
        }
    };
    builder.state.safe_weight = builder.state.safe_weight.checked_add(weight).ok_or(PalwStateV2Error::Overflow("safe_weight"))?;
    if matches!(claim.source, PalwClaimSourceV2::Attempt) {
        builder.settle_anchor(now, true)?;
    }
    // RFC-0008 once-only settlement (dormant): the held issuance reward is named once across every consumer. This guards the
    // PAYMENT only; the weight credit above is Gate C's own (SecurityCertified) and is neither added nor read here.
    let releases = row.held_reward > 0 && {
        use crate::palw_settlement_once_v1::{
            PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
        };
        builder
            .settle_once_v1(
                now,
                PalwSettlementObligationV1::of(PalwSettlementLegV1::CertificationRelease, claim_id.as_bytes()),
                PalwSettlementConsumerV1::SecurityCertification,
                row.held_reward,
                PalwSettlementDebitV1::Held { amount: row.held_reward },
            )?
            .pays()
    };
    if releases {
        let key = palw_certification_payout_key_v1(claim_id);
        let held = builder.state.pending_payouts.get(&key).map(|p| p.amount).unwrap_or(0);
        let amount = held.checked_add(row.held_reward).ok_or(PalwStateV2Error::Overflow("certified reward"))?;
        builder.write_payout(key, Some(PalwPayoutV2 { payload: Hash64::from_bytes(row.payout_payload), amount }))?;
    }
    Ok(())
}

/// **The sweep** (the kernel route's closing step): draw every committed certification once a later seed anchor exists and the epoch's
/// draw capacity allows; decide every drawn one; credit the certified; retire decided rows whose claim has left the state. A no-op below
/// the fence.
pub(super) fn sweep_security_certification_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let Some(mirror) = builder.params.rfc0018_v1().copied() else { return Ok(()) };
    if !mirror.gate_c_at(now) {
        return Ok(());
    }
    let policy = mirror.certification;
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let table = PALW_SECURITY_CERTIFICATION_TABLE_V1;
    let rows: Vec<(Vec<u8>, Vec<u8>)> =
        route.aux.range((table, Vec::new())..(table + 1, Vec::new())).map(|((_, key), bytes)| (key.clone(), bytes.clone())).collect();
    let anchor = builder.state.round_seed_anchor.clone();
    let epoch = now / policy.epoch_daa.max(1);
    let mut network = row_of::<PalwCertificationNetworkRowV1>(&builder.state, &certification_network_key_v1()).unwrap_or_default();
    if network.epoch != epoch {
        network = PalwCertificationNetworkRowV1 { epoch, draws: 0 };
    }
    let network_before = network.clone();
    for (key, bytes) in rows {
        let native = key.first().copied() == Some(CERTIFICATION_NATIVE_MINING_PREFIX_V39);
        if !(key.first().copied() == Some(CERTIFICATION_CLAIM_PREFIX_V1) || native) || key.len() != 65 {
            continue;
        }
        let mut row: PalwClaimCertificationRowV1 = borsh::from_slice(&bytes).map_err(|e| refuse(e.to_string()))?;
        let claim_id = Hash64::from_bytes(row.claim);
        let claim = if native { None } else { builder.state.claims.get(&claim_id).cloned() };
        // An owning39 mining claim stands while its native claim row and its complete Master escrow Final stand.
        let native_live = native && super::palw_native_mining_fold_v39::native_mining_claim_live_v39(&builder.state, &claim_id);
        if row.outcome.is_some() {
            // A decided native row is retired by the native fold's own collection (its receipt reads it), never here.
            if claim.is_none() && !native {
                write_row::<PalwClaimCertificationRowV1>(builder, key, None)?;
            }
            continue;
        }
        let mut changed = false;
        // (Round 5, user decision 2026-10-15: U-2's Probation bar has no subject — model review is abolished — and is removed; every
        // tier's claim is drawn and certified alike.)
        if row.anchor.is_none()
            && !palw_certification_public_check_v1(&row.class_id)
            && network.draws < policy.draws_per_epoch
            && let Some(a) = anchor.as_ref()
            && a.span >= row.committed_span.saturating_add(policy.anchor_delay_spans.max(1))
        {
            let id = if native { palw_native_certification_id_v39(&claim_id) } else { palw_certification_id_v1(&claim_id) };
            let execution_key = a.execution_key.as_bytes();
            let seed = palw_certification_seed_v1(&id, a.span, &execution_key);
            row.anchor = Some((a.span, execution_key));
            row.drawn_daa = now;
            row.deadline_daa = now.saturating_add(policy.attest_window_daa);
            network.draws = network.draws.saturating_add(1);
            match palw_certification_draw_v1(&seed, &id, &pool_v1(&builder.state, &policy, &row), policy.m) {
                Some(draws) => row.draws = draws,
                None => row.outcome = Some((PalwCertificationOutcomeV1::NoPopulation, now)),
            }
            changed = true;
        }
        if row.outcome.is_none() {
            let facts = if native {
                // Native courts settle inside the component jobs before their escrow Final; the Master Final row's presence is
                // the standing fact, and no chain accusation can name a native mining claim.
                PalwCertificationClaimFactsV1 { still_final: native_live, accusation_pending: false }
            } else {
                PalwCertificationClaimFactsV1 {
                    still_final: claim.as_ref().is_some_and(|c| matches!(c.phase, PalwClaimPhaseV2::Final { .. })),
                    accusation_pending: builder.state.palw_accusation_pending_v1(&claim_id),
                }
            };
            if let Some(outcome) = palw_certification_verdict_v1(&policy, &row, facts, now) {
                row.outcome = Some((outcome, now));
                changed = true;
            }
        }
        if let (Some((outcome, _)), true) = (row.outcome, changed) {
            if outcome.certifies() {
                // Write the decided row FIRST: the credit's own reads (`palw_independently_work_certified_v1` in any re-derivation
                // this block runs) then see the claim certified.
                write_row(builder, key.clone(), Some(&row))?;
                // Credit only what the ONE predicate certifies (the U-2 bar, a void, the fence — all inside it), so the credit and every
                // re-derivation read the same answer.
                if let Some(claim) = claim.as_ref()
                    && crate::palw_rfc0018_v1::palw_independently_work_certified_v1(
                        &claim_id,
                        claim,
                        &crate::palw_rfc0018_v1::PalwGateCViewV1::of(builder.params, &builder.state),
                    )
                {
                    credit_certified_v1(builder, now, &claim_id, claim, &row)?;
                }
            }
            let class_key = certification_class_key_v1(&row.class_id);
            let mut counters = row_of::<PalwCertificationClassRowV1>(&builder.state, &class_key).unwrap_or_default();
            if outcome.certifies() {
                counters.certified = counters.certified.saturating_add(1);
            } else {
                // The held issuance-funded reward is never named (burned: never minted), as a voided escrow's.
                counters.failed = counters.failed.saturating_add(1);
            }
            write_row(builder, class_key, Some(&counters))?;
        }
        if changed {
            write_row(builder, key, Some(&row))?;
        }
    }
    if network != network_before {
        write_row(builder, certification_network_key_v1(), Some(&network))?;
    }
    Ok(())
}
