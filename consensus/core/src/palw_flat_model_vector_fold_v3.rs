//! Consumer-owned admission and court folds for explicit flat-v5 candidate traces.
use super::palw_kernel_route_fold_v1::{charge_route_budget_v1, ensure_route_header, route_ledger_policy_v1};
use super::*;
use crate::palw_flat_model_vector_da_v3::*;
use crate::palw_flat_model_vector_v3::*;
use crate::palw_model_vector_da_v2::MODEL_VECTOR_DEMAND_SLOTS_V2;
use crate::palw_model_vector_v2::MODEL_VECTORS_PER_BOND_V2;
use crate::palw_onboarding_v1::{
    ArtifactBindingStateV1, PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1, PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
    PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1,
};
use misaka_palw_kernel::hash::Digest;
fn refused(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(s.into())
}
pub(super) fn enabled(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    if !builder.extras.kernel_route.as_ref().is_some_and(|r| r.opv.is_some()) {
        return Err(refused("flat vectors require palw_panel_free_v1"));
    }
    Ok(())
}
pub(super) fn charge(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    work: u64,
    court: bool,
) -> Result<Option<u128>, PalwStateV2Error> {
    let fee = route_ledger_policy_v1(builder)?.dismissal_fee_v1(work) as u128;
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < fee {
        return Err(refused("flat vector actor cannot cover its dismissal fee"));
    }
    Ok(charge_route_budget_v1(builder, ctx, work, court)?.then_some(fee))
}
impl PalwChainStateV2 {
    pub fn flat_vector_demand_reserved_v3(&self, bond: &PalwBondKeyV2) -> u128 {
        self.kernel_route.as_ref().map(|r| r.flat_vector_demand_reserved_v3(bond)).unwrap_or(0)
    }
    pub fn flat_model_vector_reserved_at_v3(&self, bond: &PalwBondKeyV2, daa: u64) -> u128 {
        self.kernel_route.as_ref().map(|r| r.flat_model_vector_reserved_at_v3(bond, daa)).unwrap_or(0)
    }
}
pub(super) fn post(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    class: &Digest,
    binding: &Digest,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    post_inner(builder, ctx, signer, class, binding, bytes, false)
}
pub(super) fn post_protected(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    class: &Digest,
    binding: &Digest,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    post_inner(builder, ctx, signer, class, binding, bytes, true)
}
fn post_inner(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    class: &Digest,
    binding: &Digest,
    bytes: &[u8],
    protected: bool,
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    let (class, binding) = (Hash64::from_bytes(*class), Hash64::from_bytes(*binding));
    let route = builder.state.kernel_route.as_ref().unwrap();
    let mut ids = route.flat_model_vectors_of_v3(signer);
    if ids.len() + route.model_vectors_of_v2(signer).len() >= MODEL_VECTORS_PER_BOND_V2
        || bytes.len() > FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3
    {
        return Err(refused("shared vector catalog/carrier full"));
    }
    let statement = route.model_artifact_binding_header_v2(&binding).ok_or_else(|| refused("flat vector has no scoped model"))?;
    if !matches!(statement.state_at(ctx.daa_score), ArtifactBindingStateV1::Matured | ArtifactBindingStateV1::Final)
        || route.model_artifact_candidate_binding_v2(&class).is_none()
        || route.template().schedule.standing_at(&statement.descriptor, ctx.daa_score)
            != misaka_palw_kernel::descriptor::KernelStandingV1::Active
    {
        return Err(refused("flat vector model/candidate does not stand"));
    }
    let work = flat_model_vector_work_v3(statement.program_bytes_len, statement.pc_instances, bytes.len())
        .ok_or_else(|| refused("flat vector work overflow"))?;
    let Some(fee) = charge(builder, ctx, signer, work, false)? else { return Ok(()) };
    builder.slash_bond(*signer, fee)?;
    let checked = (|| -> Result<_, String> {
        let route = builder.state.kernel_route.as_ref().unwrap();
        let (post, root) = if protected {
            let p = crate::palw_flat_model_seal_v6::decode_flat_model_vector_post_v6(bytes).map_err(|e| e.to_string())?;
            let root = p.trace_root(class.as_bytes(), binding.as_bytes());
            (p.body, root)
        } else {
            let p = decode_flat_model_vector_post_v3(bytes).map_err(|e| e.to_string())?;
            let root = p.trace_root(class.as_bytes(), binding.as_bytes());
            (p, root)
        };
        let view = checked_flat_model_vector_with_root(route, class, binding, &post, root)?;
        if view.trace.params.by_instance.len() != statement.pc_instances as usize {
            return Err("flat PC count changed".into());
        }
        let worst = flat_model_vector_work_v3(
            statement.program_bytes_len,
            statement.pc_instances,
            bytes.len().checked_add(view.scope.worst_court_bytes as usize).ok_or("flat carrier overflow")?,
        )
        .and_then(|w| w.checked_add(view.scope.worst_court_work))
        .and_then(|w| {
            if protected {
                super::palw_flat_model_seal_fold_v6::metadata_work(view.scope.worst_court_bytes as usize)
                    .and_then(|m| w.checked_add(m))
            } else {
                Some(w)
            }
        })
        .ok_or("flat worst court overflow")?;
        if worst > route_ledger_policy_v1(builder).map_err(|e| e.to_string())?.guaranteed_proof_work_v1()
            || view.scope.worst_court_bytes > FLAT_MODEL_VECTOR_MAX_PROOF_BYTES_V3 as u64
        {
            return Err("flat court exceeds shared guaranteed budget/carrier".into());
        }
        Ok((post, root, view.scope))
    })();
    let Ok((post, root, scope)) = checked else { return Ok(()) };
    let id = vector_id(root);
    if builder.state.kernel_route.as_ref().unwrap().flat_model_vector_header_v3(&id).is_some() {
        return Ok(());
    }
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1 as u128 {
        return Ok(());
    }
    let h = FlatModelVectorHeaderV3 {
        trace_root: root,
        class,
        binding,
        profile: post.profile,
        scope_root: post.scope_root,
        program_bytes_len: statement.program_bytes_len,
        pc_instances: statement.pc_instances,
        post_bytes_len: bytes.len() as u32,
        max_filing_bytes: scope.worst_court_bytes,
        max_court_work: scope.worst_court_work,
        max_court_scratch_bytes: scope.worst_court_scratch_bytes,
        poster: *signer,
        posted_daa: ctx.daa_score,
        liability_until: ctx
            .daa_score
            .checked_add(PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1)
            .ok_or_else(|| refused("flat horizon overflow"))?,
        reserved: PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
        refuted: false,
    };
    ids.push(id);
    builder.write_kernel_row(
        FLAT_MODEL_VECTOR_TABLE_V3,
        borsh::to_vec(&id).unwrap(),
        Some(borsh::to_vec(&FlatModelVectorRowV3 { header: h, post: bytes.to_vec() }).unwrap()),
    );
    builder.write_kernel_row(FLAT_MODEL_VECTOR_BOND_TABLE_V3, borsh::to_vec(signer).unwrap(), Some(borsh::to_vec(&ids).unwrap()));
    if protected {
        builder.write_kernel_row(
            crate::palw_flat_model_seal_v6::FLAT_PROTECTED_TABLE_V6,
            borsh::to_vec(&id).unwrap(),
            Some(borsh::to_vec(&true).unwrap()),
        );
    }
    Ok(())
}
pub(super) fn refute(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    vector: &Digest,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    let id = Hash64::from_bytes(*vector);
    if builder.state.kernel_route.as_ref().unwrap().flat_model_vector_protected_v6(&id) {
        return Err(refused("protected flat vector requires its actor's mature seal/reveal"));
    }
    let h = builder.state.kernel_route.as_ref().unwrap().flat_model_vector_header_v3(&id).ok_or_else(|| refused("no flat vector"))?;
    if h.refuted || builder.state.kernel_route.as_ref().unwrap().flat_vector_unavailable_v3(&id) || ctx.daa_score >= h.liability_until
    {
        return Err(refused("flat vector no longer refutable"));
    }
    let challenger = builder.state.bonds.get(signer).unwrap();
    if builder.state.bonds.get(&h.poster).is_some_and(|b| b.operator_id == challenger.operator_id) {
        return Err(refused("flat poster operator cannot collect its bounty"));
    }
    let payout = challenger.payout_payload;
    let work = flat_model_vector_work_v3(h.program_bytes_len, h.pc_instances, bytes.len().saturating_add(h.post_bytes_len as usize))
        .and_then(|w| w.checked_add(h.max_court_work))
        .ok_or_else(|| refused("flat court work overflow"))?;
    let Some(fee) = charge(builder, ctx, signer, work, true)? else { return Ok(()) };
    let proven = (|| -> Result<(), String> {
        if bytes.len() as u64 > h.max_filing_bytes {
            return Err("flat filing beyond stored admission".into());
        }
        let fault = misaka_palw_kernel::flat_court5::FlatFaultV5::from_bytes(bytes).map_err(|e| e.to_string())?;
        let v = builder.state.kernel_route.as_ref().unwrap().flat_model_vector_view_v3(&id)?;
        let b = misaka_palw_kernel::flat_court5::flat_fault_court_bounds_v5(&v.context(), &fault).map_err(|e| format!("{e:?}"))?;
        if b.work > h.max_court_work || b.filing_bytes > h.max_filing_bytes || b.scratch_bytes > h.max_court_scratch_bytes {
            return Err("flat exact court outside persisted vector".into());
        }
        misaka_palw_kernel::flat_court5::verify_flat_fault_v5(&v.context(), &fault).map(|_| ()).map_err(|e| format!("{e:?}"))
    })()
    .is_ok();
    if !proven {
        builder.slash_bond(*signer, fee)?;
        return Ok(());
    }
    let mut row = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .flat_model_vector_row_v3(&id)
        .ok_or_else(|| refused("flat vector disappeared"))?;
    let slash = builder.slash_bond(h.poster, h.reserved as u128)?;
    builder.add_kernel_payout(payout, slash.saturating_mul(PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1) / 1000)?;
    row.header.refuted = true;
    row.header.reserved = 0;
    builder.write_kernel_row(FLAT_MODEL_VECTOR_TABLE_V3, borsh::to_vec(&id).unwrap(), Some(borsh::to_vec(&row).unwrap()));
    Ok(())
}

pub(super) fn demand(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    vector: &Digest,
    slot: u8,
    unit: FlatVectorUnitV3,
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    let id = Hash64::from_bytes(*vector);
    let route = builder.state.kernel_route.as_ref().unwrap();
    let h = route.flat_model_vector_header_v3(&id).ok_or_else(|| refused("no flat vector to demand"))?;
    let policy = route_ledger_policy_v1(builder)?;
    let deadline = ctx.daa_score.checked_add(policy.court_deadline_daa).ok_or_else(|| refused("flat deadline overflow"))?;
    let proof_until = deadline.checked_add(policy.proof_grace_daa).ok_or_else(|| refused("flat proof grace overflow"))?;
    if h.refuted
        || route.flat_vector_unavailable_v3(&id)
        || proof_until > h.liability_until
        || slot >= MODEL_VECTOR_DEMAND_SLOTS_V2
        || route.flat_vector_open_demands_v3(&id) >= FLAT_VECTOR_OPEN_DEMANDS_MAX_V3
    {
        return Err(refused("flat demand outside horizon/slot/session bound"));
    }
    if builder.state.bonds.get(&h.poster).map(|b| b.operator_id) == builder.state.bonds.get(signer).map(|b| b.operator_id) {
        return Err(refused("flat poster operator cannot demand its own bounty"));
    }
    let prior_slot = route.flat_vector_slot_v3(signer, slot);
    if route.flat_vector_served_v3(&id, &unit).is_some()
        || prior_slot.and_then(|d| route.flat_vector_demand_v3(&d)).is_some_and(|d| !d.settled)
        || route.model_vector_demand_slot_v2(signer, slot).and_then(|d| route.model_vector_demand_v2(&d)).is_some_and(|d| !d.settled)
    {
        return Err(refused("flat public unit or shared requester slot already held"));
    }
    let work = flat_model_vector_work_v3(h.program_bytes_len, h.pc_instances, h.post_bytes_len as usize)
        .ok_or_else(|| refused("flat demand work overflow"))?;
    let Some(fee) = charge(builder, ctx, signer, work, false)? else { return Ok(()) };
    builder.slash_bond(*signer, fee)?;
    let checked = (|| -> Result<_, String> {
        let view = builder.state.kernel_route.as_ref().unwrap().flat_model_vector_view_v3(&id)?;
        if unit.position >= view.trace.positions {
            return Err("flat position not committed".into());
        }
        let w = misaka_palw_kernel::trace::WiringV1::new(&view.trace.program).map_err(|e| e.to_string())?;
        let (block, _) = w.occurrences.get(unit.occurrence as usize).ok_or("flat occurrence not committed")?;
        let node = view.trace.program.blocks[*block as usize].nodes.get(unit.node as usize).ok_or("flat node not committed")?;
        let shape = node.out.resolve(w.h(unit.occurrence, unit.position));
        if let Some(index) = unit.leaf {
            let shape: Vec<_> = shape.iter().map(|d| *d as u64).collect();
            if misaka_palw_kernel::merkle5::FlatLayoutV5::of(&shape).and_then(|l| l.span(index)).is_none() {
                return Err("flat hash leaf outside declared large-node layout".into());
            }
        }
        if shape.len() > 4 {
            return Err("flat shape rank".into());
        }
        let mut fixed_shape = [0u64; 4];
        for (i, d) in shape.iter().enumerate() {
            fixed_shape[i] = *d as u64;
        }
        let mut count = 0u64;
        let mut index = 0u64;
        for (s, (b, _)) in w.occurrences.iter().enumerate() {
            if s == unit.occurrence as usize {
                index = count + unit.node as u64;
            }
            count = count.checked_add(view.trace.program.blocks[*b as usize].nodes.len() as u64).ok_or("flat node count overflow")?;
        }
        let root = view.trace.segment_roots[(unit.position / misaka_palw_kernel::seg::SEG_LEN_V4) as usize];
        Ok((index, count, view.trace.positions, root, node.out.dtype.tag(), shape.len() as u8, fixed_shape))
    })();
    let Ok((node_index, node_count, positions, segment_root, dtype, rank, shape)) = checked else { return Ok(()) };
    let answer_work = crate::palw_model_artifact_v2::model_artifact_work_v2(0, FLAT_VECTOR_ANSWER_MAX_BYTES_V3).unwrap();
    let bond = policy.demand_bond.max(policy.dismissal_fee_v1(answer_work));
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < bond as u128 {
        return Ok(());
    }
    let row = FlatVectorDemandV3 {
        vector: id,
        unit,
        requester: *signer,
        requester_payout: builder.state.bonds.get(signer).unwrap().payout_payload,
        poster: h.poster,
        poster_payout: builder.state.bonds.get(&h.poster).unwrap().payout_payload,
        slot,
        filed_daa: ctx.daa_score,
        deadline_daa: deadline,
        proof_until,
        bond,
        node_index,
        node_count,
        positions,
        segment_root,
        dtype,
        rank,
        shape,
        settled: false,
    };
    let demand = row.id();
    let count = builder.state.kernel_route.as_ref().unwrap().flat_vector_open_demands_v3(&id) + 1;
    if let Some(prior) = prior_slot {
        builder.write_kernel_row(FLAT_VECTOR_DEMAND_TABLE_V3, borsh::to_vec(&prior).unwrap(), None);
    }
    builder.write_kernel_row(FLAT_VECTOR_DEMAND_TABLE_V3, borsh::to_vec(&demand).unwrap(), Some(borsh::to_vec(&row).unwrap()));
    builder.write_kernel_row(
        FLAT_VECTOR_SLOT_TABLE_V3,
        borsh::to_vec(&(*signer, slot)).unwrap(),
        Some(borsh::to_vec(&demand).unwrap()),
    );
    builder.write_kernel_row(FLAT_VECTOR_OPEN_COUNT_TABLE_V3, borsh::to_vec(&id).unwrap(), Some(borsh::to_vec(&count).unwrap()));
    Ok(())
}
fn close_demand(builder: &mut TransitionBuilder<'_>, id: &Hash64, mut d: FlatVectorDemandV3) -> Result<(), PalwStateV2Error> {
    let count = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .flat_vector_open_demands_v3(&d.vector)
        .checked_sub(1)
        .ok_or_else(|| refused("flat demand count underflow"))?;
    d.settled = true;
    builder.write_kernel_row(FLAT_VECTOR_DEMAND_TABLE_V3, borsh::to_vec(id).unwrap(), Some(borsh::to_vec(&d).unwrap()));
    builder.write_kernel_row(FLAT_VECTOR_OPEN_COUNT_TABLE_V3, borsh::to_vec(&d.vector).unwrap(), Some(borsh::to_vec(&count).unwrap()));
    Ok(())
}
pub(super) fn answer(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    demand: &Digest,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    let id = Hash64::from_bytes(*demand);
    let route = builder.state.kernel_route.as_ref().unwrap();
    let d = route.flat_vector_demand_v3(&id).ok_or_else(|| refused("no flat demand"))?;
    let h = route.flat_model_vector_header_v3(&d.vector).ok_or_else(|| refused("no flat vector"))?;
    if d.settled || h.refuted || route.flat_vector_unavailable_v3(&d.vector) || ctx.daa_score > d.deadline_daa {
        return Err(refused("flat answer closed or late"));
    }
    if route.flat_vector_served_v3(&d.vector, &d.unit).is_some() {
        return settle_demand(builder, ctx, signer, demand);
    }
    // Like the existing bounded response lane, this uses no adjudication run.
    // It decodes no program, PC, trace or values: one fixed request, capped paths
    // and hash-only material, bounded by the existing per-block payload carriage.
    let work = crate::palw_model_artifact_v2::model_artifact_work_v2(0, bytes.len().min(FLAT_VECTOR_ANSWER_MAX_BYTES_V3)).unwrap();
    let fee = route_ledger_policy_v1(builder)?.dismissal_fee_v1(work) as u128;
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < fee {
        return Err(refused("flat answer actor cannot cover invalid fee"));
    }
    if !decode_flat_vector_answer_v3(bytes).is_ok_and(|a| a.authenticates(&d)) {
        builder.slash_bond(*signer, fee)?;
        return Ok(());
    }
    let row = FlatVectorServedRowV3 {
        header: FlatVectorServedHeaderV3 { answered_daa: ctx.daa_score, answer_root: flat_vector_answer_root_v3(bytes) },
        answer: bytes.to_vec(),
    };
    builder.write_kernel_row(FLAT_VECTOR_SERVED_TABLE_V3, d.unit.key(&d.vector), Some(borsh::to_vec(&row).unwrap()));
    let paid = builder.slash_bond(d.requester, d.bond as u128)?;
    builder.add_kernel_payout(d.poster_payout, paid)?;
    close_demand(builder, &id, d)
}
pub(super) fn settle_demand(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    demand: &Digest,
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    let id = Hash64::from_bytes(*demand);
    let route = builder.state.kernel_route.as_ref().unwrap();
    let d = route.flat_vector_demand_v3(&id).ok_or_else(|| refused("no flat demand to close"))?;
    if d.settled {
        return Err(refused("flat demand already closed"));
    }
    let h = route.flat_model_vector_header_v3(&d.vector).ok_or_else(|| refused("no flat vector to close"))?;
    if h.refuted || route.flat_vector_unavailable_v3(&d.vector) {
        return close_demand(builder, &id, d);
    }
    if route.flat_vector_served_v3(&d.vector, &d.unit).is_some_and(|s| s.answered_daa <= d.deadline_daa) {
        let paid = builder.slash_bond(d.requester, d.bond as u128)?;
        builder.add_kernel_payout(d.poster_payout, paid)?;
        return close_demand(builder, &id, d);
    }
    if ctx.daa_score <= d.deadline_daa {
        return Err(refused("flat response deadline still open"));
    }
    // Fixed-row default never decodes a stored variable statement. It cannot
    // bypass the shared court count/work reserve or steal the requester's bounty.
    let work = crate::palw_model_artifact_v2::model_artifact_work_v2(0, 1024).unwrap();
    let Some(_fee) = charge(builder, ctx, signer, work, true)? else { return Ok(()) };
    let slash = builder.slash_bond(h.poster, h.reserved as u128)?;
    builder.add_kernel_payout(d.requester_payout, slash.saturating_mul(PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1) / 1000)?;
    builder.write_kernel_row(FLAT_VECTOR_UNAVAILABLE_TABLE_V3, borsh::to_vec(&d.vector).unwrap(), Some(borsh::to_vec(&true).unwrap()));
    close_demand(builder, &id, d)
}
