//! RFC-0023 stage 2 (ADR-0186), §4.1/§4.5: the fold-derived claim-local checkpoint.
//!
//! Past the dormant `palw_claim_local_settlement_v1` fence every block's fold derives the
//! expected `PalwClaimLocalCheckpointV1` from the committed state it just produced — every live
//! claim, its phase / Gate C certification / own liability windows, the open set's claim and court
//! rows (inline material) and the original admission frames (bound by key in `liability_state`;
//! the frames themselves are rows of the same rooted namespace). It writes that checkpoint as the
//! aux row `144 / 'C' ‖ 0^64`, so:
//!
//! * the row is in the state root (kernel-route aux) and in the pruning-point carriage, so a pruned
//!   IBD import carries it and the load-time check (`assert_claim_local_checkpoint_v1`) rebuilds the
//!   expected value from the imported state and refuses a difference;
//! * the write is a `KernelRouteRow` delta entry, so a reorg reverts it exactly;
//! * the frontier (fork choice's first key, settlement, pruning's input) is raised to the
//!   checkpoint's IRREVERSIBLE frontier — the deepest attempt whose whole dependency closure is
//!   Final, SecurityCertified and past every liability window. An unrelated unresolved claim is not
//!   in that closure and holds nothing.
//!
//! Nothing here is caller authority: the stored row is only ever compared with a recomputation.
use super::palw_claim_local_rewards_v1 as rewards;
use super::*;
use crate::palw_claim_local_v1::*;
use borsh::BorshDeserialize;

/// The single checkpoint row's key: prefix `'C'`, zero tail (one row per state; the point is inside).
pub fn palw_claim_local_checkpoint_key_v1() -> [u8; 65] {
    let mut key = [0u8; 65];
    key[0] = b'C';
    key
}

fn bad(why: &str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why.into())
}

/// The canonical open-liability projection. Owned and borsh-encoded field by field, so the bytes are
/// a pure function of the state: (claim id, claim row, its open court sessions) for every claim whose
/// liability is not closed, then the accepting blocks of their retained admission frames.
#[derive(borsh::BorshSerialize)]
struct LiabilityStateV1 {
    open: Vec<(Hash64, PalwClaimStateV2, Vec<(Hash64, PalwCourtSessionStateV2)>)>,
    birth_frames: Vec<Hash64>,
}

/// Expected checkpoint at `point` from `state` (which must already be the state AT `point`).
/// `Ok(None)` below the fence. Errors only on corrupt retained rows or local capacity.
pub(super) fn expected_claim_local_checkpoint_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    point: &PalwBlockContextV2,
) -> Result<Option<PalwClaimLocalCheckpointV1>, PalwStateV2Error> {
    if !params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(None);
    }
    let now = point.daa_score;
    // Accepting (chain) block of each retained admission frame, by its point.
    let mut frame_points: BTreeMap<(u64, u64), Hash64> = BTreeMap::new();
    for (_, bytes) in rewards::frames(state) {
        let p = PalwBlockContextV2::deserialize_reader(&mut bytes.as_slice()).map_err(|_| bad("truncated liability frame point"))?;
        frame_points.insert((p.daa_score, p.blue_score), p.block);
    }
    let mut courts: BTreeMap<Hash64, Vec<(Hash64, PalwCourtSessionStateV2)>> = BTreeMap::new();
    for (sid, session) in &state.court_sessions {
        courts.entry(session.claim).or_default().push((*sid, session.clone()));
    }
    let mut records = BTreeMap::new();
    let mut material = BTreeMap::new();
    let mut liability = LiabilityStateV1 { open: Vec::new(), birth_frames: Vec::new() };
    for (id, c) in &state.claims {
        let work = PalwClaimLocalWorkRefV1::ChainClaim { claim: *id };
        let voided = matches!(c.phase, PalwClaimPhaseV2::Voided { .. });
        let final_claim = matches!(c.phase, PalwClaimPhaseV2::Final { .. });
        let certified = final_claim
            && rewards::approval(params, *id, c, rewards::cert_bytes(state.kernel_route.as_ref(), *id))?
                == rewards::RewardReadinessV1::Ready;
        // Own liability only (court / trace / EXEC windows, prosecution, native dependency). Shared
        // execution roots are the checkpoint's dependency closure, not this flag.
        let liability_closed = if final_claim {
            rewards::live_source(state, params, now, *id, c, false)? == rewards::RewardReadinessV1::Ready
        } else if voided {
            !state.unresolved.contains(&(c.accepted_blue_score, *id)) && !courts.contains_key(id)
        } else {
            false
        };
        let birth = frame_points.get(&(c.accepted_daa, c.accepted_blue_score)).copied();
        let mut manifest = BTreeSet::new();
        if !liability_closed {
            let row = borsh::to_vec(c).expect("canonical claim row");
            let row_id = palw_liability_material_id_v1(&row);
            manifest.insert(row_id);
            material.insert(row_id, row);
            for (sid, session) in courts.get(id).into_iter().flatten() {
                let bytes = borsh::to_vec(&(*sid, session.clone())).expect("canonical court row");
                let mid = palw_liability_material_id_v1(&bytes);
                manifest.insert(mid);
                material.insert(mid, bytes);
            }
            liability.open.push((*id, c.clone(), courts.get(id).cloned().unwrap_or_default()));
            if let Some(block) = birth {
                liability.birth_frames.push(block);
            }
        }
        records.insert(
            work,
            PalwClaimLocalRecordV1 {
                work,
                accepted_daa: c.accepted_daa,
                accepted_blue_score: c.accepted_blue_score,
                accepting_block: birth.unwrap_or(c.accepted_block),
                anchors: matches!(c.source, PalwClaimSourceV2::Attempt),
                // No transaction-output → claim consumption map exists yet; shared execution roots
                // (the actual RFC §4.2 dependency the fold can prove) are carried by `execution_root`.
                inputs: BTreeSet::new(),
                execution_root: c.execution_root,
                // Each claim's collateral reservation is its own; none is shared, so none links claims.
                reservations: BTreeSet::new(),
                final_claim,
                certified,
                voided,
                liability_closed,
                material: manifest,
            },
        );
    }
    let liability_state = if liability.open.is_empty() { Vec::new() } else { borsh::to_vec(&liability).expect("canonical liability") };
    Ok(Some(PalwClaimLocalCheckpointV1 { point: *point, records, liability_state, material }))
}

/// The stored checkpoint row, if any (raw; never authority).
pub(super) fn stored_claim_local_checkpoint_v1(
    state: &PalwChainStateV2,
) -> Result<Option<crate::palw_native_resource_v1::NativeRetainedV1<PalwClaimLocalCheckpointV1>>, PalwStateV2Error> {
    let key = palw_claim_local_checkpoint_key_v1();
    let Some(bytes) = state.kernel_route.as_ref().and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec()))) else {
        return Ok(None);
    };
    palw_decode_claim_checkpoint_v1(bytes).map(Some).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => bad(&format!("claim-local checkpoint: {e}")),
    })
}

/// **The claims this state's claim-local checkpoint holds as irreversible anchors** (audit D-1): `anchors` and
/// irreversible-eligible (credit-eligible with its whole dependency closure liability-closed) — exactly the set whose deepest member
/// is the checkpoint's irreversible frontier, the fold's frontier past the fence. Rule E reads it so a side's frontier is that same
/// rule restricted to the side, never held back by an unrelated live claim. Empty below `palw_claim_local_settlement_v1` (and for a
/// state with no point or no stored row), so every pre-fence reader is unchanged.
pub fn palw_claim_local_irreversible_anchors_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
) -> Result<BTreeSet<Hash64>, PalwStateV2Error> {
    let Some(point) = state.last_point else { return Ok(BTreeSet::new()) };
    if !params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(BTreeSet::new());
    }
    let Some(checkpoint) = stored_claim_local_checkpoint_v1(state)? else { return Ok(BTreeSet::new()) };
    let validated = checkpoint.validate(&point, &checkpoint.records, &checkpoint.liability_state).map_err(checkpoint_err)?;
    Ok(checkpoint
        .records
        .values()
        .filter(|r| r.anchors && validated.irreversible_eligible(r.work))
        .filter_map(|r| match r.work {
            PalwClaimLocalWorkRefV1::ChainClaim { claim } => Some(claim),
            _ => None,
        })
        .collect())
}

fn checkpoint_err(e: PalwClaimCheckpointErrorV1) -> PalwStateV2Error {
    match e {
        PalwClaimCheckpointErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
        PalwClaimCheckpointErrorV1::Semantic(e) => bad(e),
    }
}

/// Fold step (end of every block past the fence): write the expected checkpoint and raise the
/// frontier to its irreversible frontier. The caller re-reads the frontier afterwards.
///
/// A claim can retire (leave `claims`) in the very block its terminal clock runs out, so it may
/// never be seen live with `liability_closed`. The chain's retirement rule is itself the end of the
/// claim's own liability, so a claim removed in THIS block from `Final` counts as irreversible here
/// when, in the parent's checkpoint, it was credit-eligible and every member of its dependency
/// closure was either liability-closed there or also retired from `Final` in this block.
pub(super) fn write_claim_local_checkpoint_v1(
    builder: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(());
    }
    // The state AT ctx: `last_point` is installed after this step, so judge with an explicit point.
    let Some(checkpoint) = expected_claim_local_checkpoint_v1(&builder.state, builder.params, ctx)? else { return Ok(()) };
    // Self-check: the fold's own value must pass the same validation a reader applies.
    let mut raise = {
        let validated = checkpoint.validate(ctx, &checkpoint.records, &checkpoint.liability_state).map_err(checkpoint_err)?;
        validated.irreversible_frontier().and_then(|r| match r.work {
            PalwClaimLocalWorkRefV1::ChainClaim { claim } => {
                builder.state.claims.get(&claim).map(|c| (c.accepted_blue_score, c.accepted_block))
            }
            _ => None,
        })
    };
    // Claims retired from `Final` in this block (old row Final, no new row, absent now).
    let retired: BTreeMap<Hash64, (u64, BlockHash)> = builder
        .entries
        .iter()
        .filter_map(|entry| match entry {
            PalwDeltaEntryV2::Claim { key, old: Some(old), new: None }
                if matches!(old.phase, PalwClaimPhaseV2::Final { .. })
                    && matches!(old.source, PalwClaimSourceV2::Attempt)
                    && !builder.state.claims.contains_key(key) =>
            {
                Some((*key, (old.accepted_blue_score, old.accepted_block)))
            }
            _ => None,
        })
        .collect();
    if !retired.is_empty()
        && let Some(parent_point) = parent.last_point
        && let Some(prior) = expected_claim_local_checkpoint_v1(parent, builder.params, &parent_point)?
    {
        let validated = prior.validate(&parent_point, &prior.records, &prior.liability_state).map_err(checkpoint_err)?;
        for (id, (blue, block)) in &retired {
            let work = PalwClaimLocalWorkRefV1::ChainClaim { claim: *id };
            if !prior.records.contains_key(&work) || !validated.credit_eligible(work) {
                continue;
            }
            let Ok(closure) = validated.dependencies(work) else { continue };
            let closed = closure.iter().all(|dep| {
                prior.records.get(dep).is_some_and(|r| r.liability_closed)
                    || matches!(dep, PalwClaimLocalWorkRefV1::ChainClaim { claim } if retired.contains_key(claim))
            });
            if closed && raise.is_none_or(|(b, _)| *blue > b) {
                raise = Some((*blue, *block));
            }
        }
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    builder.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &palw_claim_local_checkpoint_key_v1(), Some(&checkpoint))?;
    // RFC-0023 §4.3/§4.5: past the fence fork choice, settlement and the pruning input read ONE frontier,
    // and it moves only on irreversible claim-local credit: the parent's frontier, raised by this block's
    // irreversible anchor. The shared-prefix / Gate C value computed earlier in this block is superseded
    // (an unresolved A no longer caps it; an irreversible anchor is past every liability, so no later
    // conviction can reach it and there is nothing for Gate C's rollback to undo). At the fence block the
    // inherited frontier is the parent's, unchanged.
    let base = (parent.safe_frontier_blue_score, parent.safe_frontier);
    let target = match raise {
        Some(r) if r.0 > base.0 => r,
        _ => base,
    };
    builder.state.safe_frontier_blue_score = target.0;
    builder.state.safe_frontier = target.1;
    Ok(())
}

/// Load-time / import-time check: past the fence the stored row must equal the recomputation at
/// the state's own last point (pruned IBD imports land here through the carriage load).
pub(super) fn assert_claim_local_checkpoint_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
    let Some(point) = state.last_point else { return Ok(()) };
    let stored = stored_claim_local_checkpoint_v1(state)?;
    let expected = expected_claim_local_checkpoint_v1(state, params, &point)?;
    match (stored, expected) {
        (None, None) => Ok(()),
        (Some(_), None) => Err(bad("claim-local checkpoint below its fence")),
        (None, Some(_)) => Err(bad("claim-local checkpoint missing past its fence")),
        (Some(stored), Some(expected)) => {
            stored.validate(&point, &expected.records, &expected.liability_state).map_err(checkpoint_err)?;
            if stored.material != expected.material {
                return Err(bad("claim-local checkpoint material differs from the fold"));
            }
            Ok(())
        }
    }
}

/// RFC-0023 stage 2 checkpoint read for one claim (RPC / settlement consumers). `None` below the
/// fence or when the state retains no checkpoint/record; the read validates the stored row against
/// a recomputation first, so a stored row never stands on its own word.
pub fn palw_claim_local_checkpoint_status_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim: Hash64,
) -> Result<Option<PalwClaimLocalCheckpointStatusV1>, PalwStateV2Error> {
    let Some(point) = state.last_point else { return Ok(None) };
    let Some(expected) = expected_claim_local_checkpoint_v1(state, params, &point)? else { return Ok(None) };
    let Some(stored) = stored_claim_local_checkpoint_v1(state)? else {
        return Err(bad("claim-local checkpoint missing past its fence"));
    };
    let validated = stored.validate(&point, &expected.records, &expected.liability_state).map_err(checkpoint_err)?;
    let work = PalwClaimLocalWorkRefV1::ChainClaim { claim };
    let Some(record) = expected.records.get(&work) else { return Ok(None) };
    Ok(Some(PalwClaimLocalCheckpointStatusV1 {
        credit_eligible: validated.credit_eligible(work),
        irreversible: validated.irreversible_eligible(work),
        liability_closed: record.liability_closed,
        settled_through_daa: validated.irreversible_frontier().map(|r| r.accepted_daa),
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimLocalCheckpointStatusV1 {
    /// Final + certified + not voided across the whole dependency closure (reversible credit).
    pub credit_eligible: bool,
    /// ... and every member of the closure is past its liability: irreversible support.
    pub irreversible: bool,
    pub liability_closed: bool,
    /// DAA of the checkpoint's irreversible frontier (effects at or below it are settled).
    pub settled_through_daa: Option<u64>,
}
