//! The fold arms of the protected candidate refutation (`crate::palw_model_refutation_seal_v4`, inner 36 / 37): consensus-owned rows,
//! the authenticated signer as the actor, the route's shared block budget, original-collateral reservations, and the kinds' frozen
//! courts called unchanged for the payout (atomic, once only).
use super::palw_kernel_route_fold_v1::{charge_route_budget_v1, ensure_route_header, route_ledger_policy_v1};
use super::*;
use crate::palw_model_refutation_seal_v4::*;
use misaka_palw_kernel::hash::Digest;

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}

impl PalwChainStateV2 {
    /// The original collateral `bond`'s live refutation seals reserve at `daa` (0 with no route or no seal row).
    pub fn model_refutation_seal_reserved_v4(&self, bond: &PalwBondKeyV2, daa: u64) -> u128 {
        self.kernel_route.as_ref().map(|r| r.model_refutation_seal_reserved_v4(bond, daa)).unwrap_or(0)
    }
}

/// Is `palw_model_refutation_seal_v1` in force at this block (the processor passes its DAA only where it is)?
pub(super) fn sealed_refutations_in_force_v4(builder: &TransitionBuilder<'_>) -> bool {
    builder.extras.kernel_route.as_ref().is_some_and(|r| r.model_refutation_seal.is_some())
}

fn enabled(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    if !builder.extras.kernel_route.as_ref().is_some_and(|r| r.opv.is_some()) {
        return Err(refused("sealed candidate refutations require palw_panel_free_v1"));
    }
    if !sealed_refutations_in_force_v4(builder) {
        return Err(refused("sealed candidate refutations require palw_model_refutation_seal_v1"));
    }
    Ok(())
}

/// **Inner 36: post (or, identically, re-post) a hidden refutation seal in one of the signer's slots.**
pub(super) fn seal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    slot: u8,
    seal: &Digest,
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    if slot >= MODEL_REFUTATION_SEAL_SLOTS_V4 {
        return Err(refused("no such refutation seal slot"));
    }
    let daa = ctx.daa_score;
    let policy = route_ledger_policy_v1(builder)?;
    if let Some(row) = builder.state.kernel_route.as_ref().unwrap().model_refutation_seal_v4(signer, slot) {
        if row.live_at(daa) {
            // G14-R's rule for the claim/proof seals: an IDENTICAL re-seal is a no-op — the earlier row and its clock stand (a
            // restarted finder re-sends the same bytes; it must not restart its own maturity). Another seal waits for the slot.
            return if row.seal == *seal { Ok(()) } else { Err(refused("the slot holds a live refutation seal")) };
        }
    }
    // Original capital: the deposit comes out of collateral nothing else has committed (an expired row in this slot no longer counts).
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, daa)) < policy.seal_deposit as u128 {
        return Err(refused("the sealer's free collateral does not cover the seal deposit"));
    }
    let (Some(mature_daa), Some(expires_daa)) =
        (daa.checked_add(policy.claim_seal_delay_daa), daa.checked_add(policy.seal_ttl_daa))
    else {
        return Err(refused("refutation seal clock overflows"));
    };
    if mature_daa >= expires_daa {
        return Err(refused("a refutation seal must mature before it expires"));
    }
    // A fixed-metadata object still spends the block's shared route budget (never the proof reserve).
    if !charge_route_budget_v1(builder, ctx, MODEL_REFUTATION_SEAL_WORK_V4, false)? {
        return Ok(());
    }
    let row = ModelRefutationSealRowV4 { version: 4, seal: *seal, posted_daa: daa, mature_daa, expires_daa, deposit: policy.seal_deposit };
    builder.write_kernel_row(
        MODEL_REFUTATION_SEAL_TABLE_V4,
        model_refutation_seal_key_v4(signer, slot),
        Some(borsh::to_vec(&row).expect("a seal row serializes")),
    );
    Ok(())
}

/// **Inner 37: reveal a sealed refutation.** Only the signer's own mature, unexpired seal that opens to exactly these bytes admits
/// the kind's frozen court; the row is consumed once a court ran.
#[allow(clippy::too_many_arguments)]
pub(super) fn reveal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    slot: u8,
    kind: u8,
    target: &Digest,
    salt: &Digest,
    proof: &[u8],
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    if !known_model_refutation_kind_v4(kind) {
        return Err(refused("unknown refutation target kind"));
    }
    if proof.len() > MODEL_REFUTATION_MAX_PROOF_BYTES_V4 {
        return Err(refused("refutation proof past its carrier"));
    }
    let row = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .model_refutation_seal_v4(signer, slot)
        .ok_or_else(|| refused("no refutation seal of the signer in this slot"))?;
    if ctx.daa_score < row.mature_daa {
        return Err(refused("the refutation seal is not mature"));
    }
    if !row.live_at(ctx.daa_score) {
        return Err(refused("the refutation seal expired"));
    }
    // One bounded digest of the exact carrier bytes, then a fixed-size preimage: no court input is decoded before the seal opens.
    if seal_of_digest_v4(kind, target, signer, salt, &model_refutation_proof_digest_v4(proof)) != row.seal {
        return Err(refused("the reveal does not open the signer's seal"));
    }
    let outcome = match kind {
        MODEL_REFUTATION_KIND_ARTIFACT_V2 => super::palw_model_artifact_fold_v2::refute_outcome(builder, ctx, signer, target, proof)?,
        MODEL_REFUTATION_KIND_VECTOR_V2 => super::palw_model_vector_fold_v2::refute_outcome(builder, ctx, signer, target, proof)?,
        _ => super::palw_flat_model_vector_fold_v3::refute_outcome(builder, ctx, signer, target, proof)?,
    };
    if outcome != ModelRefutationOutcomeV4::NotRun {
        builder.write_kernel_row(MODEL_REFUTATION_SEAL_TABLE_V4, model_refutation_seal_key_v4(signer, slot), None);
    }
    Ok(())
}
