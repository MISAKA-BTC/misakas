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
fn decoded<T: borsh::BorshDeserialize + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeWireScanV1>(
    bytes: &[u8],
) -> Result<NativeRetainedV1<T>, PalwStateV2Error> {
    crate::palw_claim_local_v1::decode_claim_local_value_v1(bytes).map_err(|e| match e {
        NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
        NativeDecodeErrorV1::Decode(e) => bad(&format!("claim-local reward metadata: {e}")),
    })
}
fn frame_read<T>(
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

fn same_birth(a: &PalwClaimStateV2, b: &PalwClaimStateV2) -> bool {
    a.accepted_daa == b.accepted_daa && a.accepted_blue_score == b.accepted_blue_score
}
fn cert_key(id: Hash64) -> [u8; 65] {
    let mut key = [0; 65];
    key[0] = CERTIFICATION_CLAIM_PREFIX_V1;
    key[1..].copy_from_slice(&palw_certification_id_v1(&id));
    key
}
pub(super) fn cert_bytes<'a>(route: Option<&'a crate::palw_kernel_route_v1::PalwKernelRouteStateV1>, id: Hash64) -> Option<&'a [u8]> {
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
fn prepare_exec_reader(
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
pub(super) fn exec_dependency_readiness_v1(state: &PalwChainStateV2, id: Hash64, now: u64) -> Result<RewardReadinessV1, PalwStateV2Error> {
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
    if !retiring_self {
        let until = c.trace_retention_daa.max(final_daa.saturating_add(params.window_court()));
        if now <= until {
            decision = decision.join(RewardReadinessV1::Hold(until));
        }
        if let RewardReadinessV1::Hold(until) = exec_window(state, params, id)? {
            if now <= until {
                decision = decision.join(RewardReadinessV1::Hold(until));
            }
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
fn source(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    id: Hash64,
    born: &PalwClaimStateV2,
    target: Hash64,
    target_birth: &PalwClaimStateV2,
    retiring: bool,
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
                    if prior.exec_v2.roots.contains_key(&id) || prior.exec_v2_rewards.contains_key(&id) {
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

fn origin_for(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<Option<PalwClaimLocalAdmissionContextV1>, PalwStateV2Error> {
    for (_, bytes) in frames(state) {
        let point =
            PalwBlockContextV2::deserialize_reader(&mut bytes.as_slice()).map_err(|_| bad("truncated liability frame point"))?;
        if point.daa_score == c.accepted_daa && point.blue_score == c.accepted_blue_score {
            if let Some(found) = state.claim_local_admission_context_v1(params, &point, id)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}
fn inputs_known(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<bool, PalwStateV2Error> {
    let Some(origin) = origin_for(state, params, id, c)? else {
        return Ok(false);
    };
    if origin.original_claim().execution_root != c.execution_root {
        return Err(bad("reward input provenance changed its execution identity"));
    }
    Ok(match c.source {
        PalwClaimSourceV2::Attempt=>true,
        PalwClaimSourceV2::FreePrompt{..}=>origin.frame().accepted_objects.iter().any(|o|matches!(o,
            PalwConsensusObjectV2::FreePromptCommitted{claim,consumed_prefix_state,..} if *claim==id && consumed_prefix_state.is_genesis())),
    })
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
    let Some(origin) = origin_for(state, params, id, target)? else {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    };
    if !inputs_known(state, params, id, target)? {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    }
    let mut result = live_source(state, params, now, id, target, retiring)?;
    let birth = origin.original_claim();
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
