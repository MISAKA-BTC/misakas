//! Bounded v7 public node/hash/selected-value acquisition and fixed-row defaults.
use super::palw_flat_model_vector_fold_v3::{charge, enabled};
use super::palw_kernel_route_fold_v1::route_ledger_policy_v1;
use super::*;
use crate::palw_flat_model_vector_da_v7::*;
use crate::palw_flat_model_vector_v3::flat_model_vector_work_v3;
use crate::palw_model_vector_da_v2::MODEL_VECTOR_DEMAND_SLOTS_V2;
use crate::palw_onboarding_v1::PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1;
use misaka_palw_kernel::hash::Digest;
fn refused(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(s.into())
}
impl PalwChainStateV2 {
    pub fn flat_vector_demand_reserved_v7(&self, bond: &PalwBondKeyV2) -> u128 {
        self.kernel_route.as_ref().map(|r| r.flat_vector_demand_reserved_v7(bond)).unwrap_or(0)
    }
}
pub(super) fn demand(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    vector: &Digest,
    slot: u8,
    unit: FlatVectorUnitV7,
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    let id = Hash64::from_bytes(*vector);
    let route = builder.state.kernel_route.as_ref().unwrap();
    let h = route.flat_model_vector_header_v3(&id).ok_or_else(|| refused("no flat vector to demand"))?;
    let policy = route_ledger_policy_v1(builder)?;
    let deadline = ctx.daa_score.checked_add(policy.court_deadline_daa).ok_or_else(|| refused("flat deadline overflow"))?;
    let proof_until = deadline.checked_add(policy.proof_grace_daa).ok_or_else(|| refused("flat proof grace overflow"))?;
    if !route.flat_model_vector_protected_v7(&id)
        || h.refuted
        || route.flat_vector_unavailable_v7(&id)
        || proof_until > h.liability_until
        || slot >= MODEL_VECTOR_DEMAND_SLOTS_V2
        || route.flat_vector_open_demands_v7(&id) >= FLAT_VECTOR_OPEN_DEMANDS_MAX_V7
    {
        return Err(refused("flat demand outside horizon/slot/session bound"));
    }
    if builder.state.bonds.get(&h.poster).map(|b| b.operator_id) == builder.state.bonds.get(signer).map(|b| b.operator_id) {
        return Err(refused("flat poster operator cannot demand its own bounty"));
    }
    if route.flat_vector_slot_v9(signer, slot).and_then(|id| route.flat_vector_demand_v9(&id)).is_some_and(|d| !d.settled) {
        return Err(refused("shared new requester slot held"));
    }
    let prior_slot = route.flat_vector_slot_v7(signer, slot);
    if route.flat_vector_slot_v3(signer, slot).and_then(|d| route.flat_vector_demand_v3(&d)).is_some_and(|d| !d.settled) {
        return Err(refused("shared old/new flat requester slot held"));
    }
    if route.flat_vector_served_v7(&id, &unit).is_some()
        || prior_slot.and_then(|d| route.flat_vector_demand_v7(&d)).is_some_and(|d| !d.settled)
        || route.model_vector_demand_slot_v2(signer, slot).and_then(|d| route.model_vector_demand_v2(&d)).is_some_and(|d| !d.settled)
    {
        return Err(refused("flat public unit or shared requester slot already held"));
    }
    let work = flat_model_vector_work_v3(h.program_bytes_len, h.pc_instances, h.post_bytes_len as usize)
        .ok_or_else(|| refused("flat demand work overflow"))?;
    let Some(fee) = charge(builder, ctx, signer, work, false)? else { return Ok(()) };
    builder.slash_bond(*signer, fee)?;
    let checked = (|| -> Result<_, String> {
        let view = builder.state.kernel_route.as_ref().unwrap().flat_model_vector_view_v7(&id)?;
        if unit.position >= view.trace.positions {
            return Err("flat position not committed".into());
        }
        let w = misaka_palw_kernel::trace::WiringV1::new(&view.trace.program).map_err(|e| e.to_string())?;
        let (block, _) = w.occurrences.get(unit.occurrence as usize).ok_or("flat occurrence not committed")?;
        let node = view.trace.program.blocks[*block as usize].nodes.get(unit.node as usize).ok_or("flat node not committed")?;
        let shape = node.out.resolve(w.h(unit.occurrence, unit.position));
        if let FlatVectorUnitKindV7::Hash(index) = unit.kind {
            let shape: Vec<_> = shape.iter().map(|d| *d as u64).collect();
            if misaka_palw_kernel::merkle5::FlatLayoutV5::of(&shape).and_then(|l| l.span(index)).is_none() {
                return Err("flat hash leaf outside declared large-node layout".into());
            }
        }
        if let FlatVectorUnitKindV7::Value(index) = unit.kind
            && (!matches!(node.prim, misaka_palw_tir::Prim::TopK { .. })
                || node.out.dtype != misaka_palw_tir::DType::Idx
                || misaka_palw_kernel::merkle3::LayoutV3::of(&shape).len <= index)
        {
            return Err("selected value is not in the admitted TopK output".into());
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
    let answer_work = crate::palw_model_artifact_v2::model_artifact_work_v2(0, FLAT_VECTOR_ANSWER_MAX_BYTES_V7).unwrap();
    let bond = policy.demand_bond.max(policy.dismissal_fee_v1(answer_work));
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < bond as u128 {
        return Ok(());
    }
    let row = FlatVectorDemandV7 {
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
    let count = builder.state.kernel_route.as_ref().unwrap().flat_vector_open_demands_v7(&id) + 1;
    if let Some(prior) = prior_slot {
        builder.write_kernel_row(FLAT_VECTOR_DEMAND_TABLE_V7, borsh::to_vec(&prior).unwrap(), None)?;
    }
    builder.write_kernel_row(FLAT_VECTOR_DEMAND_TABLE_V7, borsh::to_vec(&demand).unwrap(), Some(borsh::to_vec(&row).unwrap()))?;
    builder.write_kernel_row(
        FLAT_VECTOR_SLOT_TABLE_V7,
        borsh::to_vec(&(*signer, slot)).unwrap(),
        Some(borsh::to_vec(&demand).unwrap()),
    )?;
    builder.write_kernel_row(FLAT_VECTOR_OPEN_COUNT_TABLE_V7, borsh::to_vec(&id).unwrap(), Some(borsh::to_vec(&count).unwrap()))?;
    Ok(())
}
fn close_demand(builder: &mut TransitionBuilder<'_>, id: &Hash64, mut d: FlatVectorDemandV7) -> Result<(), PalwStateV2Error> {
    let count = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .flat_vector_open_demands_v7(&d.vector)
        .checked_sub(1)
        .ok_or_else(|| refused("flat demand count underflow"))?;
    d.settled = true;
    builder.write_kernel_row(FLAT_VECTOR_DEMAND_TABLE_V7, borsh::to_vec(id).unwrap(), Some(borsh::to_vec(&d).unwrap()))?;
    builder.write_kernel_row(
        FLAT_VECTOR_OPEN_COUNT_TABLE_V7,
        borsh::to_vec(&d.vector).unwrap(),
        Some(borsh::to_vec(&count).unwrap()),
    )?;
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
    let d = route.flat_vector_demand_v7(&id).ok_or_else(|| refused("no flat demand"))?;
    let h = route.flat_model_vector_header_v3(&d.vector).ok_or_else(|| refused("no flat vector"))?;
    if d.settled || h.refuted || route.flat_vector_unavailable_v7(&d.vector) || ctx.daa_score > d.deadline_daa {
        return Err(refused("flat answer closed or late"));
    }
    if route.flat_vector_served_v7(&d.vector, &d.unit).is_some() {
        return settle_demand(builder, ctx, signer, demand);
    }
    // Price bounded selected bytes and paths against the shared admission budget
    // before decoding them. Authentication uses one fixed request and decodes no
    // stored program, parameter commitments or variable trace statement.
    let work = crate::palw_model_artifact_v2::model_artifact_work_v2(0, bytes.len().min(FLAT_VECTOR_ANSWER_MAX_BYTES_V7)).unwrap();
    let Some(fee) = charge(builder, ctx, signer, work, false)? else { return Ok(()) };
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < fee {
        return Err(refused("flat answer actor cannot cover invalid fee"));
    }
    if !decode_flat_vector_answer_v7(bytes).is_ok_and(|a| a.authenticates(&d)) {
        builder.slash_bond(*signer, fee)?;
        return Ok(());
    }
    let row = FlatVectorServedRowV7 {
        header: FlatVectorServedHeaderV7 { answered_daa: ctx.daa_score, answer_root: flat_vector_answer_root_v7(bytes) },
        answer: bytes.to_vec(),
    };
    builder.write_kernel_row(FLAT_VECTOR_SERVED_TABLE_V7, d.unit.key(&d.vector), Some(borsh::to_vec(&row).unwrap()))?;
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
    let d = route.flat_vector_demand_v7(&id).ok_or_else(|| refused("no flat demand to close"))?;
    if d.settled {
        return Err(refused("flat demand already closed"));
    }
    let h = route.flat_model_vector_header_v3(&d.vector).ok_or_else(|| refused("no flat vector to close"))?;
    if h.refuted || route.flat_vector_unavailable_v7(&d.vector) {
        return close_demand(builder, &id, d);
    }
    if route.flat_vector_served_v7(&d.vector, &d.unit).is_some_and(|s| s.answered_daa <= d.deadline_daa) {
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
    builder.write_kernel_row(
        FLAT_VECTOR_UNAVAILABLE_TABLE_V7,
        borsh::to_vec(&d.vector).unwrap(),
        Some(borsh::to_vec(&true).unwrap()),
    )?;
    close_demand(builder, &id, d)
}
