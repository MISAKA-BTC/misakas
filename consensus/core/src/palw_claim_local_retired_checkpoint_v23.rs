//! Birth-qualified retired source material and current canonical EXEC verdicts.
//! Retired sources never become new live WorkRef vertices or new credit anchors.
use super::*;
use super::palw_claim_local_legacy_terminal_v18::{RetiredExecInputViewV18, RetiredExecViewV18};
use super::palw_claim_local_rewards_v1::RewardReadinessV1 as Readiness;
use crate::palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1, NativeWireScanV1, reserve_native_requests_v1};
use crate::palw_claim_challenge_v2::{PalwPaidClaimFinalV2, PalwPaidClaimLockV2, PAID_CLAIM_FINALS_V2, PAID_CLAIM_LOCKS_V2};
use borsh::BorshDeserialize;

fn malformed(why: &str) -> PalwStateV2Error { PalwStateV2Error::CarriageInconsistent(why.into()) }
fn held() -> Readiness { Readiness::Hold(u64::MAX) }
fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, PalwStateV2Error> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw))
        .map_err(|error| match error {
            NativeDecodeErrorV1::Resource(error) => PalwStateV2Error::NativeResource(error),
            NativeDecodeErrorV1::Decode(_) => malformed("retired EXEC canonical native row codec"),
        })
}
fn fixed_aux<T: BorshDeserialize + NativeWireScanV1, const N: usize>(
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1, table: u8, key: &[u8; N],
) -> Result<Option<NativeRetainedV1<T>>, PalwStateV2Error> {
    crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(route, table, key)
        .map_err(PalwStateV2Error::NativeResource)?.map(decode).transpose()
}

/// Read BOTH the actual captured enrollment and its canonical current selection.
/// Captured Verified data alone cannot outlive a later real kernel conviction.
fn current_input_readiness_v23(
    state: &PalwChainStateV2, input: RetiredExecInputViewV18<'_>, settled: u64, now: u64,
) -> Result<Readiness, PalwStateV2Error> {
    let Some(captured) = input.native_v18() else { return Ok(held()) };
    let Some(route) = state.kernel_route.as_ref() else { return Ok(held()) };
    let claim = input.claim_v18();
    // The temporary lookup Vec is an actual allocation, admitted before creation.
    let _key = reserve_native_requests_v1(64).map_err(PalwStateV2Error::NativeResource)?;
    let Some(raw) = route.rows.get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, claim.as_byte_slice().to_vec())) else { return Ok(held()) };
    let current = decode::<misaka_palw_kernel::ledger::ClaimRowV1>(raw)?;
    if super::palw_claim_local_kernel_source_owner_v9::identity(captured)?
        != super::palw_claim_local_kernel_source_owner_v9::identity(&current)? {
        // A different immutable source is not evidence against the old source.
        return Ok(held());
    }
    use crate::palw_exec_v2_verify::PalwSliceVerificationV1 as Outcome;
    match crate::palw_exec_v2_verify::palw_exec_v2_claim_outcome_v1(&current, now) {
        Outcome::ProvenFalse | Outcome::Defaulted => return Ok(Readiness::Denied),
        Outcome::Verified { final_daa } if final_daa <= settled => {},
        _ => return Ok(held()),
    }
    let (Some(old_until), Some(until)) = (captured.liability_until, current.liability_until) else { return Ok(held()) };
    if now <= old_until.max(until) { return Ok(held()) }
    let Some(old_authority) = input.authority_v18() else { return Ok(held()) };
    let authority = route.paid_claim_authority_for_base_checked_v1(&claim.as_bytes())
        .map_err(|error| match error {
            NativeDecodeErrorV1::Resource(error) => PalwStateV2Error::NativeResource(error),
            NativeDecodeErrorV1::Decode(_) => malformed("retired EXEC current selected authority codec"),
        })?;
    let Some(authority) = authority else { return Ok(held()) };
    if *authority != *old_authority { return Ok(held()) }
    let enrollment = authority.enrollment.id();
    let final_row = fixed_aux::<PalwPaidClaimFinalV2, 64>(route, PAID_CLAIM_FINALS_V2, &enrollment)?;
    let Some(final_row) = final_row else { return Ok(held()) };
    let Some(old_final) = input.final_v18() else { return Ok(held()) };
    if old_final.enrollment != enrollment || final_row.enrollment != enrollment
        || old_final.base_claim != claim.as_bytes() || final_row.base_claim != claim.as_bytes()
        || old_final.verification_class != authority.enrollment.class || final_row.verification_class != authority.enrollment.class
        || old_final.signed_lifetime != authority.enrollment.lifetime || final_row.signed_lifetime != authority.enrollment.lifetime
        || old_final.sample_seed != final_row.sample_seed || old_final.source_context != final_row.source_context
        || old_final.logical_final_daa != final_row.logical_final_daa || final_row.logical_final_daa > settled
        || old_final.complete_receipts == 0 || old_final.completed_rounds == 0
        || final_row.complete_receipts == 0 || final_row.completed_rounds == 0 {
        return Ok(held());
    }
    let lock = fixed_aux::<PalwPaidClaimLockV2, 64>(route, PAID_CLAIM_LOCKS_V2, &enrollment)?;
    let bootstrap = misaka_palw_kernel::claim_paid_v2::paid_claim_bootstrap_anchor_v2();
    if input.lock_v18().is_some_and(|lock| lock.anchor == bootstrap)
        || lock.as_deref().is_some_and(|lock| lock.anchor == bootstrap) {
        return Ok(held());
    }
    // A removed/replaced selection cannot transfer an old enrollment's standing.
    if input.lock_v18() != lock.as_deref() { return Ok(held()) }
    Ok(Readiness::Ready)
}

/// No synthetic route/state is reconstructed. The owner supplies its admitted
/// immutable capture; mutable input verdicts come from the same current state.
pub(super) fn retired_exec_readiness_v23(
    state: &PalwChainStateV2, params: &PalwStateParamsV2, id: Hash64,
    terminal: &PalwClaimStateV2, exec: &RetiredExecViewV18, now: u64,
) -> Result<Readiness, PalwStateV2Error> {
    use crate::palw_work_slice_v2::{PalwWorkRootPhaseV2 as Phase, PalwWorkPrefixStageV2 as Prefix, PalwWorkSliceStageV2 as Slice};
    let Some(root) = exec.root_v18() else {
        return Ok(if exec.reward_v18().is_none() && exec.policy_v18().is_none()
            && exec.slices_v18().is_empty() && exec.inputs_len_v18() == 0 { Readiness::Ready } else { held() });
    };
    let settled = match root.phase { Phase::Settled { settled_daa } => settled_daa, _ => return Ok(held()) };
    if !matches!(terminal.phase, PalwClaimPhaseV2::Final { final_daa } if final_daa == settled)
        || !matches!(root.prefix, Prefix::Verified { .. }) || root.root_bond != terminal.bond
        || root.class_id != terminal.class_id || root.anchor != terminal.accepted_block
        || root.canonical_job_id != terminal.job_identity || root.pending != 0
        || root.next_index != root.slice_count() || root.accepted_work != root.slice_work_total()
        || root.verified_work != root.accepted_work || exec.slices_v18().len() != root.slice_count() as usize
        || exec.inputs_len_v18() != exec.slices_v18().len().checked_add(1).ok_or(PalwStateV2Error::Overflow("retired EXEC input population"))? {
        return Ok(held());
    }
    for (ordinal, (index, slice)) in exec.slices_v18().iter().enumerate() {
        if *index != ordinal as u32 || slice.accepted_daa > settled
            || !matches!(slice.stage, Slice::Verified { verified_daa, .. } if verified_daa <= settled) {
            return Ok(held());
        }
    }
    let mut pending = false;
    for index in 0..exec.inputs_len_v18() {
        let Some(input) = exec.input_v18(index) else { return Ok(held()) };
        let expected = if index == 0 { root.prefix_claim } else { exec.slices_v18()[index - 1].1.evidence_root };
        if input.claim_v18() != expected { return Ok(held()) }
        match current_input_readiness_v23(state, input, settled, now)? {
            Readiness::Denied => return Ok(Readiness::Denied),
            Readiness::Hold(_) => pending = true,
            Readiness::Ready => {},
        }
    }
    // Retain all original source/settlement/expiry/reward floors. Current
    // mutable rows may deny or extend waiting, never shorten these obligations.
    if pending || !exec.captured_closed_v18(params, id, terminal, now)? { return Ok(held()) }
    Ok(Readiness::Ready)
}

/// Data availability of the exact actual retirement, independent of whether its
/// economic readiness is Held, Denied or Ready. This does not create an edge or
/// grant F1 suppression: the caller must also cover the other native owners and
/// bind each required historical body into the surviving consumer's manifest.
pub(super) fn removed_checkpoint_overlay_ready_v23(
    state: &PalwChainStateV2, params: &PalwStateParamsV2, id: Hash64,
    terminal: &PalwClaimStateV2, ctx: &PalwBlockContextV2,
) -> Result<bool, PalwStateV2Error> {
    super::palw_claim_local_legacy_terminal_v18::with_pending_checkpoint_source_v18(
        state, params, id, terminal, |source| {
            if source.removed_v18() != *ctx || source.terminal_v18() != terminal {
                return Ok(false);
            }
            source.visit_owned_material_v18(state, |_| Ok(()))?;
            // The decoded immutable capture has its own admitted ownership.
            // Mutable canonical verdicts are joined only by own_standing;
            // missing certification must retain data rather than destroy it.
            let _exec = source.exec_material_v18(state)?;
            Ok(true)
        },
    ).map(|source| source.unwrap_or(false))
}
