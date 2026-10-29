//! Iterative traversal of the single signed prefix input per FP claim. Strictly
//! decreasing accepted blue scores rule out cycles without recursive stack use.
use super::*;
use crate::palw_claim_local_v1::prefix_v1::PalwSignedPrefixUseV1;

pub(super) fn prefix_use(origin: &PalwClaimLocalAdmissionContextV1) -> Result<Option<&PalwSignedPrefixUseV1>, PalwStateV2Error> {
    let id = origin.claim_id();
    let mut inputs = origin.frame().inputs.iter().filter_map(|input| match input {
        PalwClaimLocalInputV1::PrefixDependency { claim, envelope } if *claim == id => Some(envelope),
        _ => None,
    });
    let Some(envelope) = inputs.next() else { return Ok(None) };
    if inputs.next().is_some() { return Err(bad("duplicate prefix dependency birth metadata")); }
    envelope.check_v1().map_err(|_| bad("invalid prefix birth envelope"))?;
    let Some(consumed) = &envelope.consumed else { return Ok(None) };
    // A non-genesis prefix whose producer cannot be authenticated stays held.
    // Missing birth material is not an independent input and not a fraud verdict.
    let Some(descriptor) = origin.frame().accepted_objects.iter().find_map(|object| match object {
        PalwConsensusObjectV2::FreePromptCommitted { claim, consumed_prefix_state, .. } if *claim == id => Some(consumed_prefix_state),
        _ => None,
    }) else { return Ok(None) };
    // public_job is intentionally skipped by the released object wire codec;
    // its authenticated native birth material has its own retained input tag.
    let Some(job) = origin.frame().inputs.iter().find_map(|input| match input {
        PalwClaimLocalInputV1::FreePromptNative { claim, public_job: Some(job), .. } if *claim == id => Some(job.as_ref()),
        _ => None,
    }) else { return Ok(None) };
    if consumed.intent.base_commitment != id || consumed.intent.network != job.job.network_domain
        || consumed.intent.prefix != *descriptor || crate::palw_freeprompt_v3::fp_claim_id_v3(job) != id
        || consumed.intent.verify_signature_v1(&job.job.executor_pubkey, &consumed.signature).is_err() {
        return Ok(None);
    }
    Ok(Some(consumed))
}
fn held() -> RewardReadinessV1 {
    RewardReadinessV1::Hold(u64::MAX)
}

pub(super) fn readiness(
    state: &PalwChainStateV2, params: &PalwStateParamsV2, now: u64,
    id: Hash64, born: &PalwClaimStateV2, target: Hash64, target_birth: &PalwClaimStateV2, retiring: bool,
) -> Result<RewardReadinessV1, PalwStateV2Error> {
    let Some(mut cursor) = origin_for(state, params, id, born)? else { return Ok(held()) };
    let mut result = RewardReadinessV1::Ready;
    loop {
        // No prefix edge: this claim is independent of every export conviction.
        let Some(consumed) = prefix_use(&cursor)? else { return Ok(result) };
        let identity = consumed.intent.source;
        let point = match state.claim_local_prefix_source_point_v1(&identity) {
            Ok(point) => point,
            Err(PalwStateV2Error::NativeResource(error)) => return Err(PalwStateV2Error::NativeResource(error)),
            Err(_) => return Ok(held()),
        };
        let Some(next) = state.claim_local_admission_context_v1(params, &point, identity.source_claim)? else {
            return Ok(held());
        };
        let ordinal = next.frame().inputs.iter().filter_map(|input| match input {
            PalwClaimLocalInputV1::PrefixDependency { claim, envelope } if *claim == identity.source_claim => Some(envelope),
            _ => None,
        }).flat_map(|envelope| envelope.exports.iter()).find(|e| e.intent.id_v1() == identity.intent).map(|e| e.intent.ordinal);
        let Some(ordinal) = ordinal else { return Ok(held()) };
        let Ok(Some((export, actual))) = next.prefix_export_v1(ordinal) else { return Ok(held()) };
        if consumed.intent.check_source_v1(&export.intent, &actual, consumed.intent.network, cursor.claim_id(),
            cursor.accepting_point().daa_score, cursor.accepting_point().blue_score).is_err() {
            return Ok(held());
        }
        let Some(reserve) = state.prefix_export_reservation_v1(&point, identity.source_claim)? else { return Ok(held()) };
        // The reservation row is the terminal record. Cancel this dependent once.
        // Claims that never entered this edge are not joined into the denial.
        if reserve.convicted_daa.is_some() {
            return Ok(RewardReadinessV1::Denied);
        }
        result = result.join(source(state, params, now, identity.source_claim, next.original_claim(), target, target_birth, retiring)?);
        if result == RewardReadinessV1::Denied { return Ok(result); }
        cursor = next;
    }
}
