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
use crate::palw_native_resource_v1::{NativeRetainedV1, reserve_native_requests_v1};
use borsh::BorshDeserialize;

#[path = "palw_claim_local_checkpoint_resources_v1.rs"]
mod resources;
#[path = "palw_claim_local_native_checkpoint_v1.rs"]
mod native_checkpoint;

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

/// Construct kernel claim records before binding their actual retained admission
/// points in `native_checkpoint::attach`. The temporary zero context cannot be
/// exported: a missing authenticated birth makes the complete builder fail.
/// Returns the open kernel ids for the liability projection.
fn attach_kernel_dependency_records_v1(
    state: &PalwChainStateV2,
    now: u64,
    kernel_refs: &BTreeSet<Hash64>,
    records: &mut BTreeMap<PalwClaimLocalWorkRefV1, PalwClaimLocalRecordV1>,
    material: &mut BTreeMap<Hash64, Vec<u8>>,
) -> Result<Vec<Hash64>, PalwStateV2Error> {
    let mut pending: Vec<(Hash64, Vec<u8>, PalwClaimLocalRecordV1)> = Vec::new();
    for id in kernel_refs {
        let Some(raw) = rewards::kernel_claim_row(state, id) else { continue };
        let row: misaka_palw_kernel::ledger::ClaimRowV1 =
            borsh::from_slice(raw).map_err(|_| bad("malformed kernel dependency row"))?;
        if row.committed_daa > now {
            continue;
        }
        use crate::palw_exec_v2_verify::PalwSliceVerificationV1 as V;
        let outcome = crate::palw_exec_v2_verify::palw_exec_v2_claim_outcome_v1(&row, now);
        let voided = matches!(outcome, V::ProvenFalse | V::Defaulted);
        let final_claim = matches!(outcome, V::Verified { .. });
        let certified = final_claim
            && state
                .kernel_route
                .as_ref()
                .is_some_and(|route| crate::palw_exec_v2_verify::palw_exec_v2_selected_approval_v1(route, id));
        let liability_closed = (voided || certified) && row.liability_until.is_some_and(|until| now > until);
        let work = PalwClaimLocalWorkRefV1::KernelClaim { claim: *id };
        pending.push((
            *id,
            raw.clone(),
            PalwClaimLocalRecordV1 {
                work,
                accepted_daa: row.committed_daa,
                accepted_blue_score: 0,
                accepting_block: kernel_anchor_v1(state, *id),
                anchors: false,
                inputs: BTreeSet::new(),
                execution_root: kaspa_hashes::ZERO_HASH64,
                reservations: BTreeSet::new(),
                final_claim,
                certified,
                voided,
                liability_closed,
                material: BTreeSet::new(),
            },
        ));
    }
    for record in records.values_mut() {
        let accepted_daa = record.accepted_daa;
        let accepted_blue_score = record.accepted_blue_score;
        record.inputs.retain(|input| match input {
            PalwClaimLocalWorkRefV1::KernelClaim { claim } => pending.iter().any(|(id, _, kernel)| {
                id == claim && kernel.accepted_daa <= accepted_daa && kernel.accepted_blue_score <= accepted_blue_score
            }),
            _ => true,
        });
    }
    let mut open = Vec::new();
    for (id, raw, mut record) in pending {

        if !record.liability_closed {
            let mid = palw_liability_material_id_v1(&raw);
            record.material.insert(mid);
            material.insert(mid, raw);
            open.push(id);
        }
        records.insert(record.work, record);
    }
    Ok(open)
}

fn kernel_anchor_v1(state: &PalwChainStateV2, id: Hash64) -> Hash64 {
    let mut best: Option<(u64, Hash64)> = None;
    for (root_id, root) in &state.exec_v2.roots {
        let named = root.prefix_claim == id
            || state.exec_v2.slices.range((*root_id, 0)..=(*root_id, u32::MAX)).any(|(_, slice)| slice.evidence_root == id);
        if !named {
            continue;
        }
        let candidate = (root.opened_daa, root.anchor);
        if best.is_none_or(|previous| candidate < previous) {
            best = Some(candidate);
        }
    }
    best.map(|(_, anchor)| anchor).unwrap_or(kaspa_hashes::ZERO_HASH64)
}

/// Expected checkpoint at `point` from `state` (which must already be the state AT `point`).
/// `Ok(None)` below the fence. Errors only on corrupt retained rows or local capacity.
pub(super) fn expected_claim_local_checkpoint_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    point: &PalwBlockContextV2,
) -> Result<Option<NativeRetainedV1<PalwClaimLocalCheckpointV1>>, PalwStateV2Error> {
    expected_with_base_observer_v3(state,params,point,&mut |_|Ok(()))
}
fn expected_with_base_observer_v3(
    state:&PalwChainStateV2,params:&PalwStateParamsV2,point:&PalwBlockContextV2,
    observer:&mut dyn FnMut(&BTreeMap<PalwClaimLocalWorkRefV1,PalwClaimLocalRecordV1>)->Result<(),PalwStateV2Error>,
)->Result<Option<NativeRetainedV1<PalwClaimLocalCheckpointV1>>,PalwStateV2Error>{
    if !params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(None);
    }
    // Admit the concrete projection and its construction peak before maps, deep
    // copies or material encodings allocate. The returned owner keeps the lease.
    let requests=palw_claim_local_checkpoint_construction_requests_v24(state,params,point)?;
    let lease = reserve_native_requests_v1(requests).map_err(PalwStateV2Error::NativeResource)?;
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
    let mut kernel_refs: BTreeSet<Hash64> = BTreeSet::new();
    for (id, c) in &state.claims {
        let work = PalwClaimLocalWorkRefV1::ChainClaim { claim: *id };
        let voided = matches!(c.phase, PalwClaimPhaseV2::Voided { .. });
        let final_claim = matches!(c.phase, PalwClaimPhaseV2::Final { .. });
        let readiness = if final_claim {
            Some(rewards::reward_readiness_v1(state, params, now, *id, false)?)
        } else {
            None
        };
        let certified = final_claim
            && rewards::approval(params, *id, c, rewards::cert_bytes(state.kernel_route.as_ref(), *id))?
                == rewards::RewardReadinessV1::Ready
            // Retired dependencies no longer have a graph vertex. Their retained
            // history can still deny this claim's credit, without changing its
            // raw Gate C approval or attributing fraud to the consumer.
            && readiness != Some(rewards::RewardReadinessV1::Denied);
        // The irreversible checkpoint must use the same retained admission and historical
        // dependency reader as monetary settlement. A live-only check loses retired sources
        // and can treat missing provenance as an independently closed liability.
        let liability_closed = if final_claim {
            readiness == Some(rewards::RewardReadinessV1::Ready)
        } else if voided {
            !state.unresolved.contains(&(c.accepted_blue_score, *id)) && !courts.contains_key(id)
        } else {
            false
        };
        // A8 binds the actual executing/accepting point even when an Attempt
        // was carried by a different RED block and its old full F is gone.
        let scoped_birth = match super::palw_claim_local_source_admission_v8::for_stored_claim_v8(state,*id,c)? {
            Some(source)=>Some(source.accepting_point().block),
            None=>super::palw_claim_local_execution_cohort_archive_v15::snapshot_point_v15(state,*id,c)?.map(|point|point.block),
        };
        let birth = scoped_birth.or_else(||frame_points.get(&(c.accepted_daa, c.accepted_blue_score)).copied());
        let mut manifest = BTreeSet::new();
        if !liability_closed {
            let row = borsh::to_vec(c).expect("canonical claim row");
            let row_id = palw_liability_material_id_v1(&row);
            manifest.insert(row_id);
            material.insert(row_id, row);
            for (sid, session) in courts.get(id).into_iter().flatten() {
                let bytes = borsh::to_vec(&(sid, session)).expect("canonical court row");
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
                // Committed edges only: a signed prefix source still live at its admitted
                // birth, and a bound execution root's stored prefix / slice evidence rows.
                // Shared execution roots stay on `execution_root`; they are not copied here.
                inputs: {
                    let origin = if super::palw_claim_local_source_admission_v8::has_source_admission_v8(state,*id,c)? { None } else { rewards::origin_for(state,params,*id,c)? };
                    let mut inputs = rewards::dependency_inputs_v1(state, params, *id, c, origin.as_ref())?;
                    drop(origin);
                    inputs.retain(|input| match input {
                        PalwClaimLocalWorkRefV1::KernelClaim { claim } => {
                            kernel_refs.insert(*claim);
                            true
                        }
                        PalwClaimLocalWorkRefV1::ChainClaim { claim: source } => {
                            source != id && state.claims.contains_key(source)
                        }
                        _ => false,
                    });
                    inputs
                },
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
    if let Some(route)=&state.kernel_route{
        for ((table,key),_) in &route.rows{if *table==misaka_palw_kernel::rows::TABLE_CLAIMS_V1{
            let raw:misaka_palw_kernel::hash::Digest=borsh::from_slice(key).map_err(|_|bad("checkpoint kernel claim key"))?;
            kernel_refs.insert(Hash64::from_bytes(raw));
        }}
    }
    let native_context=native_checkpoint::context(state)?;
    let open_kernels = attach_kernel_dependency_records_v1(state, now, &kernel_refs, &mut records, &mut material)?;
    native_checkpoint::attach(state,params,now,&native_context,&mut records,&mut material)?;
    // Actual native base roles are observed BEFORE current material projection.
    // This includes equal IDs shared between base and projected roles.
    observer(&records)?;
    super::palw_claim_local_material_projection_v2::attach(state,point,&mut records,&mut material)?;
    let mut liability_state = if liability.open.is_empty() { Vec::new() } else { borsh::to_vec(&liability).expect("canonical liability") };
    if !open_kernels.is_empty() {
        // Open kernel inputs must stay inside the bound projection. Absent any
        // open chain claim, the suffix is the whole projection; otherwise it
        // rides after the chain projection and does not rewrite it.
        let suffix = borsh::to_vec(&open_kernels).expect("canonical kernel liability");
        if liability_state.is_empty() {
            liability_state = suffix;
        } else {
            liability_state.extend(suffix);
        }
    }
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(
        PalwClaimLocalCheckpointV1 { point: *point, records, liability_state, material },
        lease,
    )))
}

/// The stored checkpoint row, if any (raw; never authority).
pub(super) fn stored_claim_local_checkpoint_v1(
    state: &PalwChainStateV2,
) -> Result<Option<crate::palw_native_resource_v1::NativeRetainedV1<PalwClaimLocalCheckpointV1>>, PalwStateV2Error> {
    if let Some(checkpoint)=super::palw_claim_local_checkpoint_descriptor_v3::load(state)?{return Ok(Some(checkpoint))}
    if let Some(checkpoint)=super::palw_claim_local_descriptor_v2::load(state)?{return Ok(Some(checkpoint))}
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
    objects:&[PalwConsensusObjectV2],work:PalwBlockWorkV3<'_>,merged:&[PalwMergedWorkV1<'_>],
) -> Result<(), PalwStateV2Error> {
    if !builder.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(());
    }
    if super::palw_claim_local_checkpoint_descriptor_v3::try_material_checkpoint_v3(builder,parent,ctx,objects,work,merged)?{return Ok(())}
    // All native changes and this point's retained frame have already executed,
    // but LastPoint is journalled at the fold's final step. Historical readers
    // must see the actual executed point while deriving this checkpoint. Restore
    // the marker even on refusal; the normal LastPoint delta remains the owner.
    let previous_point = builder.state.last_point.replace(*ctx);
    let mut bases=None;
    let checkpoint = expected_with_base_observer_v3(&builder.state,builder.params,ctx,&mut |records|{
        bases=Some(super::palw_claim_local_checkpoint_descriptor_v3::prepare_bases_v3(records)?);Ok(())
    });
    builder.state.last_point = previous_point;
    let Some(checkpoint) = checkpoint? else { return Ok(()) };
    // Self-check: the fold's own value must pass the same validation a reader applies.
    let mut raise = {
        let validated = checkpoint.validate(ctx, &checkpoint.records, &checkpoint.liability_state).map_err(checkpoint_err)?;
        validated.irreversible_frontier().and_then(|r| match r.work {
            PalwClaimLocalWorkRefV1::ChainClaim { claim } => {
                builder.state.claims.get(&claim).map(|c| (c.accepted_blue_score, c.accepted_block))
            }
            PalwClaimLocalWorkRefV1::NativeMining39 { .. } => Some((r.accepted_blue_score,r.accepting_block)),
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
            // Retirement alone is not evidence that historical inputs settled. A source can
            // have left the live checkpoint before this consumer; consult retained provenance
            // with the retiring-self clock before making its frontier irreversible.
            let sourced = rewards::reward_readiness_v1(parent, builder.params, ctx.daa_score, *id, true)?
                == rewards::RewardReadinessV1::Ready;
            if closed && sourced && raise.is_none_or(|(b, _)| *blue > b) {
                raise = Some((*blue, *block));
            }
        }
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    super::palw_claim_local_checkpoint_descriptor_v3::write_prepared_v3(builder,&checkpoint,bases.as_ref().ok_or_else(||bad("actual checkpoint base observation missing"))?)?;
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
    super::palw_claim_local_context_store_v2::validate_context_rows_v2(state,params).map_err(super::palw_claim_local_paid_archive_v2::state_error)?;
    let Some(point) = state.last_point else { return Ok(()) };
    let stored = stored_claim_local_checkpoint_v1(state)?;
    let expected=expected_with_base_observer_v3(state,params,&point,&mut |records|super::palw_claim_local_checkpoint_descriptor_v3::validate_actual_bases_v3(state,records))?;
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

/// Native39's claim-local frontier consumer. Caller anchors are compared with
/// their committed precise work quantum before any checkpoint capability exists.
pub fn palw_claim_local_native39_frontier_eligible_v1(
    state:&PalwChainStateV2,params:&PalwStateParamsV2,point:&PalwBlockContextV2,claim:Hash64,
    anchor:&crate::palw_native39_frontier_v1::Native39FrontierAnchorV1,
)->Result<bool,PalwStateV2Error>{
    if !params.claim_local_settlement_active_at(point.daa_score)||anchor.claim!=claim{return Ok(false)}
    let anchors=crate::palw_native39_frontier_v1::native39_frontier_anchors_v1(state)?;
    if !anchors.iter().any(|(_,stored)|stored==anchor){return Ok(false)}
    let Some(checkpoint)=expected_claim_local_checkpoint_v1(state,params,point)?else{return Ok(false)};
    let validated=checkpoint.validate(point,&checkpoint.records,&checkpoint.liability_state).map_err(checkpoint_err)?;
    Ok(validated.irreversible_eligible(PalwClaimLocalWorkRefV1::NativeMining39{claim,work:anchor.work,quantum:anchor.quantum}))
}

/// Namespaced native work retained at this exact checkpoint. A Rule E caller
/// applies its own DAG/IBD exclusivity predicate to the real accepting point.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct PalwClaimLocalNativeFrontierRecordV1{
    pub work:PalwClaimLocalWorkRefV1,pub accepting_block:Hash64,pub accepted_blue_score:u64,pub accepted_daa:u64,pub irreversible:bool,
}
pub fn palw_claim_local_native_frontier_records_v1(state:&PalwChainStateV2,params:&PalwStateParamsV2)
    ->Result<NativeRetainedV1<Vec<PalwClaimLocalNativeFrontierRecordV1>>,PalwStateV2Error>{
    use crate::palw_native_resource_v1::TirNativeRequestsV1;
    let Some(point)=state.last_point else{return Ok(NativeRetainedV1::from_owner_reserved_v1(Vec::new(),None))};
    let Some(checkpoint)=expected_claim_local_checkpoint_v1(state,params,&point)?else{return Ok(NativeRetainedV1::from_owner_reserved_v1(Vec::new(),None))};
    let validated=checkpoint.validate(&point,&checkpoint.records,&checkpoint.liability_state).map_err(checkpoint_err)?;
    let count=checkpoint.records.keys().filter(|w|matches!(w,PalwClaimLocalWorkRefV1::NativeMining39{..})).count();
    let mut q=TirNativeRequestsV1::new();q.exact_v1::<PalwClaimLocalNativeFrontierRecordV1>(count as u64).map_err(PalwStateV2Error::NativeResource)?;
    let lease=reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut out=Vec::new();out.try_reserve_exact(count).map_err(|_|PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<PalwClaimLocalNativeFrontierRecordV1>("native Rule E records","allocation refused")))?;
    for r in checkpoint.records.values(){if !matches!(r.work,PalwClaimLocalWorkRefV1::NativeMining39{..}){continue}
        out.push(PalwClaimLocalNativeFrontierRecordV1{work:r.work,accepting_block:r.accepting_block,accepted_blue_score:r.accepted_blue_score,accepted_daa:r.accepted_daa,
            irreversible:r.anchors&&validated.irreversible_eligible(r.work)});
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(out,lease))
}

/// Rebuild from actual current state and retain both material and graph demand
/// until the native settlement reader has finished/cache retirement occurs.
pub fn palw_claim_local_owned_checkpoint_v1(state:&PalwChainStateV2,params:&PalwStateParamsV2,point:&PalwBlockContextV2)
    ->Result<Option<crate::palw_claim_local_v1::PalwOwnedValidatedClaimCheckpointV1>,PalwStateV2Error>{
    if params.claim_local_settlement_active_at(point.daa_score){
        if state.last_point()!=Some(point){return Err(bad("native checkpoint consumer point differs from actual retained state"))}
        // A row/commitment is not the original admission material. The general
        // checkpoint may describe missing provenance as held; an EVM lifecycle
        // capability must refuse rather than assert that loss is preserved.
        for (id,claim) in &state.claims{
            if rewards::origin_for(state,params,*id,claim)?.is_none(){return Err(bad("native lifecycle checkpoint lost original admission material"))}
        }
    }
    expected_claim_local_checkpoint_v1(state,params,point)?.map(crate::palw_claim_local_v1::PalwOwnedValidatedClaimCheckpointV1::from_expected_native_v1)
        .transpose().map_err(checkpoint_err)
}

#[cfg(test)]
pub(super) fn prepared_expected_for_test_v3(state:&PalwChainStateV2,params:&PalwStateParamsV2,point:&PalwBlockContextV2)->Result<(NativeRetainedV1<PalwClaimLocalCheckpointV1>,super::palw_claim_local_checkpoint_descriptor_v3::PreparedBasesV3),PalwStateV2Error>{
 let mut bases=None;let cp=expected_with_base_observer_v3(state,params,point,&mut |records|{bases=Some(super::palw_claim_local_checkpoint_descriptor_v3::prepare_bases_v3(records)?);Ok(())})?.ok_or_else(||bad("test checkpoint fence dormant"))?;
 Ok((cp,bases.ok_or_else(||bad("test actual base observation missing"))?))
}

/// Typed local construction inventory, independent of consensus/credit. Includes
/// native contexts, maps, canonical material and duplicate/dedup peaks; transient
/// decode readers still retain their separately admitted leases.
pub fn palw_claim_local_checkpoint_construction_requests_v20(state:&PalwChainStateV2)->Result<u64,PalwStateV2Error>{
    resources::requests_compatibility_v24(state).map_err(PalwStateV2Error::NativeResource)?.checked_add(super::palw_claim_local_material_projection_v2::requests(state)?).ok_or(PalwStateV2Error::Overflow("scoped checkpoint construction"))
}
/// Exact native params/point quote for the normal constructor and local checker.
/// The older no-params API remains a conservative compatibility inventory.
pub fn palw_claim_local_checkpoint_construction_requests_v24(state:&PalwChainStateV2,params:&PalwStateParamsV2,point:&PalwBlockContextV2)->Result<u64,PalwStateV2Error>{
 resources::requests_targeted_v24(state,params,point).map_err(PalwStateV2Error::NativeResource)?.checked_add(super::palw_claim_local_material_projection_v2::requests(state)?).ok_or(PalwStateV2Error::Overflow("targeted scoped checkpoint construction"))
}
/// Additional caller-local logical capacity, around the SAME normal constructor.
/// Global logical/physical admission remains active; this never substitutes a
/// local checker for those checks. The returned ticket owns its reservation
/// until the opaque owner itself is dropped; the checkpoint cannot be extracted
/// separately from that reservation.
pub struct PalwLocallyAdmittedClaimCheckpointV20{
    checkpoint:Option<crate::palw_claim_local_v1::PalwOwnedValidatedClaimCheckpointV1>,
    _local_lease:crate::palw_native_resource_v1::NativeDemandLeaseV1,
}
impl PalwLocallyAdmittedClaimCheckpointV20{
    pub fn checkpoint(&self)->Option<&crate::palw_claim_local_v1::PalwOwnedValidatedClaimCheckpointV1>{self.checkpoint.as_ref()}
    pub fn allocation_footprint_v20(&self)->Result<Option<crate::palw_claim_local_v1::PalwCheckpointOwnedFootprintV20>,crate::palw_native_resource_v1::NativeCoverageV1>{self.checkpoint.as_ref().map(|c|c.allocation_footprint_v20()).transpose()}
}
pub fn palw_claim_local_owned_checkpoint_with_checker_v20(
    state:&PalwChainStateV2,params:&PalwStateParamsV2,point:&PalwBlockContextV2,
    checker:&std::sync::Arc<crate::palw_native_resource_v1::NativeDemandCheckerV1>,
)->Result<PalwLocallyAdmittedClaimCheckpointV20,PalwStateV2Error>{
    let requests=palw_claim_local_checkpoint_construction_requests_v24(state,params,point)?;
    let lease=checker.reserve_requests_v1(requests,0).map_err(PalwStateV2Error::NativeResource)?;
    let checkpoint=palw_claim_local_owned_checkpoint_v1(state,params,point)?;Ok(PalwLocallyAdmittedClaimCheckpointV20{checkpoint,_local_lease:lease})
}
