//! Fixed native typed-node/value authentication and original trace-DA defaults.
use super::*;
use crate::palw_flat_model_vector_da_v9::{
    FLAT_VECTOR_ANSWER_MAX_BYTES_V9, FlatVectorDemandV9, FlatVectorServedHeaderV9, FlatVectorServedRowV9, FlatVectorUnitKindV9,
    FlatVectorUnitV9, decode_flat_vector_answer_v9, flat_vector_answer_root_v9,
};
fn slot_key(signer: &PalwBondKeyV2, slot: u8) -> Vec<u8> {
    key(&(*signer, slot))
}
fn stored_slot(r: &PalwKernelRouteStateV1, signer: &PalwBondKeyV2, slot: u8) -> Option<Hash64> {
    r.aux_row(EXECUTION_DEMAND_SLOT_TABLE_V11, &slot_key(signer, slot))
}
pub(super) fn counters(r: &PalwKernelRouteStateV1, id: Digest) -> Result<NativeExecutionDemandCountersV13, PalwStateV2Error> {
    let Some(raw) = r.aux.get(&(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&(13u8, id)))) else {
        return Ok(NativeExecutionDemandCountersV13::default());
    };
    if raw.len() != 12 {
        return Err(refused("native DA counters fixed framing"));
    }
    let c: NativeExecutionDemandCountersV13 = borsh::from_slice(raw).map_err(|_| refused("native DA counters decode"))?;
    if c.open_ordinary > NATIVE_EXECUTION_OPEN_ORDINARY_V13 || c.collected_ordinal > c.next_ordinal {
        return Err(refused("native DA counters monotone/open bounds"));
    }
    Ok(c)
}
pub(super) fn demand(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    if bytes.len() != 147 {
        return Err(refused("native demand fixed request framing"));
    }
    let request: NativeExecutionDemandRequestV11 = borsh::from_slice(bytes).map_err(|_| refused("native demand decode"))?;
    if request.slot >= 2 || request.lane > 2 {
        return Err(refused("native demand lane/slot"));
    }
    let unit = FlatVectorUnitV9 {
        position: request.position,
        occurrence: request.occurrence,
        node: request.node,
        kind: match request.kind {
            0 if request.index == 0 => FlatVectorUnitKindV9::Node,
            1 => FlatVectorUnitKindV9::Hash(request.index),
            2 => FlatVectorUnitKindV9::Value(request.index),
            3 if request.index == 0 => FlatVectorUnitKindV9::Header,
            _ => return Err(refused("native typed demand kind/index")),
        },
    };
    let id = request.job_id;
    let actor = palw_kernel_bond_id_v1(signer);
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = r.execution_job_header_v11(&id).ok_or_else(|| refused("native demanded job absent"))?;
    let requester = builder.state.bonds.get(signer).unwrap();
    if requester.operator_id.as_bytes() == h.producer_operator {
        return Err(refused("native producer operator cannot demand its bounty"));
    }
    if stored_slot(r, signer, request.slot).and_then(|id| r.execution_demand_v11(&id)).is_some_and(|d| !d.body.settled) {
        return Err(refused("native original actor slot occupied"));
    }
    if source::slot_occupied(r, signer, request.slot)? {
        return Err(refused("native source/trace shared requester slot occupied"));
    }
    // The same small native actors also cannot double-borrow a legacy requester
    // slot. Paid discovery entitlements retain their own capacity after normal
    // job request exhaustion, but never another actor's live original slot.
    if r.flat_vector_slot_v9(signer, request.slot).and_then(|id| r.flat_vector_demand_v9(&id)).is_some_and(|d| !d.settled)
        || r.flat_vector_slot_v7(signer, request.slot).and_then(|id| r.flat_vector_demand_v7(&id)).is_some_and(|d| !d.settled)
        || r.flat_vector_slot_v3(signer, request.slot).and_then(|id| r.flat_vector_demand_v3(&id)).is_some_and(|d| !d.settled)
        || r.model_vector_demand_slot_v2(signer, request.slot).and_then(|id| r.model_vector_demand_v2(&id)).is_some_and(|d| !d.settled)
    {
        return Err(refused("native shared requester slot occupied"));
    }
    if request.lane == 0 && request.entitlement != [0; 64] {
        return Err(refused("native ordinary request has no paid entitlement"));
    }
    if request.lane == 0 && !prepay(builder, ctx, signer, class_work(builder, h.class_id, bytes.len())?)? {
        return Ok(());
    }
    if h.terminal != ExecutionTerminalV11::Challengeable || ctx.daa_score > h.deadline_daa {
        return Err(refused("native DA original header is closed"));
    }
    if request.lane == 1 {
        let seal = court::seal_row(builder.state.kernel_route.as_ref().unwrap(), &request.entitlement)?;
        if seal.actor != actor || seal.job_id != id || seal.spent || ctx.daa_score < seal.mature_daa {
            return Err(refused("native protected DA has no mature original own proof"));
        }
    } else if request.lane == 2 {
        discovery::check_request_owner(builder, ctx, actor, id, request.entitlement)?;
    }
    // Supplement survives a subsequent malformed coordinate/body. It pays the
    // original class/post restoration before the frozen small DARequest token.
    debit_trace_request(builder, ctx, p, actor, id, request.lane, request.entitlement)?;
    let (_, class, book, plan) = original_job(builder, id)?;
    let claim = book.claim().ok_or_else(|| refused("native demanded job has no original claim"))?;
    let mut state = pursuit(builder.state.kernel_route.as_ref().unwrap(), &id)?;
    let mut counts = counters(builder.state.kernel_route.as_ref().unwrap(), id)?;
    let deadline =
        ctx.daa_score.checked_add(class.record().horizons.da_answer).ok_or_else(|| refused("native answer deadline overflow"))?;
    let proof_until =
        deadline.checked_add(class.record().horizons.proof_grace).ok_or_else(|| refused("native answer grace overflow"))?;
    if book.terminal() != ExecutionTerminalV11::Challengeable
        || ctx.daa_score > claim.challenge_end
        || proof_until > claim.retention_until
        || state.open_demands >= NATIVE_EXECUTION_OPEN_TOTAL_V13
        || counts.next_ordinal == u32::MAX
        || request.lane == 0
            && (state.ordinary_requests >= p.discovery.ordinary_lifetime_requests
                || counts.open_ordinary >= NATIVE_EXECUTION_OPEN_ORDINARY_V13)
    {
        return Err(refused("native original DA window/lifetime/open bound"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    match request.lane {
        0 => {
            let mut shard = quota_shard(builder, p, &epoch, actor, id)?;
            let binding = book.job().quota_subject(class.record()).map_err(refused)?;
            let action = plan.action(QuotaActionKindV11::DARequest, bytes).map_err(refused)?;
            shard.debit_public_action(&p.quota, &epoch, &binding, &plan, &action).map_err(refused)?;
            write_shard(builder, &shard, id);
        }
        1 => {
            let seal = court::seal_row(builder.state.kernel_route.as_ref().unwrap(), &request.entitlement)?;
            if seal.actor != actor || seal.job_id != id || seal.spent || ctx.daa_score < seal.mature_daa {
                return Err(refused("native protected DA has no mature original own proof"));
            }
            let mut shard = quota_shard_seal(builder, p, &epoch, actor, id, Some(request.entitlement))?;
            shard.debit_protected(&p.quota, &epoch, &plan, ProtectedActionKindV11::DARequest, bytes).map_err(refused)?;
            write_shard(builder, &shard, id);
        }
        2 => {
            discovery::debit_request(builder, ctx, p, actor, id, request.entitlement, &plan, bytes)?;
        }
        _ => return Err(refused("native paid discovery lane")),
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let served_key = unit.key(&Hash64::from_bytes(claim.vector));
    if r.aux.contains_key(&(EXECUTION_SERVED_TABLE_V11, served_key)) {
        return Err(refused("native public selected unit already served"));
    }
    let view = r.execution_view_v11(&id).map_err(refused)?;
    if unit.position >= view.trace.positions {
        return Err(refused("native unit position beyond funded execution"));
    }
    let wiring = misaka_palw_kernel::trace::WiringV1::new(&view.trace.program).map_err(|e| refused(e.to_string()))?;
    let (block, _) = wiring.occurrences.get(unit.occurrence as usize).ok_or_else(|| refused("native unit occurrence"))?;
    let node = view.trace.program.blocks[*block as usize].nodes.get(unit.node as usize).ok_or_else(|| refused("native unit node"))?;
    let shape = node.out.resolve(wiring.h(unit.occurrence, unit.position));
    if shape.len() > 4 {
        return Err(refused("native typed rank"));
    }
    if let FlatVectorUnitKindV9::Hash(i) = unit.kind {
        let dims: Vec<_> = shape.iter().map(|d| *d as u64).collect();
        if misaka_palw_kernel::merkle5::FlatLayoutV5::of(&dims).and_then(|l| l.span(i)).is_none() {
            return Err(refused("native hash unit outside original layout"));
        }
    }
    if let FlatVectorUnitKindV9::Value(i) = unit.kind
        && misaka_palw_kernel::merkle3::LayoutV3::of(&shape).len <= i
    {
        return Err(refused("native selected value outside original shape"));
    }
    let mut fixed_shape = [0; 4];
    for (i, d) in shape.iter().enumerate() {
        fixed_shape[i] = *d as u64;
    }
    let mut node_count = 0u64;
    let mut node_index = 0u64;
    for (i, (block, _)) in wiring.occurrences.iter().enumerate() {
        if i == unit.occurrence as usize {
            node_index = node_count + unit.node as u64;
        }
        node_count = node_count
            .checked_add(view.trace.program.blocks[*block as usize].nodes.len() as u64)
            .ok_or_else(|| refused("native node count overflow"))?;
    }
    let policy = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    let answer_work = crate::palw_model_artifact_v2::model_artifact_work_v2(0, FLAT_VECTOR_ANSWER_MAX_BYTES_V9).unwrap();
    let bond = policy.demand_bond.max(policy.dismissal_fee_v1(answer_work));
    let requester = builder.state.bonds.get(signer).unwrap();
    if (requester.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < bond as u128 {
        return Err(refused("native original DA requester bond unavailable"));
    }
    let d = NativeExecutionDemandV11 {
        job_id: id,
        lane: request.lane,
        entitlement: request.entitlement,
        body: FlatVectorDemandV9 {
            vector: Hash64::from_bytes(claim.vector),
            unit,
            requester: *signer,
            requester_payout: requester.payout_payload,
            poster: h.producer,
            poster_payout: h.producer_payout,
            slot: request.slot,
            filed_daa: ctx.daa_score,
            deadline_daa: deadline,
            proof_until,
            bond,
            node_index,
            node_count,
            positions: view.trace.positions,
            segment_root: view.trace.segment_roots[(request.position / misaka_palw_kernel::seg::SEG_LEN_V4) as usize],
            scope_root: class.record().class_scope9_root,
            dtype: node.out.dtype.tag(),
            rank: shape.len() as u8,
            shape: fixed_shape,
            settled: false,
        },
    };
    state.open_demands += 1;
    if request.lane == 0 {
        state.ordinary_requests += 1;
        counts.open_ordinary += 1;
    }
    state.latest_answer_proof_until = state.latest_answer_proof_until.max(proof_until);
    state.required_retained_until = state.required_retained_until.max(proof_until);
    let demand = d.id();
    capacity::request(builder, id, request.lane, request.entitlement, request.slot, 1, demand.as_bytes(), ctx.daa_score, deadline)?;
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&(14u8, id, counts.next_ordinal)), Some(key(&demand)));
    counts.next_ordinal += 1;
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&(13u8, id)), Some(key(&counts)));
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, key(&demand), Some(key(&d)));
    builder.write_kernel_row(EXECUTION_DEMAND_SLOT_TABLE_V11, slot_key(signer, request.slot), Some(key(&demand)));
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&id), Some(key(&state)));
    builder.note_kernel_bond(signer);
    Ok(())
}
fn close_demand(builder: &mut TransitionBuilder<'_>, mut d: NativeExecutionDemandV11) -> Result<(), PalwStateV2Error> {
    let mut state = pursuit(builder.state.kernel_route.as_ref().unwrap(), &d.job_id)?;
    state.open_demands = state.open_demands.checked_sub(1).ok_or_else(|| refused("native open DA count underflow"))?;
    let mut counts = counters(builder.state.kernel_route.as_ref().unwrap(), d.job_id)?;
    if d.lane == 0 {
        counts.open_ordinary = counts.open_ordinary.checked_sub(1).ok_or_else(|| refused("native ordinary DA count underflow"))?;
    }
    d.body.settled = true;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, key(&d.id()), Some(key(&d)));
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&d.job_id), Some(key(&state)));
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&(13u8, d.job_id)), Some(key(&counts)));
    Ok(())
}
pub(super) fn answer(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    if bytes.len() < 64 {
        return Err(refused("native answer fixed prefix"));
    }
    let id = Hash64::from_bytes(fixed_id(&bytes[..64])?);
    let r = builder.state.kernel_route.as_ref().unwrap();
    let d = r.execution_demand_v11(&id).ok_or_else(|| refused("native original DA request absent"))?;
    let complete23 = r.aux.contains_key(&(EXECUTION_POLICY_TABLE_V11, key(&(23u8, d.job_id))));
    if bytes.len() > 68 + FLAT_VECTOR_ANSWER_MAX_BYTES_V9 {
        return Err(refused("native answer exceeds original wire cap"));
    }
    let frame = || {
        let raw = bytes.get(64..68).ok_or_else(|| refused("native answer nested count absent"))?;
        let n = u32::from_le_bytes(raw.try_into().unwrap()) as usize;
        if n != bytes.len() - 68 {
            return Err(refused("native answer bounded exact count"));
        }
        Ok(())
    };
    if !complete23 {
        frame()?;
    }
    if d.body.settled || ctx.daa_score > d.body.deadline_daa || *signer != d.body.poster {
        return Err(refused("native producer answer unauthorized/closed/late"));
    }
    capacity::debit_trace_attempt(builder, d.job_id, id.as_bytes())?;
    let trace_plan = trace_plan(builder, p, d.job_id)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let producer = palw_kernel_bond_id_v1(signer);
    let mut trace = trace_shard(builder, p, &trace_plan, producer, d.job_id, 0, d.job_id)?;
    trace.debit_reserved(&p.quota, &epoch, &trace_plan, &d.job_id, 0).map_err(refused)?;
    write_trace(builder, &trace, 0, d.job_id);
    if complete23 {
        frame()?;
    }
    let (_, class, book, plan) = original_job(builder, d.job_id)?;
    if book.terminal() != ExecutionTerminalV11::Challengeable {
        return Err(refused("native DA book terminal"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let binding = book.job().quota_subject(class.record()).map_err(refused)?;
    let mut shard = quota_shard(builder, p, &epoch, binding.producer, d.job_id)?;
    let action = plan.action(QuotaActionKindV11::DAAnswer, &bytes[68..]).map_err(refused)?;
    shard.debit_reserved(&p.quota, &epoch, &binding, &plan, &action).map_err(refused)?;
    write_shard(builder, &shard, d.job_id);
    let answer = decode_flat_vector_answer_v9(&bytes[68..]).map_err(|e| refused(e.to_string()))?;
    if !answer.authenticates(&d.body) {
        return Err(refused("native answer is not the original typed opening"));
    }
    let paid = builder.slash_bond(d.body.requester, d.body.bond as u128)?;
    if paid != d.body.bond {
        return Err(refused("native original requester DA debit differs"));
    }
    pay_delivery_fee(builder, ctx, d.job_id, d.body.poster, d.body.poster_payout, paid, false)?;
    let served = FlatVectorServedRowV9 {
        header: FlatVectorServedHeaderV9 { answered_daa: ctx.daa_score, answer_root: flat_vector_answer_root_v9(&bytes[68..]) },
        answer: bytes[68..].to_vec(),
    };
    builder.write_kernel_row(EXECUTION_SERVED_TABLE_V11, d.body.unit.key(&d.body.vector), Some(key(&served)));
    close_demand(builder, d)
}
pub(super) fn settle(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id = Hash64::from_bytes(fixed_id(bytes)?);
    let d = builder.state.kernel_route.as_ref().unwrap().execution_demand_v11(&id).ok_or_else(|| refused("native demand absent"))?;
    if d.body.settled {
        return Err(refused("native original demand already settled"));
    }
    if !prepay(builder, ctx, signer, crate::palw_model_artifact_v2::model_artifact_work_v2(0, 16 << 10).unwrap())? {
        return Ok(());
    }
    let (mut row, class, mut book, plan) = original_job(builder, d.job_id)?;
    if book.terminal() != ExecutionTerminalV11::Challengeable {
        return close_demand(builder, d);
    }
    if ctx.daa_score <= d.body.deadline_daa {
        return Err(refused("native answer deadline open"));
    }
    let claim = book.claim().unwrap();
    let established = d.body.deadline_daa.checked_add(1).ok_or_else(|| refused("native DA default clock overflow"))?;
    let proof = object_id(b"misaka-palw/native/original-trace-da-default/v11", &(d.id().as_bytes(), d.body.deadline_daa));
    let result = NativeExecutionJudgmentV11 {
        job_id: d.job_id,
        claim_id: claim.id(),
        vector: claim.vector,
        class_id: class.record().id(),
        inventory_root: class.record().inventory_root,
        parameter_root: class.record().parameter_root,
        current_daa: ctx.daa_score,
        established_daa: established,
        proof_id: proof,
        finder: palw_kernel_bond_id_v1(&d.body.requester),
        kind: NativeAdjudicationKindV11::DataUnavailable,
    };
    let reserve = claim.reserved_original;
    if builder.state.bonds.get(&row.header.producer).is_none_or(|b| b.collateral < reserve) {
        return Err(refused("native original DA default slash unavailable"));
    }
    let settlement = book.adjudicate(class.record(), &result, reserve).map_err(refused)?;
    close_adjudicated_quota(builder, ctx, p, d.job_id, &class, &book, &plan, *signer)?;
    let collected = builder.slash_bond(row.header.producer, reserve as u128)?;
    if collected != reserve {
        return Err(refused("native original DA default debit changed"));
    }
    credit(builder, ctx, &mut row, &book, &settlement)?;
    close_demand(builder, d)
}
