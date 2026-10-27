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
use crate::palw_model_eligibility_v1::{
    PalwEligibilityAttestationRowV1, PalwEligibilityAttestationV1, PalwEligibilityVerdictV1, PalwEligibilityVerifierV1,
};
use crate::palw_security_certification_v1::*;

fn refuse(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}

fn row_of<T: borsh::BorshDeserialize>(state: &PalwChainStateV2, key: &[u8]) -> Option<T> {
    state.kernel_route.as_ref()?.aux_row::<T>(PALW_SECURITY_CERTIFICATION_TABLE_V1, key)
}

fn write_row<T: borsh::BorshSerialize>(
    builder: &mut TransitionBuilder<'_>,
    key: Vec<u8>,
    row: Option<&T>,
) -> Result<(), PalwStateV2Error> {
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

/// **lane CERT (2026-10-26): a kernel-route claim's `Final` reward is held for SecurityCertified.** Called from the kernel fold's
/// settlement at the producer's escrow `FinalReward` (never a verifier check fee, never the EXEC fee route). Below Gate C (`gate_c_at`
/// the block): `Ok(false)` and the released rule pays at `Final`. Past it: the claim's `'K'` certification row is committed (claim,
/// producer, pool cut at this DAA, exactly as a chain claim's at its `Final`) and the reward, already debited from the poster's
/// escrow, is held in its `'H'` row; `Ok(true)` and nothing is paid now. A second hold of the same claim is refused (the once-only
/// marker before this already stops it; this refusal is the hold's own invariant, not a fallback).
pub(super) fn hold_kernel_final_reward_v1(
    builder: &mut TransitionBuilder<'_>,
    now: u64,
    kernel_claim: &[u8; 64],
    producer: &[u8; 64],
    poster: &[u8; 64],
    amount: u64,
) -> Result<bool, PalwStateV2Error> {
    let Some(mirror) = builder.params.rfc0018_v1().copied() else { return Ok(false) };
    if !mirror.gate_c_at(now) {
        return Ok(false);
    }
    let id = palw_kernel_claim_certification_id_v1(kernel_claim);
    let key = certification_kernel_claim_key_v1(&id);
    let hold_key = certification_kernel_hold_key_v1(&id);
    if row_of::<PalwClaimCertificationRowV1>(&builder.state, &key).is_some()
        || row_of::<PalwKernelCertifiedRewardHoldV1>(&builder.state, &hold_key).is_some()
    {
        return Err(refuse("a kernel claim's Final reward is held for certification once"));
    }
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| refuse("kernel certification: no kernel route"))?;
    let producer_key = route.bond_key_of(producer).ok_or_else(|| refuse("kernel certification: the producer has no native bond"))?;
    let poster_key = route.bond_key_of(poster).ok_or_else(|| refuse("kernel certification: the poster has no native bond"))?;
    let class_id = route
        .ledger_checked_v1()
        .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refuse))?
        .claims
        .get(kernel_claim)
        .map(|row| row.class_binding_id)
        .ok_or_else(|| refuse("kernel certification: the Final claim is not in the ledger"))?;
    let producer_bond = builder.state.bonds.get(&producer_key).ok_or(PalwStateV2Error::MissingBond(producer_key))?;
    let poster_payload = builder.state.bonds.get(&poster_key).ok_or(PalwStateV2Error::MissingBond(poster_key))?.payout_payload;
    let row = PalwClaimCertificationRowV1 {
        claim: *kernel_claim,
        class_id,
        producer_bond: *producer,
        producer_principal: producer_bond.operator_id.as_bytes(),
        // A kernel claim is user-paid work (no issuance leg); its held reward is the escrow's, kept in the 'H' row.
        attempt: false,
        final_daa: now,
        committed_daa: now,
        committed_span: builder.state.round_seed_anchor.as_ref().map(|anchor| anchor.span).unwrap_or(0),
        anchor: None,
        draws: Vec::new(),
        drawn_daa: 0,
        deadline_daa: 0,
        attestations: Vec::new(),
        held_reward: 0,
        payout_payload: producer_bond.payout_payload.as_bytes(),
        outcome: None,
    };
    let hold = PalwKernelCertifiedRewardHoldV1 {
        version: PALW_KERNEL_CERTIFIED_REWARD_HOLD_VERSION_V1,
        claim: *kernel_claim,
        producer_bond: *producer,
        producer_payload: producer_bond.payout_payload.as_bytes(),
        poster_bond: *poster,
        poster_payload: poster_payload.as_bytes(),
        amount,
        held_daa: now,
    };
    let class_key = certification_class_key_v1(&row.class_id);
    write_row(builder, key, Some(&row))?;
    write_row(builder, hold_key, Some(&hold))?;
    let mut counters = row_of::<PalwCertificationClassRowV1>(&builder.state, &class_key).unwrap_or_default();
    counters.committed = counters.committed.saturating_add(1);
    write_row(builder, class_key, Some(&counters))?;
    Ok(true)
}

/// **lane CERT: the held kernel Final reward settles once, at the certification's decision.** Certifying: paid to the producer exactly
/// as the released `Final` paid it (the bond-budget recipient, the RFC-0016 telemetry, the kernel payout). Any other outcome
/// (`NotCertified`, `Expired`, `NoPopulation`, `Withdrawn`; a conviction after `Final` withdraws): the V3 burn share is burned and the
/// rest is returned to the poster ([`palw_kernel_uncertified_split_v1`], provisional); the
/// producer is never paid uncertified. A producer whose bond left the route before certification is not paid either (returned).
fn settle_kernel_reward_hold_v1(
    builder: &mut TransitionBuilder<'_>,
    now: u64,
    kernel_claim: &[u8; 64],
    certified: bool,
) -> Result<(), PalwStateV2Error> {
    let hold_key = certification_kernel_hold_key_v1(&palw_kernel_claim_certification_id_v1(kernel_claim));
    let Some(hold) = row_of::<PalwKernelCertifiedRewardHoldV1>(&builder.state, &hold_key) else {
        return Err(refuse("a decided kernel certification has no held reward"));
    };
    write_row::<PalwKernelCertifiedRewardHoldV1>(builder, hold_key, None)?;
    let producer_key = builder.state.kernel_route.as_ref().and_then(|route| route.bond_key_of(&hold.producer_bond));
    match (certified, producer_key) {
        (true, Some(bond)) => {
            let payload = Hash64::from_bytes(hold.producer_payload);
            builder.consume_kernel_final_reward_v1(kernel_claim, bond, payload, now, hold.amount)?;
            super::palw_rfc0016_fold_v1::rfc0016_note_escrow_payout_v1(builder, now, kernel_claim, hold.amount)?;
            builder.add_kernel_payout(payload, hold.amount)
        }
        // PROVISIONAL (coordinator, 2026-10-26, value delegation): the V3 burn share is never named (burned; the poster's debit
        // stands), the rest returns to the poster. Neither goes to the producer.
        _ => builder.add_kernel_payout(Hash64::from_bytes(hold.poster_payload), palw_kernel_uncertified_split_v1(hold.amount).1),
    }
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
    let mut row: PalwClaimCertificationRowV1 = row_of(&builder.state, &key)
        .ok_or_else(|| refuse("a governed Final has no certification row (commit_at_final_v1 runs first)"))?;
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
    // lane CERT: or a kernel-route ledger claim's (`'K'`).
    let key = [certification_claim_key_v1(&attestation.exam), certification_kernel_claim_key_v1(&attestation.exam)]
        .into_iter()
        .find(|k| row_of::<PalwClaimCertificationRowV1>(&builder.state, k).is_some())
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
        // G14 lane closure G-N2: a Reproduced attestor of an owning39 mining claim answers for it — a lock on its free collateral
        // (the seat-lock relation over the claim's reserved exposure and the k-of-m quorum) through the claim's liability; a
        // conviction charges it. An attestor that cannot back it is not counted.
        if positive && key.first().copied() == Some(CERTIFICATION_NATIVE_MINING_PREFIX_V39) {
            native_attestor_lock_v39(builder, ctx, signer, &Hash64::from_bytes(row.claim))?;
        }
        // lane GC-DUTY (`palw_gate_c_attestor_liability_v1`): a positive attestor of a chain (`'S'`) or kernel (`'K'`) claim answers
        // for it the same way — a lock over the claim's gain; a court conviction of the claim charges it. Negative or withheld
        // attestations lock nothing (withholding is no computation fault).
        if positive && mirror.attestor_liability_at(now) {
            let gain = match key.first().copied() {
                Some(CERTIFICATION_CLAIM_PREFIX_V1) => {
                    let reserved = builder.state.claims.get(&Hash64::from_bytes(row.claim)).map(|c| c.reserved).unwrap_or(0);
                    Some(u64::try_from(reserved).unwrap_or(u64::MAX).max(row.held_reward))
                }
                Some(CERTIFICATION_KERNEL_CLAIM_PREFIX_V1) => Some(
                    row_of::<PalwKernelCertifiedRewardHoldV1>(
                        &builder.state,
                        &certification_kernel_hold_key_v1(&palw_kernel_claim_certification_id_v1(&row.claim)),
                    )
                    .map(|h| h.amount)
                    .unwrap_or(0),
                ),
                _ => None,
            };
            if let Some(gain) = gain {
                attestor_lock_v1(builder, ctx, signer, &row.claim, gain)?;
            }
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

/// **G-N2: the lock a Reproduced attestor of a native mining claim holds** (aux 143, `0x03 ‖ attestor ‖ claim`, counted in the
/// committed ledger with the kernel signer locks).
fn native_attestor_lock_v39(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    claim: &Hash64,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_g14_lane_closure_v1::*;
    let mirror = builder.params.rfc0018_v1().copied().ok_or_else(|| refuse("native attestor lock requires RFC-0018"))?;
    let exposure = super::palw_native_mining_fold_v39::claim(builder.state.kernel_route.as_ref().unwrap(), &claim.as_bytes())?
        .price
        .reserved_exposure;
    let amount = palw_kernel_signer_lock_amount_v1(u64::try_from(exposure).unwrap_or(u64::MAX), mirror.certification.k.min(255) as u8);
    let record = builder.state.bonds.get(signer).ok_or(PalwStateV2Error::MissingBond(*signer))?;
    if (record.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < amount as u128 {
        return Err(refuse("the attestor cannot back its lock on the native mining claim (G-N2)"));
    }
    let key = palw_native_attestor_lock_key_v1(&palw_kernel_bond_id_v1(signer), &claim.as_bytes());
    let row = PalwKernelSignerLockRowV1 {
        version: PALW_G14_LANE_CLOSURE_ROW_VERSION_V1,
        claim: claim.as_bytes(),
        seat: *signer,
        amount,
        locked_daa: ctx.daa_score,
    };
    builder.write_kernel_row(PALW_G14_LANE_CLOSURE_TABLE_V1, key, Some(borsh::to_vec(&row).expect("fixed lock row")))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}
/// **lane GC-DUTY: the lock a positive `'S'` / `'K'` attestor holds** — the G-N2 relation and key (aux 143, `0x03 ‖ attestor ‖
/// claim`, counted in the committed ledger by `signer_locks_of_v1`) over `gain`. An attestor that cannot back it is not counted.
fn attestor_lock_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    claim: &[u8; 64],
    gain: u64,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_g14_lane_closure_v1::*;
    let mirror = builder.params.rfc0018_v1().copied().ok_or_else(|| refuse("attestor liability requires RFC-0018"))?;
    let amount = palw_kernel_signer_lock_amount_v1(gain, mirror.certification.k.min(255) as u8);
    if amount == 0 {
        return Ok(());
    }
    let record = builder.state.bonds.get(signer).ok_or(PalwStateV2Error::MissingBond(*signer))?;
    if (record.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < amount as u128 {
        return Err(refuse("the attestor cannot back its Gate C liability lock (GC-DUTY)"));
    }
    let key = palw_native_attestor_lock_key_v1(&palw_kernel_bond_id_v1(signer), claim);
    let row = PalwKernelSignerLockRowV1 {
        version: PALW_G14_LANE_CLOSURE_ROW_VERSION_V1,
        claim: *claim,
        seat: *signer,
        amount,
        locked_daa: ctx.daa_score,
    };
    builder.write_kernel_row(PALW_G14_LANE_CLOSURE_TABLE_V1, key, Some(borsh::to_vec(&row).expect("fixed lock row")))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}

/// lane GC-DUTY: the attestor locks held on the claim of certification row `row_key` (any prefix).
fn attestor_locks_of_row_v1(
    builder: &TransitionBuilder<'_>,
    row_key: &[u8],
) -> Vec<(Vec<u8>, crate::palw_g14_lane_closure_v1::PalwKernelSignerLockRowV1)> {
    use crate::palw_g14_lane_closure_v1::*;
    let Some(route) = builder.state.kernel_route.as_ref() else { return Vec::new() };
    let Some(row) = row_of::<PalwClaimCertificationRowV1>(&builder.state, row_key) else { return Vec::new() };
    row.attestations
        .iter()
        .filter_map(|a| {
            let key = palw_native_attestor_lock_key_v1(&a.verifier, &row.claim);
            let lock: PalwKernelSignerLockRowV1 =
                borsh::from_slice(route.aux.get(&(PALW_G14_LANE_CLOSURE_TABLE_V1, key.clone()))?).ok()?;
            Some((key, lock))
        })
        .collect()
}

/// **lane GC-DUTY: a conviction charges every positive attestor's lock** on the claim of `row_key` (burned). Returns the total.
pub(super) fn charge_attestor_locks_of_row_v1(builder: &mut TransitionBuilder<'_>, row_key: &[u8]) -> Result<u64, PalwStateV2Error> {
    let mut total = 0u64;
    for (key, lock) in attestor_locks_of_row_v1(builder, row_key) {
        builder.write_kernel_row(crate::palw_g14_lane_closure_v1::PALW_G14_LANE_CLOSURE_TABLE_V1, key, None)?;
        let held = builder.state.bonds.get(&lock.seat).map(|b| b.collateral).unwrap_or(0).min(lock.amount);
        if held > 0 {
            total = total.saturating_add(builder.slash_bond(lock.seat, held as u128)?);
        }
    }
    Ok(total)
}

/// lane GC-DUTY: the claim of `row_key` left its liability unconvicted — its attestors' locks are released.
pub(super) fn release_attestor_locks_of_row_v1(builder: &mut TransitionBuilder<'_>, row_key: &[u8]) -> Result<(), PalwStateV2Error> {
    for (key, _) in attestor_locks_of_row_v1(builder, row_key) {
        builder.write_kernel_row(crate::palw_g14_lane_closure_v1::PALW_G14_LANE_CLOSURE_TABLE_V1, key, None)?;
    }
    Ok(())
}

/// **lane GC-DUTY: a convicted `Final` chain claim's positive attestors are charged** (from `reverse_convicted_final`).
pub(super) fn charge_chain_claim_attestors_v1(
    builder: &mut TransitionBuilder<'_>,
    claim_id: &Hash64,
) -> Result<u64, PalwStateV2Error> {
    charge_attestor_locks_of_row_v1(builder, &certification_claim_key_v1(&palw_certification_id_v1(claim_id)))
}

fn native_attestor_locks_v39(
    builder: &TransitionBuilder<'_>,
    claim: &Hash64,
) -> Vec<(Vec<u8>, crate::palw_g14_lane_closure_v1::PalwKernelSignerLockRowV1)> {
    use crate::palw_g14_lane_closure_v1::*;
    let Some(route) = builder.state.kernel_route.as_ref() else { return Vec::new() };
    let Some(row) =
        row_of::<PalwClaimCertificationRowV1>(&builder.state, &certification_native_key_v39(&palw_native_certification_id_v39(claim)))
    else {
        return Vec::new();
    };
    row.attestations
        .iter()
        .filter_map(|a| {
            let key = palw_native_attestor_lock_key_v1(&a.verifier, &claim.as_bytes());
            let lock: PalwKernelSignerLockRowV1 =
                borsh::from_slice(route.aux.get(&(PALW_G14_LANE_CLOSURE_TABLE_V1, key.clone()))?).ok()?;
            Some((key, lock))
        })
        .collect()
}
/// **G-N2: a conviction charges every Reproduced attestor's lock** (what each bond still holds of it; burned). Returns the total.
pub(super) fn charge_native_attestor_locks_v39(builder: &mut TransitionBuilder<'_>, claim: &Hash64) -> Result<u64, PalwStateV2Error> {
    let mut total = 0u64;
    for (key, lock) in native_attestor_locks_v39(builder, claim) {
        builder.write_kernel_row(crate::palw_g14_lane_closure_v1::PALW_G14_LANE_CLOSURE_TABLE_V1, key, None)?;
        let held = builder.state.bonds.get(&lock.seat).map(|b| b.collateral).unwrap_or(0).min(lock.amount);
        if held > 0 {
            total = total.saturating_add(builder.slash_bond(lock.seat, held as u128)?);
        }
    }
    Ok(total)
}
/// G-N2: the claim left its liability uncharged — its attestors' locks are released.
pub(super) fn release_native_attestor_locks_v39(builder: &mut TransitionBuilder<'_>, claim: &Hash64) -> Result<(), PalwStateV2Error> {
    for (key, _) in native_attestor_locks_v39(builder, claim) {
        builder.write_kernel_row(crate::palw_g14_lane_closure_v1::PALW_G14_LANE_CLOSURE_TABLE_V1, key, None)?;
    }
    Ok(())
}
/// G-N2: a convicted mining claim's certification is withdrawn — decided or not, it never certifies again.
pub(super) fn withdraw_native_mining_v39(
    builder: &mut TransitionBuilder<'_>,
    claim: &Hash64,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    let key = certification_native_key_v39(&palw_native_certification_id_v39(claim));
    if let Some(mut row) = row_of::<PalwClaimCertificationRowV1>(&builder.state, &key) {
        row.outcome = Some((PalwCertificationOutcomeV1::Withdrawn, now));
        write_row(builder, key, Some(&row))?;
    }
    Ok(())
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
    // **RFC-0023 stage 1** (audit F05: the floor update never re-checked the oldest open claim): past the fence the floor rises only
    // through the resolved prefix; a retirement above it is deferred (the highest one kept) and applied when the prefix reaches it.
    if let Some(last) = builder.state.last_point().map(|p| p.daa_score)
        && builder.params.unresolved_bound_active_at(last)
    {
        let through = crate::palw_unresolved_bound_v1::palw_resolved_through_v1(&builder.state, builder.params, last);
        if raised.blue_score > through {
            let key = certification_deferred_floor_key_v1();
            let pending = row_of::<PalwCertifiedFrontierFloorV1>(&builder.state, &key);
            if pending.is_none_or(|p| p.blue_score < raised.blue_score) {
                write_row(builder, key, Some(&raised))?;
            }
            return Ok(());
        }
    }
    write_row(builder, certification_frontier_floor_key_v1(), Some(&raised))?;
    Ok(())
}

/// RFC-0023 stage 1: the key of the deferred floor (`'f'`), the highest retirement not yet inside the resolved prefix.
pub fn certification_deferred_floor_key_v1() -> Vec<u8> {
    vec![b'f']
}

/// RFC-0023 stage 1: apply the deferred floor once the resolved prefix reaches it.
pub(super) fn apply_deferred_frontier_floor_v1(
    builder: &mut TransitionBuilder<'_>,
    resolved_through: u64,
) -> Result<(), PalwStateV2Error> {
    let key = certification_deferred_floor_key_v1();
    let Some(pending) = row_of::<PalwCertifiedFrontierFloorV1>(&builder.state, &key) else { return Ok(()) };
    if pending.blue_score > resolved_through {
        return Ok(());
    }
    if palw_certified_frontier_floor_v1(&builder.state).is_none_or(|f| f.blue_score < pending.blue_score) {
        write_row(builder, certification_frontier_floor_key_v1(), Some(&pending))?;
    }
    write_row::<PalwCertifiedFrontierFloorV1>(builder, key, None)?;
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
    // **G-15 (`palw_reward_liability_horizon_v1`)**: past the fence the held reward is NOT named at certification — it waits in the row
    // until the claim's liability horizon (`final + window_court`) has passed and is named by the sweep only if the claim is still
    // certified then (the deferred release in the sweep, which alone names a governed reward and zeroes it, so it is named once even
    // when certification lands past the horizon). The weight credit above is unchanged. Below the fence: named now.
    if crate::palw_g14_gates_v1::palw_certified_reward_held_until_v1(builder.params, claim).is_some()
        || builder.params.claim_local_settlement_active_at(claim.accepted_daa)
    {
        return Ok(());
    }
    name_certified_reward_v1(builder, now, claim_id, claim, row)
}

/// **G-15: a deferred certified reward at its claim's retirement** — the claim's liability window closes at the EARLIER of
/// `final + window_court` (the sweep's release) and its retirement (a retired claim can no longer be convicted). Called by the deadline
/// sweep just before a terminal claim retires: a `Final`, governed, still-certified claim whose held reward has not been named is named
/// here, once (the row's `held_reward` is zeroed). A no-op below the fence, for a voided claim and for a reward already named.
pub(super) fn release_deferred_certified_reward_at_retirement_v1(
    builder: &mut TransitionBuilder<'_>,
    now: u64,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
) -> Result<(), PalwStateV2Error> {
    if crate::palw_g14_gates_v1::palw_certified_reward_held_until_v1(builder.params, claim).is_none() {
        return Ok(());
    }
    let key = certification_claim_key_v1(&palw_certification_id_v1(claim_id));
    let Some(mut row) = row_of::<PalwClaimCertificationRowV1>(&builder.state, &key) else { return Ok(()) };
    if row.held_reward == 0
        || !row.outcome.is_some_and(|(outcome, _)| outcome.certifies())
        || !crate::palw_rfc0018_v1::palw_independently_work_certified_v1(
            claim_id,
            claim,
            &crate::palw_rfc0018_v1::PalwGateCViewV1::of(builder.params, &builder.state),
        )
    {
        return Ok(());
    }
    name_certified_reward_v1(builder, now, claim_id, claim, &row)?;
    row.held_reward = 0;
    write_row(builder, key, Some(&row))
}

/// RFC23 stage2: the single fenced export of a held certified issuance leg.
/// A dependency denial burns the unissued right, never collateral or a fraud
/// label. A pending dependency preserves both the row and its amount.
pub(super) fn try_release_claim_local_reward_v1(
    builder: &mut TransitionBuilder<'_>,
    now: u64,
    id: &Hash64,
    claim: &PalwClaimStateV2,
    retiring: bool,
) -> Result<Option<u64>, PalwStateV2Error> {
    use super::palw_claim_local_rewards_v1::{self as local, RewardReadinessV1};
    if !builder.params.claim_local_settlement_active_at(claim.accepted_daa) {
        return Ok(None);
    }
    let ordinary = if retiring { None } else { local::ordinary_retirement_v1(&builder.state, builder.params, *id, claim, now)? };
    let retire_now = retiring || ordinary.is_some_and(|at| now > at);
    let Some(row) = local::retained_certification_v1(&builder.state, *id)? else {
        if local::hold_hint_v1(&builder.state, *id, claim).is_some() {
            // Preserve the unpaid claim and keep the derived deadline equal to
            // the caller's retry clock even if its certification row is absent.
            local::set_hold_v1(builder, *id, claim, now)?;
            return Ok(Some(now));
        }
        return Ok(None);
    };
    if row.held_reward == 0 || !row.outcome.is_some_and(|(o, _)| o.certifies()) {
        local::clear_hold_v1(builder, now, *id)?;
        if let Some(at) = ordinary {
            builder.disarm_deadline(*id);
            builder.arm_deadline(at, *id)?;
        }
        return Ok(None);
    }
    match local::reward_readiness_v1(&builder.state, builder.params, now, *id, retire_now)? {
        RewardReadinessV1::Hold(at) => {
            let at = at.max(now);
            if retire_now {
                local::set_hold_v1(builder, *id, claim, at)?;
            }
            return Ok(Some(at));
        }
        RewardReadinessV1::Ready => name_certified_reward_v1(builder, now, id, claim, &row)?,
        RewardReadinessV1::Denied => {}
    }
    use crate::palw_native_resource_v1::{NativeCoverageV1, NativeOwnerMutationErrorV1};
    let consumed = row
        .try_mutate_owner_classified_v1(NativeCoverageV1::zero(), |r| {
            r.held_reward = 0;
            Ok::<(), std::convert::Infallible>(())
        })
        .map_err(|e| match e {
            NativeOwnerMutationErrorV1::TypedRam(e) => PalwStateV2Error::NativeResource(e),
            NativeOwnerMutationErrorV1::Rejected(e) => match e {},
            NativeOwnerMutationErrorV1::Bound(e) => PalwStateV2Error::CarriageInconsistent(e),
        })?;
    let mut key = [0u8; 65];
    key[0] = CERTIFICATION_CLAIM_PREFIX_V1;
    key[1..].copy_from_slice(&palw_certification_id_v1(id));
    builder.write_kernel_row_encoded_v1(PALW_SECURITY_CERTIFICATION_TABLE_V1, &key, Some(&*consumed))?;
    local::clear_hold_v1(builder, now, *id)?;
    if let Some(at) = ordinary {
        builder.disarm_deadline(*id);
        builder.arm_deadline(at, *id)?;
    }
    Ok(None)
}

/// **The held issuance reward named** (the certified claim's coinbase leg): once, through the once-only settlement guard.
fn name_certified_reward_v1(
    builder: &mut TransitionBuilder<'_>,
    now: u64,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
    row: &PalwClaimCertificationRowV1,
) -> Result<(), PalwStateV2Error> {
    // RFC-0008 once-only settlement (dormant): the held issuance reward is named once across every consumer. This guards the
    // PAYMENT only; the weight credit above is Gate C's own (SecurityCertified) and is neither added nor read here.
    let releases = row.held_reward > 0 && {
        use crate::palw_settlement_once_v1::{
            PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
        };
        // RFC08-2: the issuance reward drawn at the claim's Final and held for certification — no new R draw; named at once now.
        let final_daa = match claim.phase {
            PalwClaimPhaseV2::Final { final_daa } => Some(final_daa),
            _ => None,
        };
        builder
            .settle_once_liable_v1(
                now,
                PalwSettlementObligationV1::of(PalwSettlementLegV1::CertificationRelease, claim_id.as_bytes()),
                PalwSettlementConsumerV1::SecurityCertification,
                row.held_reward,
                PalwSettlementDebitV1::Held { amount: row.held_reward },
                super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::V2Claim {
                    claim: *claim_id,
                    final_daa,
                    horizon: None,
                    draw: 0,
                    paid_now: row.held_reward,
                    exported_before: None,
                    vest_until: None,
                },
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
    // lane CERT: the kernel ledger, loaded once if an undecided kernel-route row needs its facts.
    let mut kernel_ledger: Option<crate::palw_native_resource_v1::ReservedKernelLedgerV1> = None;
    for (key, bytes) in rows {
        let native = key.first().copied() == Some(CERTIFICATION_NATIVE_MINING_PREFIX_V39);
        let kernel = key.first().copied() == Some(CERTIFICATION_KERNEL_CLAIM_PREFIX_V1);
        if !(key.first().copied() == Some(CERTIFICATION_CLAIM_PREFIX_V1) || native || kernel) || key.len() != 65 {
            continue;
        }
        let mut row: PalwClaimCertificationRowV1 = borsh::from_slice(&bytes).map_err(|e| refuse(e.to_string()))?;
        if kernel {
            sweep_kernel_claim_certification_v1(builder, now, &policy, anchor.as_ref(), &mut network, key, row, &mut kernel_ledger)?;
            continue;
        }
        let claim_id = Hash64::from_bytes(row.claim);
        let claim = if native { None } else { builder.state.claims.get(&claim_id).cloned() };
        // An owning39 mining claim stands while its native claim row and its complete Master escrow Final stand.
        let native_live = native && super::palw_native_mining_fold_v39::native_mining_claim_live_v39(&builder.state, &claim_id);
        if builder.params.claim_local_settlement_active_at(now)
            && let Some(c) = claim.as_ref()
            && builder.params.claim_local_settlement_active_at(c.accepted_daa)
            && (super::palw_claim_local_rewards_v1::hold_hint_v1(&builder.state, claim_id, c).is_some()
                || (row.held_reward > 0 && row.outcome.is_some_and(|(o, _)| o.certifies())))
        {
            // The fenced owner either consumes this amount exactly once or
            // preserves it. Never fall through to the legacy G-15 shortcut.
            try_release_claim_local_reward_v1(builder, now, &claim_id, c, false)?;
            continue;
        }
        // **G-15: a certified reward deferred to the liability horizon** is named here, once, the first sweep strictly past it — and
        // only if the claim still stands certified (a conviction in the window voids it, and the reward is never named: burned).
        if let (Some((outcome, _)), Some(c)) = (row.outcome, claim.as_ref())
            && outcome.certifies()
            && row.held_reward > 0
            && crate::palw_g14_gates_v1::palw_certified_reward_held_until_v1(builder.params, c).is_some_and(|until| now > until)
            && crate::palw_rfc0018_v1::palw_independently_work_certified_v1(
                &claim_id,
                c,
                &crate::palw_rfc0018_v1::PalwGateCViewV1::of(builder.params, &builder.state),
            )
        {
            name_certified_reward_v1(builder, now, &claim_id, c, &row)?;
            row.held_reward = 0;
            write_row(builder, key.clone(), Some(&row))?;
            continue;
        }
        if row.outcome.is_some() {
            // A decided native row is retired by the native fold's own collection (its receipt reads it), never here.
            if claim.is_none() && !native {
                // lane GC-DUTY: the claim is gone unconvicted (a conviction charged the locks already): release them.
                release_attestor_locks_of_row_v1(builder, &key)?;
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
                // G14 lane closure G-N2: an open post-Final accusation of the mining claim is pending; a convicted claim is not live.
                PalwCertificationClaimFactsV1 {
                    still_final: native_live
                        && !crate::palw_native_receipt_v39::native_mining_convicted_v39(&builder.state, &claim_id),
                    accusation_pending: crate::palw_native_receipt_v39::native_mining_accusation_pending_v39(
                        &builder.state,
                        &claim_id,
                    ),
                }
            } else {
                PalwCertificationClaimFactsV1 {
                    // RFC-0023 stage 1: a governed claim whose `Final` came only after its unresolved bound is withdrawn (no credit).
                    still_final: claim.as_ref().is_some_and(|c| {
                        matches!(c.phase, PalwClaimPhaseV2::Final { .. })
                            && !crate::palw_unresolved_bound_v1::palw_claim_lapsed_bound_v1(builder.params, c, now)
                    }),
                    // G14 R6: an ORIGINAL TIR frame's conviction door is a pending prosecution for Gate C until it closes — the
                    // decision waits (never a deadline failure), so no SecurityCertified credit precedes the last possible verdict.
                    accusation_pending: builder.state.palw_accusation_pending_v1(&claim_id)
                        || builder.state.palw_tir_public_prosecution_open_v1(&claim_id, now),
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
            if builder.params.claim_local_settlement_active_at(now)
                && row.held_reward > 0
                && row.outcome.is_some_and(|(o, _)| o.certifies())
                && let Some(c) = claim.as_ref()
                && builder.params.claim_local_settlement_active_at(c.accepted_daa)
            {
                try_release_claim_local_reward_v1(builder, now, &claim_id, c, false)?;
            }
        }
    }
    if network != network_before {
        write_row(builder, certification_network_key_v1(), Some(&network))?;
    }
    Ok(())
}

/// **lane CERT: one kernel-route claim's certification in the sweep**: the same anchor rule, epoch capacity, global-pool draw and
/// verdict as a chain claim's (`palw_certification_verdict_v1`). Its facts come from the kernel ledger: `still_final` while the ledger
/// claim is `Final` and unconvicted (a conviction after `Final` withdraws it), `accusation_pending` while any demand on it is open.
/// At the decision the held Final reward settles once (`settle_kernel_reward_hold_v1`). A decided row stays (the claim's public
/// certification status); nothing credits consensus weight here: a kernel claim carries none (`B = F = 0`).
#[allow(clippy::too_many_arguments)]
fn sweep_kernel_claim_certification_v1(
    builder: &mut TransitionBuilder<'_>,
    now: u64,
    policy: &PalwSecurityCertificationPolicyV1,
    anchor: Option<&crate::palw_execution_lane_v1::PalwExecSeedAnchorV1>,
    network: &mut PalwCertificationNetworkRowV1,
    key: Vec<u8>,
    mut row: PalwClaimCertificationRowV1,
    kernel_ledger: &mut Option<crate::palw_native_resource_v1::ReservedKernelLedgerV1>,
) -> Result<(), PalwStateV2Error> {
    // lane GC-DUTY: the positive attestors' locks end exactly when the kernel claim's own liability does (G-K1's fate rule): a
    // conviction charges them; a timeout, a gone row or a passed horizon releases them. The ledger is read only while a lock exists.
    if !attestor_locks_of_row_v1(builder, &key).is_empty() {
        if kernel_ledger.is_none() {
            let route = builder.state.kernel_route.as_ref().ok_or_else(|| refuse("kernel certification: no kernel route"))?;
            *kernel_ledger = Some(
                route
                    .ledger_checked_v1()
                    .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refuse))?,
            );
        }
        let ledger = kernel_ledger.as_ref().expect("loaded above");
        use crate::palw_g14_lane_closure_v1::{PalwKernelSignerLockFateV1 as F, palw_kernel_signer_lock_fate_v1};
        match palw_kernel_signer_lock_fate_v1(ledger.claims.get(&row.claim), now) {
            F::Charge => {
                charge_attestor_locks_of_row_v1(builder, &key)?;
            }
            F::Release => release_attestor_locks_of_row_v1(builder, &key)?,
            F::Hold => {}
        }
    }
    if row.outcome.is_some() {
        return Ok(());
    }
    let mut changed = false;
    if row.anchor.is_none()
        && !palw_certification_public_check_v1(&row.class_id)
        && network.draws < policy.draws_per_epoch
        && let Some(a) = anchor
        && a.span >= row.committed_span.saturating_add(policy.anchor_delay_spans.max(1))
    {
        let id = palw_kernel_claim_certification_id_v1(&row.claim);
        let execution_key = a.execution_key.as_bytes();
        let seed = palw_certification_seed_v1(&id, a.span, &execution_key);
        row.anchor = Some((a.span, execution_key));
        row.drawn_daa = now;
        row.deadline_daa = now.saturating_add(policy.attest_window_daa);
        network.draws = network.draws.saturating_add(1);
        match palw_certification_draw_v1(&seed, &id, &pool_v1(&builder.state, policy, &row), policy.m) {
            Some(draws) => row.draws = draws,
            None => row.outcome = Some((PalwCertificationOutcomeV1::NoPopulation, now)),
        }
        changed = true;
    }
    if row.outcome.is_none() {
        if kernel_ledger.is_none() {
            let route = builder.state.kernel_route.as_ref().ok_or_else(|| refuse("kernel certification: no kernel route"))?;
            *kernel_ledger = Some(
                route
                    .ledger_checked_v1()
                    .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refuse))?,
            );
        }
        let ledger = kernel_ledger.as_ref().expect("loaded above");
        let claim_row = ledger.claims.get(&row.claim);
        let facts = PalwCertificationClaimFactsV1 {
            still_final: claim_row
                .is_some_and(|c| !c.convicted && matches!(c.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. })),
            accusation_pending: ledger.demands.keys().any(|(claim, _, _)| *claim == row.claim),
        };
        if let Some(outcome) = palw_certification_verdict_v1(policy, &row, facts, now) {
            row.outcome = Some((outcome, now));
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }
    write_row(builder, key, Some(&row))?;
    if let Some((outcome, _)) = row.outcome {
        let class_key = certification_class_key_v1(&row.class_id);
        let mut counters = row_of::<PalwCertificationClassRowV1>(&builder.state, &class_key).unwrap_or_default();
        if outcome.certifies() {
            counters.certified = counters.certified.saturating_add(1);
        } else {
            counters.failed = counters.failed.saturating_add(1);
        }
        write_row(builder, class_key, Some(&counters))?;
        settle_kernel_reward_hold_v1(builder, now, &row.claim, outcome.certifies())?;
    }
    Ok(())
}

#[cfg(test)]
mod gc_duty_tests {
    use super::*;
    use std::collections::BTreeSet;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(n), index: 0 })
    }
    fn state() -> PalwChainStateV2 {
        let mut state = PalwChainStateV2::genesis();
        for n in 1..=4u64 {
            state.bonds.insert(
                bond(n),
                PalwBondStateV2 {
                    pubkey: vec![n as u8],
                    operator_id: h(n as u8),
                    collateral: 10_000,
                    slashed: 0,
                    status: PalwBondStatusV2::Active,
                    registered_daa: 0,
                    payout_payload: h(n as u8 + 100),
                    capable_classes: BTreeSet::new(),
                },
            );
        }
        state
    }
    fn extras() -> PalwTransitionExtrasV1 {
        PalwTransitionExtrasV1 {
            kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: h(1),
                ruleset_digest: h(2),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// An `'S'` row with Reproduced attestors 3 and 4 (each locked 600) and a negative attestor 2 (no lock).
    fn world(tb: &mut TransitionBuilder<'_>, claim: &Hash64) -> Vec<u8> {
        use crate::palw_g14_lane_closure_v1::*;
        let at = PalwBlockContextV2 { block: h(200), daa_score: 1, blue_score: 1, subsidy: 0 };
        super::super::palw_kernel_route_fold_v1::ensure_route_header(tb, &at).unwrap();
        let attest = |n: u64, verdict| PalwEligibilityAttestationRowV1 {
            verifier: palw_kernel_bond_id_v1(&bond(n)),
            verdict,
            transcript: if matches!(verdict, PalwEligibilityVerdictV1::Reproduced) { [n as u8; 64] } else { [0; 64] },
            attested_daa: 15,
        };
        let row = PalwClaimCertificationRowV1 {
            claim: claim.as_bytes(),
            class_id: [8; 64],
            producer_bond: palw_kernel_bond_id_v1(&bond(1)),
            producer_principal: h(1).as_bytes(),
            attempt: true,
            final_daa: 5,
            committed_daa: 5,
            committed_span: 0,
            anchor: Some((1, [0; 64])),
            draws: [2u64, 3, 4].iter().map(|n| palw_kernel_bond_id_v1(&bond(*n))).collect(),
            drawn_daa: 12,
            deadline_daa: 30,
            attestations: vec![
                attest(2, PalwEligibilityVerdictV1::Unavailable),
                attest(3, PalwEligibilityVerdictV1::Reproduced),
                attest(4, PalwEligibilityVerdictV1::Reproduced),
            ],
            held_reward: 0,
            payout_payload: [0; 64],
            outcome: Some((PalwCertificationOutcomeV1::Certified, 18)),
        };
        let key = certification_claim_key_v1(&palw_certification_id_v1(claim));
        write_row(tb, key.clone(), Some(&row)).unwrap();
        for n in [3u64, 4] {
            let lock = PalwKernelSignerLockRowV1 { version: 1, claim: claim.as_bytes(), seat: bond(n), amount: 600, locked_daa: 15 };
            tb.write_kernel_row(
                PALW_G14_LANE_CLOSURE_TABLE_V1,
                palw_native_attestor_lock_key_v1(&palw_kernel_bond_id_v1(&bond(n)), &claim.as_bytes()),
                Some(borsh::to_vec(&lock).unwrap()),
            )
            .unwrap();
        }
        key
    }

    #[test]
    fn gc_duty_a_conviction_slashes_only_the_positive_attestors_and_a_retirement_releases() {
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0).unwrap();
        let (state, extras) = (state(), extras());
        let claim = h(77);

        let mut tb = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        world(&mut tb, &claim);
        assert_eq!(tb.state.kernel_route.as_ref().unwrap().signer_locks_of_v1(&bond(3)), 600, "in the committed ledger");
        assert_eq!(charge_chain_claim_attestors_v1(&mut tb, &claim).unwrap(), 1_200);
        assert_eq!(
            [2u64, 3, 4].map(|n| tb.state.bonds[&bond(n)].collateral),
            [10_000, 9_400, 9_400],
            "the withholder/negative is untouched"
        );
        assert_eq!(tb.state.kernel_route.as_ref().unwrap().signer_locks_of_v1(&bond(3)), 0);
        assert_eq!(charge_chain_claim_attestors_v1(&mut tb, &claim).unwrap(), 0, "once");

        let mut tb = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let key = world(&mut tb, &claim);
        release_attestor_locks_of_row_v1(&mut tb, &key).unwrap();
        assert_eq!([3u64, 4].map(|n| tb.state.bonds[&bond(n)].collateral), [10_000, 10_000]);
        assert_eq!(tb.state.kernel_route.as_ref().unwrap().signer_locks_of_v1(&bond(4)), 0, "released");
    }

    #[test]
    fn gc_duty_the_node_read_names_drawn_unattested_open_rows_only_past_gate_c() {
        use crate::palw_rfc0018_v1::PalwRfc0018MirrorV1;
        let base = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0).unwrap();
        let (state, extras) = (state(), extras());
        let claim = h(77);
        let mut tb = TransitionBuilder::new(&state, &base, false, false, false, false, &extras);
        let key = world(&mut tb, &claim);
        let mut row: PalwClaimCertificationRowV1 = row_of(&tb.state, &key).unwrap();
        row.outcome = None;
        row.attestations.retain(|a| a.verifier != palw_kernel_bond_id_v1(&bond(4)));
        write_row(&mut tb, key, Some(&row)).unwrap();
        let s = tb.state.clone();
        let armed = base.clone().with_rfc0018_v1(Some(PalwRfc0018MirrorV1 { gate_c_from_daa: Some(10), ..Default::default() }));
        assert!(palw_certification_duties_v1(&s, &base, &bond(4), 20).is_empty(), "no Gate C, no duty");
        assert!(palw_certification_duties_v1(&s, &armed, &bond(4), 9).is_empty(), "below the fence");
        let owed = palw_certification_duties_v1(&s, &armed, &bond(4), 20);
        assert_eq!(owed.len(), 1);
        assert_eq!((owed[0].claim_id, owed[0].positions, owed[0].native, owed[0].deadline_daa), (claim, 1, false, 30));
        assert!(palw_certification_duties_v1(&s, &armed, &bond(3), 20).is_empty(), "already attested");
        assert!(palw_certification_duties_v1(&s, &armed, &bond(1), 20).is_empty(), "not drawn");
        assert!(palw_certification_duties_v1(&s, &armed, &bond(4), 30).is_empty(), "the deadline passed");
    }
}
