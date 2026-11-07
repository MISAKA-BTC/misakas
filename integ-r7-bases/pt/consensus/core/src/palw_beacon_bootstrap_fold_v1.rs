//! **The fold of N3's BootstrapExact source rounds** (inner 73 into kernel route aux 130; `crate::palw_beacon_bootstrap_v1` holds the
//! policy, the rows, the pure judgement and the restore fence). A child module of `palw_state_v2`, like the kernel route's own fold.
//! Every entry refuses below `palw_beacon_bootstrap_v1` (the processor's A-2 gate drops the object first), and a refusal writes
//! nothing. Source-only: no reward, block right, Final weight or payout is written here.

use super::*;
use crate::palw_beacon_bootstrap_v1::*;
use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
use misaka_palw_kernel::beacon_bootstrap_v1::{BootstrapExactActionV1, BootstrapExactContributionV1, bootstrap_exact_commitment_v1};

fn refuse(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}

fn encode(row: &PalwBootstrapRoundRowV1) -> Vec<u8> {
    borsh::to_vec(row).expect("a bootstrap round serializes")
}

/// **Inner 73**: one contributor's seal or reveal. The round opens at its first seal, with the target's commitment position read from
/// the kernel ledger (a target that is not a kernel claim is refused). A seal lands strictly after that position and by the round's
/// seal end, once per contributor, under the per-round cap; it records the sealing bond's operator. A reveal lands after the seal end
/// and by the reveal end and must open the contributor's own commitment. A judged round takes nothing more.
pub(super) fn apply_bootstrap_exact_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    contribution: &BootstrapExactContributionV1,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let policy = *builder
        .params
        .econ_contracts_v1()
        .beacon_bootstrap_at(now)
        .ok_or_else(|| refuse("palw_beacon_bootstrap_v1 is not in force"))?;
    policy.validate().map_err(refuse)?;
    contribution.validate().map_err(refuse)?;
    if contribution.contributor != palw_kernel_bond_id_v1(signer) {
        return Err(refuse("the contribution's contributor is not its signer"));
    }
    let record = builder.state.bonds.get(signer).ok_or(PalwStateV2Error::MissingBond(*signer))?;
    if !matches!(record.status, PalwBondStatusV2::Active) {
        return Err(refuse("a BootstrapExact contribution needs an Active bond"));
    }
    let operator = record.operator_id.as_bytes();
    let round = contribution.round;
    if round.profile != policy.profile.id.as_bytes() || round.slot >= policy.maximum_sources_per_target {
        return Err(refuse("the round names another profile than the fence's, or a slot past its sources"));
    }
    let round_id = round.id();
    let mut row = {
        let route = builder.state.kernel_route.as_ref().ok_or_else(|| refuse("the kernel route is missing"))?;
        if contribution.network != route.header.policy.network_domain {
            return Err(refuse("the contribution names another network"));
        }
        match route.aux_row::<PalwBootstrapRoundRowV1>(PALW_BEACON_BOOTSTRAP_TABLE_V1, &round_id) {
            Some(row) => row,
            None => {
                if !matches!(contribution.action, BootstrapExactActionV1::Seal { .. }) {
                    return Err(refuse("no such BootstrapExact round: a round opens at its first seal"));
                }
                let open = route
                    .aux
                    .range((PALW_BEACON_BOOTSTRAP_TABLE_V1, Vec::new())..(PALW_BEACON_BOOTSTRAP_TABLE_V1 + 1, Vec::new()))
                    .filter(|(_, bytes)| borsh::from_slice::<PalwBootstrapRoundRowV1>(bytes).is_ok_and(|r| r.judgement.is_none()))
                    .count();
                if open >= policy.maximum_open_rounds as usize {
                    return Err(refuse("the open BootstrapExact round capacity is exhausted"));
                }
                let ledger = route.ledger().map_err(refuse)?;
                let target = ledger.claims.get(&round.target).ok_or_else(|| refuse("the round's target is not a kernel claim"))?;
                PalwBootstrapRoundRowV1 {
                    round,
                    target_committed_daa: target.committed_daa,
                    seals: Vec::new(),
                    judgement: None,
                    policy: policy.id(),
                }
            }
        }
    };
    if row.judgement.is_some() || row.policy != policy.id() {
        return Err(refuse("the round is already judged, or was opened under another policy"));
    }
    let (seal_end, reveal_end) = (row.seal_end(&policy), row.reveal_end(&policy));
    match contribution.action {
        BootstrapExactActionV1::Seal { commitment } => {
            // Unpredictability by construction: every input is fixed strictly after the target's commitment.
            if now <= row.target_committed_daa || now > seal_end {
                return Err(refuse("a seal lands only after the target's commitment and inside the round's seal window"));
            }
            if row.seals.iter().any(|s| s.contributor == contribution.contributor) {
                return Err(refuse("the contributor already sealed in this round"));
            }
            if row.seals.len() >= policy.maximum_contributors_per_round as usize {
                return Err(refuse("the round's contributor capacity is exhausted"));
            }
            row.seals.push(PalwBootstrapSealRowV1 {
                contributor: contribution.contributor,
                operator,
                commitment,
                sealed_daa: now,
                reveal: None,
            });
            row.seals.sort_by(|a, b| (a.sealed_daa, a.contributor).cmp(&(b.sealed_daa, b.contributor)));
        }
        BootstrapExactActionV1::Reveal { preimage } => {
            if now <= seal_end || now > reveal_end {
                return Err(refuse("a reveal lands only after the round's seal window and inside its reveal window"));
            }
            let seal = row
                .seals
                .iter_mut()
                .find(|s| s.contributor == contribution.contributor)
                .ok_or_else(|| refuse("the contributor sealed nothing in this round"))?;
            if seal.reveal.is_some()
                || seal.commitment != bootstrap_exact_commitment_v1(&round_id, &contribution.contributor, &preimage)
            {
                return Err(refuse("the reveal repeats, or does not open the contributor's own seal"));
            }
            seal.reveal = Some((preimage, now));
        }
    }
    row.validate(&policy).map_err(refuse)?;
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    builder.write_kernel_row(PALW_BEACON_BOOTSTRAP_TABLE_V1, round_id.to_vec(), Some(encode(&row)))?;
    Ok(())
}

/// **The judging step** (the kernel route's closing step, at the block's DAA; journaled): every due round
/// ([`PalwBootstrapRoundRowV1::due`]) gets its judgement ([`palw_bootstrap_judge_round_v1`] — the fold derives every fact; an aborted
/// round is judged `WITHHELD`); a judgement older than the policy's retention retires its round. A no-op below the fence.
pub(super) fn sweep_beacon_bootstrap_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let Some(policy) = builder.params.econ_contracts_v1().beacon_bootstrap_at(now).copied() else { return Ok(()) };
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let mut writes: Vec<(Vec<u8>, Option<Vec<u8>>)> = Vec::new();
    for ((_, key), bytes) in
        route.aux.range((PALW_BEACON_BOOTSTRAP_TABLE_V1, Vec::new())..(PALW_BEACON_BOOTSTRAP_TABLE_V1 + 1, Vec::new()))
    {
        let mut row: PalwBootstrapRoundRowV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
        match row.judgement.as_ref() {
            Some(judged) if now > judged.judged_daa.saturating_add(policy.judged_retention_daa) => writes.push((key.clone(), None)),
            Some(_) => {}
            None if row.due(&policy, now) => {
                row.judgement = Some(palw_bootstrap_judge_round_v1(&row, &policy.profile, now).0);
                writes.push((key.clone(), Some(encode(&row))));
            }
            None => {}
        }
    }
    for (key, row) in writes {
        builder.write_kernel_row(PALW_BEACON_BOOTSTRAP_TABLE_V1, key, row)?;
    }
    Ok(())
}
