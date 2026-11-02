//! RFC23's first monetary consumer: do not export a held certified reward
//! through a live dependency. This does not replace Gate C or grant checkpoint
//! finality. Unsupported/missing input provenance remains held, never fraud.
use super::*;
use crate::palw_claim_local_v1::*;
use crate::palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1};
use crate::palw_security_certification_v1::*;
use borsh::BorshDeserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RewardReadinessV1 {
    Ready,
    Hold(u64),
    Denied,
}
impl RewardReadinessV1 {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Denied, _) | (_, Self::Denied) => Self::Denied,
            (Self::Hold(a), Self::Hold(b)) => Self::Hold(a.max(b)),
            (Self::Hold(a), _) | (_, Self::Hold(a)) => Self::Hold(a),
            _ => Self::Ready,
        }
    }
}
fn bad(why: &str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why.into())
}
pub(super) fn decoded<T: borsh::BorshDeserialize + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeWireScanV1>(
    bytes: &[u8],
) -> Result<NativeRetainedV1<T>, PalwStateV2Error> {
    crate::palw_claim_local_v1::decode_claim_local_value_v1(bytes).map_err(|e| match e {
        NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
        NativeDecodeErrorV1::Decode(e) => bad(&format!("claim-local reward metadata: {e}")),
    })
}
pub(super) fn frame_read<T>(
    f: &PalwClaimLiabilityFrameV1,
    inspect: impl FnOnce(&PalwStateCarriageV2, &PalwStateCarriageV2) -> Result<T, PalwStateV2Error>,
) -> Result<T, PalwStateV2Error> {
    f.with_context_carriages_v1(|a, b| Ok(inspect(a, b))).map_err(|e| match e {
        PalwClaimCheckpointErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
        PalwClaimCheckpointErrorV1::Semantic(e) => bad(e),
    })?
}
pub(super) fn frames(state: &PalwChainStateV2) -> impl Iterator<Item = (&Vec<u8>, &Vec<u8>)> {
    state
        .kernel_route
        .iter()
        .flat_map(|r| r.aux.iter())
        .filter(|((table, key), _)| *table == PALW_CLAIM_LOCAL_TABLE_V1 && key.len() == 65 && key[0] == b'F')
        .map(|((_, key), bytes)| (key, bytes))
}
fn frame_checked(
    state: &PalwChainStateV2,
    key: &[u8],
    bytes: &[u8],
) -> Result<NativeRetainedV1<PalwClaimLiabilityFrameV1>, PalwStateV2Error> {
    let f = decoded::<PalwClaimLiabilityFrameV1>(bytes)?;
    if key != palw_claim_liability_frame_key_v1(f.point.block)
        || state.last_point.is_none_or(|last| f.point.daa_score > last.daa_score || f.point.blue_score > last.blue_score)
    {
        return Err(bad("claim-local reward frame is outside executed history"));
    }
    Ok(f)
}
fn hold_key(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = b'R';
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
pub(super) fn hold_hint_v1(state: &PalwChainStateV2, id: Hash64, c: &PalwClaimStateV2) -> Option<u64> {
    let key = hold_key(id);
    let bytes = &state.kernel_route.as_ref()?.aux.iter().find(|((t, k), _)| *t == PALW_CLAIM_LOCAL_TABLE_V1 && k.as_slice() == key)?.1;
    if bytes.len() != 24 {
        return None;
    }
    let read = |i| u64::from_le_bytes(bytes[i..i + 8].try_into().expect("fixed hold word"));
    (read(0) == c.accepted_daa && read(8) == c.accepted_blue_score).then(|| read(16))
}
pub(super) fn set_hold_v1(
    builder: &mut TransitionBuilder<'_>,
    id: Hash64,
    c: &PalwClaimStateV2,
    until: u64,
) -> Result<(), PalwStateV2Error> {
    let row = PalwClaimLocalRewardHoldV1 { accepted_daa: c.accepted_daa, accepted_blue_score: c.accepted_blue_score, until };
    builder.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &hold_key(id), Some(&row))
}
pub(super) fn clear_hold_v1(builder: &mut TransitionBuilder<'_>, now: u64, id: Hash64) -> Result<(), PalwStateV2Error> {
    if !builder.params.claim_local_settlement_active_at(now) {
        return Ok(());
    }
    let key = hold_key(id);
    if builder
        .state
        .kernel_route
        .as_ref()
        .is_some_and(|r| r.aux.keys().any(|(t, k)| *t == PALW_CLAIM_LOCAL_TABLE_V1 && k.as_slice() == key))
    {
        builder.write_kernel_row_encoded_v1::<PalwClaimLocalRewardHoldV1>(PALW_CLAIM_LOCAL_TABLE_V1, &key, None)?;
    }
    Ok(())
}

#[path = "palw_claim_local_prefix_rewards_v1.rs"]
mod prefix_rewards;

fn same_birth(a: &PalwClaimStateV2, b: &PalwClaimStateV2) -> bool {
    a.accepted_daa == b.accepted_daa && a.accepted_blue_score == b.accepted_blue_score && a.accepted_block == b.accepted_block
}
fn cert_key(id: Hash64) -> [u8; 65] {
    let mut key = [0; 65];
    key[0] = CERTIFICATION_CLAIM_PREFIX_V1;
    key[1..].copy_from_slice(&palw_certification_id_v1(&id));
    key
}
pub(super) fn cert_bytes(route: Option<&crate::palw_kernel_route_v1::PalwKernelRouteStateV1>, id: Hash64) -> Option<&[u8]> {
    let wanted = cert_key(id);
    route?
        .aux
        .iter()
        .find(|((table, key), _)| *table == PALW_SECURITY_CERTIFICATION_TABLE_V1 && key.as_slice() == wanted)
        .map(|(_, bytes)| bytes.as_slice())
}
pub(super) fn retained_certification_v1(
    state: &PalwChainStateV2,
    id: Hash64,
) -> Result<Option<NativeRetainedV1<PalwClaimCertificationRowV1>>, PalwStateV2Error> {
    cert_bytes(state.kernel_route.as_ref(), id).map(decoded).transpose()
}
pub(super) fn approval(
    params: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
    bytes: Option<&[u8]>,
) -> Result<RewardReadinessV1, PalwStateV2Error> {
    if !crate::palw_rfc0018_v1::palw_gate_c_governs_v1(params, c) {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    }
    let Some(bytes) = bytes else {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    };
    let row = decoded::<PalwClaimCertificationRowV1>(bytes)?;
    if row.claim != id.as_bytes() || row.class_id != c.class_id.as_bytes() {
        return Err(bad("certification changed its claim binding"));
    }
    let Some((outcome, _)) = row.outcome else {
        return Ok(RewardReadinessV1::Hold(row.deadline_daa));
    };
    if crate::palw_rfc0018_v1::palw_work_certified_from_row_v1(params, c, || outcome.certifies()) {
        Ok(RewardReadinessV1::Ready)
    } else {
        Ok(RewardReadinessV1::Denied)
    }
}

/// Fund the existing EXEC horizon reader's variable ClaimRow decodes and its
/// Borsh key buffers before calling it. A missing historical input is not guilt.
pub(super) fn prepare_exec_reader(
    state: &PalwChainStateV2,
    id: Hash64,
) -> Result<(bool, Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>), PalwStateV2Error> {
    use crate::palw_native_resource_v1::{TirNativeRequestsV1, preflight_native_value_v1, reserve_native_requests_v1};
    let Some(root) = state.exec_v2.roots.get(&id) else {
        return Ok((true, None));
    };
    if matches!(root.prefix, crate::palw_work_slice_v2::PalwWorkPrefixStageV2::Unbound) {
        return Ok((false, None));
    }
    let Some(route) = state.kernel_route.as_ref() else {
        return Ok((false, None));
    };
    let mut q = TirNativeRequestsV1::new();
    for input in
        std::iter::once(root.prefix_claim).chain(state.exec_v2.slices.range((id, 0)..=(id, u32::MAX)).map(|(_, s)| s.evidence_root))
    {
        let Some((_, bytes)) = route
            .rows
            .iter()
            .find(|((table, key), _)| *table == misaka_palw_kernel::rows::TABLE_CLAIMS_V1 && key.as_slice() == input.as_byte_slice())
        else {
            return Ok((false, None));
        };
        let shape = preflight_native_value_v1::<misaka_palw_kernel::ledger::ClaimRowV1>(bytes.as_slice(), bytes.len() as u64)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::OutOfMemory => {
                    PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<
                        misaka_palw_kernel::ledger::ClaimRowV1,
                    >("reward dependency scan"))
                }
                std::io::ErrorKind::Unsupported | std::io::ErrorKind::TimedOut => {
                    PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                        misaka_palw_kernel::ledger::ClaimRowV1,
                    >("reward dependency scan", &e.to_string()))
                }
                _ => bad("malformed EXEC reward dependency"),
            })?;
        q.add_v1(shape.logical_requests_v1()).map_err(PalwStateV2Error::NativeResource)?;
        q.vec_v1::<u8>(1024).map_err(PalwStateV2Error::NativeResource)?;
    }
    Ok((true, reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?))
}
fn exec_window(state: &PalwChainStateV2, params: &PalwStateParamsV2, id: Hash64) -> Result<RewardReadinessV1, PalwStateV2Error> {
    let (known, _lease) = prepare_exec_reader(state, id)?;
    if !known {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    }
    Ok(state.exec_v2_liability_until_v1(params, &id)?.map_or(RewardReadinessV1::Ready, RewardReadinessV1::Hold))
}

/// A chain claim that consumed EXEC output inherits each native evidence claim's
/// liability. A finite EXEC horizon alone does not establish that the prefix and
/// every slice were selected by the native receipt tally, remained Final, and
/// survived the full challenge window.
pub(super) fn exec_dependency_readiness_v1(
    state: &PalwChainStateV2,
    id: Hash64,
    now: u64,
) -> Result<RewardReadinessV1, PalwStateV2Error> {
    use crate::palw_exec_v2_verify::PalwSliceVerificationV1 as V;
    use crate::palw_work_slice_v2::PalwWorkPrefixStageV2;
    let Some(root) = state.exec_v2.roots.get(&id) else {
        return Ok(if state.exec_v2_rewards.contains_key(&id) { RewardReadinessV1::Hold(u64::MAX) } else { RewardReadinessV1::Ready });
    };
    if matches!(root.prefix, PalwWorkPrefixStageV2::Unbound) {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    }
    let Some(route) = state.kernel_route.as_ref() else {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    };
    let (known, lease) = prepare_exec_reader(state, id)?;
    if !known {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    }
    let _lease = lease;
    let inputs = std::iter::once(root.prefix_claim)
        .chain(state.exec_v2.slices.range((id, 0)..=(id, u32::MAX)).map(|(_, slice)| slice.evidence_root));
    let mut result = RewardReadinessV1::Ready;
    for input in inputs {
        let Some(bytes) = route.rows.get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, input.as_byte_slice().to_vec())) else {
            return Ok(RewardReadinessV1::Hold(u64::MAX));
        };
        let row: misaka_palw_kernel::ledger::ClaimRowV1 =
            borsh::from_slice(bytes).map_err(|_| bad("malformed EXEC dependency row"))?;
        let decision = match crate::palw_exec_v2_verify::palw_exec_v2_claim_outcome_v1(&row, now) {
            V::ProvenFalse | V::Defaulted => RewardReadinessV1::Denied,
            V::Pending => RewardReadinessV1::Hold(row.liability_until.unwrap_or(u64::MAX)),
            V::Verified { .. } if !crate::palw_exec_v2_verify::palw_exec_v2_selected_approval_v1(route, &input) => {
                RewardReadinessV1::Hold(row.liability_until.unwrap_or(u64::MAX))
            }
            V::Verified { .. } => match row.liability_until {
                Some(until) if now > until => RewardReadinessV1::Ready,
                Some(until) => RewardReadinessV1::Hold(until),
                None => RewardReadinessV1::Hold(u64::MAX),
            },
        };
        result = result.join(decision);
        if result == RewardReadinessV1::Denied {
            return Ok(result);
        }
    }
    Ok(result)
}

/// The real unhinted retirement, including existing native/material clocks.
/// A scheduling hint must never waive one of those obligations.
pub(super) fn ordinary_retirement_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
    now: u64,
) -> Result<Option<u64>, PalwStateV2Error> {
    if params.claim_retirement_daa() == 0 || terminal_operational_release_at_v1(c, params, Some(now))?.is_some() {
        return Ok(None);
    }
    let (known, _lease) = prepare_exec_reader(state, id)?;
    if !known {
        return Ok(None);
    }
    Ok(super::palw_exec_v2_rewards_v1::retirement(state, params, &id, terminal_deadline_at_v1(c, params, Some(now))?, c)?
        .map(|at| state.staged_retirement_floor_v1(&id, at)))
}

pub(super) fn live_source(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    id: Hash64,
    c: &PalwClaimStateV2,
    retiring_self: bool,
) -> Result<RewardReadinessV1, PalwStateV2Error> {
    if matches!(c.phase, PalwClaimPhaseV2::Voided { .. })
        || crate::palw_unresolved_bound_v1::palw_claim_lapsed_bound_v1(params, c, now)
    {
        return Ok(RewardReadinessV1::Denied);
    }
    let PalwClaimPhaseV2::Final { final_daa } = c.phase else {
        return Ok(RewardReadinessV1::Hold(c.trace_retention_daa.max(now)));
    };
    let mut decision = approval(params, id, c, cert_bytes(state.kernel_route.as_ref(), id))?;
    if let Some(origin) = super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(state,params,id,c)? {
        if let Some(reservation) = state.prefix_export_reservation_v1(&origin.accepting_point(), id)? {
            if reservation.convicted_daa.is_some() { return Ok(RewardReadinessV1::Denied); }
            if now <= reservation.material_until { decision = decision.join(RewardReadinessV1::Hold(reservation.material_until)); }
        }
    }
    if !retiring_self {
        let until = c.trace_retention_daa.max(final_daa.saturating_add(params.window_court()));
        if now <= until {
            decision = decision.join(RewardReadinessV1::Hold(until));
        }
        if let RewardReadinessV1::Hold(until) = exec_window(state, params, id)?
            && now <= until
        {
            decision = decision.join(RewardReadinessV1::Hold(until));
        }
    }
    // A native dependency can default or be convicted even on the block that
    // closes this chain claim. Retirement is not a substitute for resolving it.
    decision = decision.join(exec_dependency_readiness_v1(state, id, now)?);
    if decision == RewardReadinessV1::Denied {
        return Ok(decision);
    }
    if state.palw_accusation_pending_v1(&id) || state.palw_tir_public_prosecution_open_v1(&id, now) {
        decision = decision.join(RewardReadinessV1::Hold(now));
    }
    Ok(decision)
}
/// Exact original-cohort export hold, also required by Closed(A5) shortcuts.
/// The claim's accepted_block may name RED provenance rather than its actual
/// accepting point. Obtain the point from SourceOriginV8 (scoped A8 or legacy F1) or strict A5 birth authority,
/// never derive a negative E lookup from that carried provenance block.
pub(super) fn exact_export_readiness_v1(
    state: &PalwChainStateV2, params: &PalwStateParamsV2, now: u64,
    id: Hash64, born: &PalwClaimStateV2,
) -> Result<RewardReadinessV1, PalwStateV2Error> {
    // Before this birth's activation E admission was impossible. Preserve
    // legacy S1/no-F1 compatibility without imposing a new archival obligation.
    if !params.claim_local_settlement_active_at(born.accepted_daa) {
        return Ok(RewardReadinessV1::Ready);
    }
    let point = if let Some(origin) = super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(state, params, id, born)? {
        if !same_birth(origin.original_claim(), born) {
            return Err(bad("export reward hold changes its actual original cohort"));
        }
        origin.accepting_point()
    } else if let Some(closed) = super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(state, id, born)? {
        // Strict original-birth A5 verifies both directories and native body
        // authority; legacy S1 alone cannot assert absence of an old export E.
        if !same_birth(&closed.original, born) {
            return Err(bad("closed export reward hold changes its original cohort"));
        }
        closed.birth
    } else {
        // Without scoped A8, retained F1 or strict A5 authority the E scope is unknown. Guessing a
        // key from born.accepted_block could turn a RED dependency into credit.
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    };
    let Some(reservation) = state.prefix_export_reservation_v1(&point, id)? else {
        return Ok(RewardReadinessV1::Ready);
    };
    if reservation.bond != born.bond { return Err(bad("export reward hold changes its original producer")); }
    Ok(if reservation.convicted_daa.is_some() { RewardReadinessV1::Denied }
        else if now <= reservation.material_until { RewardReadinessV1::Hold(reservation.material_until) }
        else { RewardReadinessV1::Ready })
}

pub(super) fn source(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    id: Hash64,
    born: &PalwClaimStateV2,
    target: Hash64,
    target_birth: &PalwClaimStateV2,
    retiring: bool,
) -> Result<RewardReadinessV1, PalwStateV2Error> {
    let base = source_without_export_guard_v1(state, params, now, id, born, target, target_birth, retiring)?;
    // Known void/closed denial must not depend on a new source hydration.
    if base == RewardReadinessV1::Denied { return Ok(base); }
    // live_source already checked this exact cohort's E conviction/material;
    // avoid duplicate original decode/leases on every normal live read.
    if state.claims.get(&id).is_some_and(|live| same_birth(live, born)) { return Ok(base); }
    Ok(base.join(exact_export_readiness_v1(state, params, now, id, born)?))
}
fn source_without_export_guard_v1(
    state: &PalwChainStateV2, params: &PalwStateParamsV2, now: u64,
    id: Hash64, born: &PalwClaimStateV2, target: Hash64,
    target_birth: &PalwClaimStateV2, retiring: bool,
) -> Result<RewardReadinessV1, PalwStateV2Error> {
    if let Some(live) = state.claims.get(&id).filter(|c| same_birth(c, born)) {
        if matches!(live.phase, PalwClaimPhaseV2::Voided { .. }) {
            return Ok(RewardReadinessV1::Denied);
        }
        if !inputs_known(state, params, id, born)? {
            return Ok(RewardReadinessV1::Hold(u64::MAX));
        }
        if live.execution_root != born.execution_root {
            return Err(bad("claim dependency changed its execution identity"));
        }
        return live_source(state, params, now, id, live, retiring && id == target && same_birth(born, target_birth));
    }
    if let Some(decision)=super::palw_claim_local_compact_sources_v1::recorded(state,id,born)?{return Ok(decision)}
    if let Some(decision)=super::palw_claim_local_legacy_terminal_v18::readiness_for_source_v18(state,params,now,id,born,target,target_birth)?{return Ok(decision)}
    // Never infer a successful retirement just from boundary carriages: the
    // claim may have been convicted and removed in the same transition.
    let mut latest = None;
    for (key, bytes) in frames(state) {
        let f = frame_checked(state, key, bytes)?;
        for (index, change) in f.native_changes.iter().enumerate() {
            if change.owner != PalwClaimLocalRowOwnerV1::ChainClaims || change.key.as_slice() != id.as_byte_slice() {
                continue;
            }
            let Some(old) = change.old.as_deref() else {
                continue;
            };
            let old = decoded::<PalwClaimStateV2>(old)?;
            if !same_birth(&old, born) {
                continue;
            }
            if let Some(new) = change.new.as_deref() {
                let new = decoded::<PalwClaimStateV2>(new)?;
                if same_birth(&new, born) {
                    continue;
                }
            }
            let order = (f.point.blue_score, f.point.daa_score, index);
            if latest.as_ref().is_some_and(|(previous, _)| *previous >= order) {
                continue;
            }
            let decision = if !matches!(old.phase, PalwClaimPhaseV2::Final { .. })
                || crate::palw_unresolved_bound_v1::palw_claim_lapsed_bound_v1(params, &old, f.point.daa_score)
            {
                RewardReadinessV1::Denied
            } else if !inputs_known(state, params, id, born)? {
                RewardReadinessV1::Hold(u64::MAX)
            } else {
                frame_read(&f, |prior, _| {
                    if !retired_exec_source_closed_v1(prior,params,id,&old,now)?{
                        return Ok(RewardReadinessV1::Hold(u64::MAX));
                    }
                    let wanted = cert_key(id);
                    let replacement = f.native_changes[..index].iter().rev().find(|event| {
                        event.owner == PalwClaimLocalRowOwnerV1::KernelRoute
                            && event.table == PALW_SECURITY_CERTIFICATION_TABLE_V1
                            && event.key.as_slice() == wanted
                    });
                    let raw = match replacement {
                        Some(event) => event.new.as_deref(),
                        None => cert_bytes(prior.kernel_route.as_ref(), id),
                    };
                    Ok(match approval(params, id, &old, raw)? {
                        RewardReadinessV1::Ready => RewardReadinessV1::Ready,
                        _ => RewardReadinessV1::Denied,
                    })
                })?
            };
            latest = Some((order, decision));
        }
    }
    Ok(latest.map_or(RewardReadinessV1::Hold(now), |(_, decision)| decision))
}

pub(super) fn origin_for(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<Option<PalwClaimLocalAdmissionContextV1>, PalwStateV2Error> {
    for (_, bytes) in frames(state) {
        let point =
            PalwBlockContextV2::deserialize_reader(&mut bytes.as_slice()).map_err(|_| bad("truncated liability frame point"))?;
        if point.daa_score == c.accepted_daa
            && point.blue_score == c.accepted_blue_score
            && let Some(found) = state.claim_local_admission_context_v1(params, &point, id)?
        {
            return Ok(Some(found));
        }
    }
    Ok(None)
}
pub(super) fn inputs_known(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<bool, PalwStateV2Error> {
    if let Some(known)=super::palw_claim_local_source_admission_v8::known_inputs_v8(state,id,c)? { return Ok(known); }
    let Some(origin) = origin_for(state, params, id, c)? else {
        return Ok(false);
    };
    if origin.original_claim().execution_root != c.execution_root {
        return Err(bad("reward input provenance changed its execution identity"));
    }
    Ok(match c.source {
        PalwClaimSourceV2::Attempt=>true,
        PalwClaimSourceV2::FreePrompt{..}=>origin.frame().accepted_objects.iter().any(|o|matches!(o,
            PalwConsensusObjectV2::FreePromptCommitted{claim,consumed_prefix_state,..} if *claim==id && consumed_prefix_state.is_genesis()))
            || prefix_rewards::prefix_use(&origin)?.is_some(),
    })
}

pub(super) fn kernel_claim_row<'a>(state: &'a PalwChainStateV2, id: &Hash64) -> Option<&'a Vec<u8>> {
    state.kernel_route.as_ref()?.rows.get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, id.as_byte_slice().to_vec()))
}

/// Committed dependency edges for one chain claim.
///
/// Prefix envelopes contribute a source chain claim only when the signed birth
/// identity is still the live row and is not later than the consumer. A funded
/// carrier that spends a stored winning-carrier transaction contributes that
/// source claim. A bound execution root contributes its prefix claim and each
/// slice's evidence claim, in the kernel namespace, only when that row is
/// actually stored. Shared execution roots and model or bond identity are not edges.
pub(super) fn dependency_inputs_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    id: Hash64,
    claim: &PalwClaimStateV2,
    origin: Option<&PalwClaimLocalAdmissionContextV1>,
) -> Result<BTreeSet<PalwClaimLocalWorkRefV1>, PalwStateV2Error> {
    let mut inputs = BTreeSet::new();
    if let Some(source)=super::palw_claim_local_source_admission_v8::for_stored_claim_v8(state,id,claim)? {
        source.visit_dependencies(|source,block,daa,blue| {
            if source!=id && let Some(row)=state.claims.get(&source)
                && (row.accepted_block,row.accepted_daa,row.accepted_blue_score)==(block,daa,blue)
                && daa<=claim.accepted_daa && blue<=claim.accepted_blue_score {
                inputs.insert(PalwClaimLocalWorkRefV1::ChainClaim{claim:source});
            }
            Ok(())
        })?;
    } else {
    if let Some(origin) = origin {
        if let Some(consumed) = prefix_rewards::prefix_use(origin)? {
            let source = consumed.intent.source;
            if source.source_claim != id
                && let Some(row) = state.claims.get(&source.source_claim)
                && row.accepted_block == source.accepted_block
                && row.accepted_daa == source.accepted_daa
                && row.accepted_blue_score == source.accepted_blue_score
                && row.accepted_daa <= claim.accepted_daa
                && row.accepted_blue_score <= claim.accepted_blue_score
            {
                inputs.insert(PalwClaimLocalWorkRefV1::ChainClaim { claim: source.source_claim });
            }
        }
    }
    }
    // A raw tag8 names no birth. Use the SAME actual native carrier facts as
    // reward readiness, with authenticated historical fallback only. A retired
    // old owner never becomes an edge to whichever same raw ID is live today.
    if let Some(cursor)=super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(state,params,id,claim)? {
        use super::palw_claim_local_carrier_lineage_v9 as lineage;
        let point=cursor.accepting_point();
        let mut visit=|fact:&lineage::CarrierFactV9,owner:lineage::CarrierSourceViewV9| {
            let source_point=owner.accepting_point();
            if fact.consumer()!=id || fact.consumer_point()!=point
                || fact.source()!=owner.source() || fact.source_point()!=source_point
                || fact.spent_txid()!=owner.winning_txid()
                || source_point.blue_score>=point.blue_score || source_point.daa_score>point.daa_score {
                return Err(bad("checkpoint carrier fact differs from actual causal births"));
            }
            if lineage::is_exact_live_source_v9(state,&owner) {
                inputs.insert(PalwClaimLocalWorkRefV1::ChainClaim{claim:owner.source()});
            }
            Ok(())
        };
        if !lineage::visit_consumer_facts_v9(state,id,point,&mut visit)? {
            // False means unavailable provenance, never permission/closure.
            // The existing readiness reader independently keeps that work held.
            lineage::visit_legacy_consumer_facts_v9(state,params,&cursor,&mut visit)?;
        }
    }
    if let Some(root) = state.exec_v2.roots.get(&id)
        && !matches!(root.prefix, crate::palw_work_slice_v2::PalwWorkPrefixStageV2::Unbound)
    {
        let mut consider = |input: Hash64| -> Result<(),PalwStateV2Error> {
            if input != ZERO_HASH64 {
                let _key=crate::palw_native_resource_v1::reserve_native_requests_v1(64).map_err(PalwStateV2Error::NativeResource)?;
                if kernel_claim_row(state, &input).is_some() {
                    inputs.insert(PalwClaimLocalWorkRefV1::KernelClaim { claim: input });
                }
            }
            Ok(())
        };
        consider(root.prefix_claim)?;
        for (_, slice) in state.exec_v2.slices.range((id, 0)..=(id, u32::MAX)) {
            consider(slice.evidence_root)?;
        }
    }
    Ok(inputs)
}

pub(super) fn reward_readiness_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    id: Hash64,
    retiring: bool,
) -> Result<RewardReadinessV1, PalwStateV2Error> {
    if !params.claim_local_settlement_active_at(now) {
        return Ok(RewardReadinessV1::Ready);
    }
    let Some(target) = state.claims.get(&id) else {
        return Ok(RewardReadinessV1::Denied);
    };
    recorded_reward_readiness_v1(state,params,now,id,target,retiring)
}
pub(super) fn recorded_reward_readiness_v1(state:&PalwChainStateV2,params:&PalwStateParamsV2,now:u64,id:Hash64,target:&PalwClaimStateV2,retiring:bool)
    ->Result<RewardReadinessV1,PalwStateV2Error>{
    let Some(origin) = super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(state,params,id,target)? else {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    };
    if !inputs_known(state, params, id, target)? {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    }
    let birth = origin.original_claim();
    let mut result = carrier_dependencies::readiness(state, params, now, id, target, id, birth, retiring)?;
    let mut scoped=super::palw_claim_local_legacy_terminal_v18::visit_cohort_members_v18(state,params,id,birth,|source_id,c|{
        result=result.join(cohort_member_readiness_v15(state,params,now,source_id,c,id,birth,retiring)?);
        Ok(if result==RewardReadinessV1::Denied{std::ops::ControlFlow::Break(())}else{std::ops::ControlFlow::Continue(())})
    })?;
    if !scoped&&result!=RewardReadinessV1::Denied{scoped=super::palw_claim_local_execution_cohort_archive_v15::visit_snapshot_members_until_v15(state,id,birth,|source_id,c|{
        result=result.join(cohort_member_readiness_v15(state,params,now,source_id,c,id,birth,retiring)?);
        Ok(if result==RewardReadinessV1::Denied{std::ops::ControlFlow::Break(())}else{std::ops::ControlFlow::Continue(())})
    })?;}
    if result==RewardReadinessV1::Denied{return Ok(result)}
    if !scoped {
        // Incomplete census is not an empty or Ready universe. Join only real
        // birth-visible native facts first, then keep the full legacy fallback.
        // In particular a denied exact A5 source survives F0/raw-S1 reuse.
        if birth.execution_root!=ZERO_HASH64{
            let _complete=super::palw_claim_local_execution_cohort_census_v15::visit_known_birth_members_v15(state,birth.execution_root,origin.accepting_point(),|source_id,c|{
                result=result.join(cohort_member_readiness_v15(state,params,now,source_id,c,id,birth,retiring)?);
                Ok(if result==RewardReadinessV1::Denied{std::ops::ControlFlow::Break(())}else{std::ops::ControlFlow::Continue(())})
            })?;
            if result==RewardReadinessV1::Denied{return Ok(result)}
        }
    // Sources visible at birth include legacy live claims. Older frames also
    // preserve already-retired sources and transient insert/void/remove runs.
    for (key, bytes) in frames(state) {
        let f = frame_checked(state, key, bytes)?;
        if f.point.daa_score > birth.accepted_daa || f.point.blue_score > birth.accepted_blue_score {
            continue;
        }
        let mut consider = |source_id: Hash64, c: &PalwClaimStateV2| -> Result<(), PalwStateV2Error> {
            if c.execution_root != ZERO_HASH64
                && c.execution_root == birth.execution_root
                && c.accepted_daa <= birth.accepted_daa
                && c.accepted_blue_score <= birth.accepted_blue_score
            {
                result = result.join(source(state, params, now, source_id, c, id, birth, retiring)?);
                result = result.join(carrier_dependencies::readiness(state, params, now, source_id, c, id, birth, retiring)?);
            }
            Ok(())
        };
        frame_read(&f, |prior, current| {
            for (source_id, c) in prior.claims.iter().chain(current.claims.iter()) {
                consider(*source_id, c)?;
            }
            Ok(())
        })?;
        for event in &f.native_changes {
            if event.owner == PalwClaimLocalRowOwnerV1::ChainClaims {
                if event.key.len() != 64 {
                    return Err(bad("claim journal key has wrong width"));
                }
                if let Some(raw) = event.new.as_deref() {
                    let c = decoded::<PalwClaimStateV2>(raw)?;
                    let source_id = Hash64::from_bytes(event.key.as_slice().try_into().expect("checked fixed width"));
                    consider(source_id, &c)?;
                }
            }
        }
    }
    }
    result=result.join(super::palw_claim_local_compact_sources_v1::shared_root(state,birth)?);
    Ok(result)
}

/// RFC-0023 stage 2 (ADR-0186), the per-claim settlement read. Under the dormant
/// `palw_claim_local_settlement_v1` fence, whether one claim's effects are settled
/// is decided by that claim and its actual dependency closure (same non-zero
/// execution root visible at its admission, its own Gate C certification, court
/// / prosecution / native dependency windows) — never by an unrelated unresolved
/// claim elsewhere in the shared DAG. Below the fence the answer is `Dormant` and
/// callers keep the shared-frontier read (`palw_settlement_v1`) byte for byte.
///
/// `Denied` withholds credit only (void, conviction, lapse of a stage-1 bound, or
/// of a dependency); it is never a fraud verdict and moves no collateral.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwClaimLocalSettlementV1 {
    /// Stage 2 is not in force at `now`; use the shared-frontier settlement read.
    Dormant,
    /// The claim is Final, SecurityCertified, past its own liability windows, and
    /// every source it actually depends on is in the same condition.
    Settled,
    /// Not yet settled; re-evaluate no earlier than this DAA (`u64::MAX`: until
    /// missing provenance or certification appears — held, not refused).
    Held { until_daa: u64 },
    /// No credit for this claim's effects. Not a slash.
    Denied,
    /// The state retains no live claim with this id (retired or unknown): this
    /// read has no answer; it never guesses one.
    NotRetained,
}

/// The read itself. `now` is the DAA score the caller evaluates at (normally the
/// sink's). Pure over `state`/`params`; no write, no fence bypass.
pub fn palw_claim_local_settlement_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim_id: Hash64,
    now: u64,
) -> Result<PalwClaimLocalSettlementV1, PalwStateV2Error> {
    if !params.claim_local_settlement_active_at(now) {
        return Ok(PalwClaimLocalSettlementV1::Dormant);
    }
    if !state.claims.contains_key(&claim_id) {
        return Ok(PalwClaimLocalSettlementV1::NotRetained);
    }
    Ok(match reward_readiness_v1(state, params, now, claim_id, false)? {
        RewardReadinessV1::Ready => PalwClaimLocalSettlementV1::Settled,
        RewardReadinessV1::Hold(until_daa) => PalwClaimLocalSettlementV1::Held { until_daa },
        RewardReadinessV1::Denied => PalwClaimLocalSettlementV1::Denied,
    })
}


#[path = "palw_claim_local_carrier_rewards_v1.rs"]
mod carrier_dependencies;

/// Historical native evidence is resolved against its real accepting birth and
/// retirement journal. Absence from the live registry never implies success.
pub fn palw_claim_local_recorded_settlement_v1(state:&PalwChainStateV2,params:&PalwStateParamsV2,claim:Hash64,recorded:&PalwClaimStateV2,now:u64)
    ->Result<PalwClaimLocalSettlementV1,PalwStateV2Error>{
    if !params.claim_local_settlement_active_at(now){return Ok(PalwClaimLocalSettlementV1::Dormant)}
    Ok(match recorded_reward_readiness_v1(state,params,now,claim,recorded,false)?{
        RewardReadinessV1::Ready=>PalwClaimLocalSettlementV1::Settled,
        RewardReadinessV1::Hold(until_daa)=>PalwClaimLocalSettlementV1::Held{until_daa},
        RewardReadinessV1::Denied=>PalwClaimLocalSettlementV1::Denied,
    })
}

/// Authenticate a retired EXEC source against the actual retirement journal's
/// retained native snapshot. Missing roots/evidence do not stand for a completed
/// execution; optimistic Final or matching replay never replaces selected OPV
/// approval. The ordinary no-EXEC source keeps its existing path.
pub(super) fn retired_exec_source_closed_v1(prior:&PalwStateCarriageV2,params:&PalwStateParamsV2,id:Hash64,terminal:&PalwClaimStateV2,now:u64)->Result<bool,PalwStateV2Error>{
    exec_source_closed_from_borrowed_owner_v1(&prior.exec_v2,&prior.exec_v2_rewards,prior.kernel_route.as_ref(),params,id,terminal,now)
}
/// Same exact closure predicate, while the actual retirement writer still owns
/// live EXEC/native paid authority. No whole-state carriage or fake old root.
pub(super) fn current_exec_source_closed_v1(state:&PalwChainStateV2,params:&PalwStateParamsV2,id:Hash64,terminal:&PalwClaimStateV2,now:u64)->Result<bool,PalwStateV2Error>{
    exec_source_closed_from_borrowed_owner_v1(&state.exec_v2,&state.exec_v2_rewards,state.kernel_route.as_ref(),params,id,terminal,now)
}
fn exec_source_closed_from_borrowed_owner_v1(
    exec:&crate::palw_work_slice_v2::PalwExecV2StateV1,
    rewards:&BTreeMap<Hash64,crate::palw_work_slice_v2::PalwExecV2RewardRowV1>,
    route:Option<&crate::palw_kernel_route_v1::PalwKernelRouteStateV1>,
    params:&PalwStateParamsV2,id:Hash64,terminal:&PalwClaimStateV2,now:u64,
)->Result<bool,PalwStateV2Error>{
    use crate::palw_work_slice_v2::{PalwWorkRootPhaseV2 as Phase,PalwWorkPrefixStageV2 as Prefix,PalwWorkSliceStageV2 as Slice};
    let Some(root)=exec.roots.get(&id)else{return Ok(!rewards.contains_key(&id))};
    let settled=match root.phase{Phase::Settled{settled_daa}=>settled_daa,_=>return Ok(false)};
    if !matches!(terminal.phase,PalwClaimPhaseV2::Final{final_daa} if final_daa==settled)||!matches!(root.prefix,Prefix::Verified{..})
        ||root.root_bond!=terminal.bond||root.class_id!=terminal.class_id||root.anchor!=terminal.accepted_block
        ||root.canonical_job_id!=terminal.job_identity||root.pending!=0||root.next_index!=root.slice_count()
        ||root.accepted_work!=root.slice_work_total()||root.verified_work!=root.accepted_work{return Ok(false)}
    let Some(route)=route else{return Ok(false)};
    let floor=super::palw_kernel_route_fold_v1::exec_liability_floor_v1(root.expiry_daa,params.window_court(),&route.header.policy)?;
    if now<=floor||rewards.get(&id).is_some_and(|r|now<=r.liability_until){return Ok(false)}
    let mut count=0u32;
    for ((_,index),slice)in exec.slices.range((id,0)..=(id,u32::MAX)){
        if *index!=count||slice.accepted_daa>settled||!matches!(slice.stage,Slice::Verified{verified_daa,..} if verified_daa<=settled){return Ok(false)}count=count.checked_add(1).ok_or(PalwStateV2Error::Overflow("retired EXEC slice inventory"))?;
    }
    if count!=root.slice_count(){return Ok(false)}
    let _keys=crate::palw_native_resource_v1::reserve_native_requests_v1(512).map_err(PalwStateV2Error::NativeResource)?;
    for claim in std::iter::once(root.prefix_claim).chain(exec.slices.range((id,0)..=(id,u32::MAX)).map(|(_,s)|s.evidence_root)){
        let Some(raw)=route.rows.iter().find(|((t,k),_)|*t==misaka_palw_kernel::rows::TABLE_CLAIMS_V1&&k.as_slice()==claim.as_byte_slice()).map(|(_,r)|r)else{return Ok(false)};
        let row=decoded::<misaka_palw_kernel::ledger::ClaimRowV1>(raw)?;
        if row.liability_until.is_none_or(|until|now<=until)||!matches!(crate::palw_exec_v2_verify::palw_exec_v2_claim_outcome_v1(&row,now),crate::palw_exec_v2_verify::PalwSliceVerificationV1::Verified{final_daa} if final_daa<=settled){return Ok(false)}
        // Require an actual selected-approval source. The legacy non-OPV
        // compatibility branch of the general gate cannot close this archive.
        let authority=match route.paid_claim_authority_for_base_checked_v1(&claim.as_bytes()){
            Ok(Some(a))=>a,Ok(None)=>return Ok(false),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r))=>return Err(PalwStateV2Error::NativeResource(r)),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_))=>return Ok(false),
        };
        if authority.class.base_class!=row.class_binding_id||authority.job.base_job!=row.job_id
            ||authority.enrollment.producer!=row.producer||authority.authorized_daa!=row.committed_daa
            ||authority.enrollment.class!=authority.class.id()||authority.enrollment.job!=authority.job.id()
            ||authority.enrollment.lifetime.liability_until!=row.liability_until.expect("checked source horizon")
            ||!crate::palw_exec_v2_verify::palw_exec_v2_credit_approval_v1(route,&claim){return Ok(false)}
    }
    Ok(true)
}

/// Membership is data authority only. Existing source() decides native status
/// and exact E holds first; a known denial never hydrates a later origin.
fn cohort_member_readiness_v15(
    state:&PalwChainStateV2,params:&PalwStateParamsV2,now:u64,
    id:Hash64,born:&PalwClaimStateV2,target:Hash64,target_birth:&PalwClaimStateV2,retiring:bool,
)->Result<RewardReadinessV1,PalwStateV2Error>{
    let base=source(state,params,now,id,born,target,target_birth,retiring)?;
    if base==RewardReadinessV1::Denied{return Ok(base)}
    // Preserve the existing real-origin traversal whenever it is available.
    // A native closed summary does not suppress still-authentic input evidence.
    let origin=super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(state,params,id,born)?;
    if origin.is_some() || state.claims.get(&id).is_some_and(|c|same_birth(c,born)){
        drop(origin);
        return Ok(base.join(carrier_dependencies::readiness(state,params,now,id,born,target,target_birth,retiring)?));
    }
    let exact=super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(state,id,born)?;
    let legacy=if exact.is_none(){super::palw_claim_local_compact_sources_v1::read(state,id)?}else{None};
    let closed=exact.as_deref().or_else(||legacy.as_deref().filter(|c|same_birth(&c.original,born)));
    if let Some(closed)=closed{
        if !same_birth(&closed.original,born)||closed.claim!=id
            ||closed.original.execution_root!=born.execution_root||closed.original.class_id!=born.class_id
            ||closed.original.bond!=born.bond||closed.original.output_root!=born.output_root||closed.original.trace_root!=born.trace_root
            ||!matches!((&closed.original.source,&born.source),(PalwClaimSourceV2::Attempt,PalwClaimSourceV2::Attempt))
                &&!matches!((&closed.original.source,&born.source),(PalwClaimSourceV2::FreePrompt{quanta:a,..},PalwClaimSourceV2::FreePrompt{quanta:b,..})if a==b){
            return Err(bad("cohort closed source substitutes immutable birth identity"));
        }
        // A5's authoritative denial is not replaced by a later raw-ID state.
        if !closed.permitted{return Ok(RewardReadinessV1::Denied)}
        if !matches!(closed.terminal_phase,PalwClaimPhaseV2::Final{..})
            ||closed.closed_at.daa_score>now||closed.birth.daa_score!=born.accepted_daa
            ||closed.birth.blue_score!=born.accepted_blue_score{
            return Err(bad("cohort permitted closure lacks actual Final/clock"));
        }
        // Current mint_source explicitly requires Ready selected approval even
        // for permitted S1. An imported legacy/pre-Gate-C body is not evidence
        // for this NEW no-origin shortcut; preserve it as typed Missing/Hold,
        // rather than relabeling old-rule data malformed or certifying it.
        if !crate::palw_rfc0018_v1::palw_gate_c_governs_v1(params,&closed.original){
            return Ok(base.join(RewardReadinessV1::Hold(u64::MAX)));
        }
        if approval(params,id,&closed.original,Some(&closed.approval))?!=RewardReadinessV1::Ready{
            return Err(bad("cohort permitted closure lacks native selected approval"));
        }
        // Actual native closure already settled this retired source's inputs.
        // Keep base's exact E material Hold, rather than inventing Ready or
        // requiring the released A8/F world to rehydrate a completed chain.
        return Ok(base);
    }
    Ok(base.join(carrier_dependencies::readiness(state,params,now,id,born,target,target_birth,retiring)?))
}
