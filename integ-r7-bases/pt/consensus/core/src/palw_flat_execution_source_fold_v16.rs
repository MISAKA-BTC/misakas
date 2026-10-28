//! Independently paid public acquisition of the producer-bound ORIGINAL PC.
//! External R absence never creates a request/default. Direct 128-KiB answers
//! use the owned original obligation and never a generic chunk catalog.
use super::*;
use misaka_palw_kernel::flat_quota11::source16::{
    SOURCE_ANSWER_MAX_BYTES_V16, SOURCE_PLAN_RECORD_BYTES_V16, SourcePlanRecordV16, opening::*,
};
use misaka_palw_kernel::merkle::{AXIS_COL, AXIS_ROW};
use misaka_palw_kernel::merkle3::LayoutV3;
fn pending_key(id: Digest) -> Vec<u8> {
    key(&(16u8, id))
}
fn slot_key(bond: &PalwBondKeyV2, slot: u8) -> Vec<u8> {
    key(&(16u8, *bond, slot))
}
pub(super) fn reserved(state: &PalwChainStateV2, bond: &PalwBondKeyV2) -> u128 {
    let Some(r) = state.kernel_route.as_ref() else {
        return 0;
    };
    (0u8..2)
        .filter_map(|slot| r.aux_row::<Digest>(EXECUTION_DEMAND_SLOT_TABLE_V11, &slot_key(bond, slot)))
        .filter_map(|id| demand(r, id).ok())
        .filter(|d| d.requester == *bond && !d.settled)
        .fold(0u128, |sum, d| sum.saturating_add(d.bond as u128))
}
pub(crate) fn unit_key(request: &NativeSourceRequestV16) -> Digest {
    object_id(
        b"misaka-palw/native/original-bound-pc-served/v16",
        &(request.job, request.param, request.layer_code, request.kind, request.axis, request.level, request.index),
    )
}
pub(crate) fn demand(r: &PalwKernelRouteStateV1, id: Digest) -> Result<NativeSourceDemandV16, PalwStateV2Error> {
    let raw = r.aux.get(&(EXECUTION_DEMAND_TABLE_V11, pending_key(id))).ok_or_else(|| refused("original bound-PC request absent"))?;
    if raw.len() > 1024 {
        return Err(refused("native source request fixed extent"));
    }
    let d: NativeSourceDemandV16 = borsh::from_slice(raw).map_err(|_| refused("native source request framing"))?;
    if d.id() != id || d.request.lane > 2 || d.request.slot >= 2 || d.request.kind > 2 || d.attempts > 2 {
        return Err(refused("native source original request identity/attempt bounds"));
    }
    Ok(d)
}
impl PalwKernelRouteStateV1 {
    /// Fixed pending-unit prefix for acceptance/signature preflight. It neither
    /// assembles data nor verifies/allocates the 128-KiB source answer body.
    pub fn execution_source_demand_v16(&self, id: &Digest) -> Option<NativeSourceDemandV16> {
        demand(self, *id).ok()
    }
    pub fn execution_source_answer_owner_v16(&self, id: &Digest, now: u64) -> Option<PalwBondKeyV2> {
        let d = demand(self, *id).ok()?;
        if d.settled || d.attempts >= 2 || now > d.deadline {
            return None;
        }
        let h = self.execution_job_header_v11(&d.request.job)?;
        if h.terminal != ExecutionTerminalV11::Challengeable || h.producer != d.producer {
            return None;
        }
        let raw = self.aux.get(&(EXECUTION_QUOTA_TABLE_V11, key(&(16u8, d.request.job))))?;
        if raw.len() != 64 + SOURCE_ROW_BYTES_V16 as usize {
            return None;
        }
        let (root, row): (Digest, SourceObligationV16) = borsh::from_slice(raw).ok()?;
        if row.root() != root
            || row.retired
            || row.job_id != d.request.job
            || row.producer != palw_kernel_bond_id_v1(&d.producer)
            || row.original_consent_root == [0; 64]
            || row.answers_used >= row.requests_admitted.checked_mul(2)?
        {
            return None;
        }
        Some(d.producer)
    }
}
pub(crate) fn slot_occupied(r: &PalwKernelRouteStateV1, actor: &PalwBondKeyV2, slot: u8) -> Result<bool, PalwStateV2Error> {
    let Some(id) = r.aux_row::<Digest>(EXECUTION_DEMAND_SLOT_TABLE_V11, &slot_key(actor, slot)) else {
        return Ok(false);
    };
    match demand(r, id) {
        Ok(d) => Ok(!d.settled),
        Err(_) => Err(refused("native original source slot lost its demand")),
    }
}
pub(crate) fn original_source(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV11,
    id: Digest,
) -> Result<(SourcePlanV16, SourceShardV16), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let z = read_rooted::<SourceObligationV16>(r, key(&(16u8, id)), SOURCE_ROW_BYTES_V16 as usize)?
        .ok_or_else(|| refused("native original source guarantee absent"))?;
    let raw = r
        .aux
        .get(&(EXECUTION_QUOTA_TABLE_V11, key(&(16u8, id, 1u8))))
        .ok_or_else(|| refused("native original source quote absent"))?;
    if raw.len() != SOURCE_PLAN_RECORD_BYTES_V16 as usize {
        return Err(refused("native original source quote fixed extent"));
    }
    let rec = SourcePlanRecordV16::from_rooted_bytes(raw, &z.1.plan_root).map_err(refused)?;
    let plan = SourcePlanV16::restore(&rec, &z.1.plan_root, &p.quota, &z.1.quota_plan_root).map_err(refused)?;
    let g = read_rooted::<QuotaGlobalRowV11>(r, vec![0], QUOTA_GLOBAL_ROW_BYTES_V11 as usize)?
        .ok_or_else(|| refused("native original source global absent"))?;
    let a = read_rooted::<QuotaActorRowV11>(r, quota_key(1, &z.1.producer), QUOTA_SELECTED_ACTOR_ROW_BYTES_V11 as usize)?
        .ok_or_else(|| refused("native original source producer absent"))?;
    let s = read_rooted::<QuotaSubjectSummaryV11>(r, quota_key(2, &id), QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11 as usize)?
        .ok_or_else(|| refused("native original source subject absent"))?;
    let shard = SourceShardV16::from_native_rows(&p.quota, &plan, &g.1, &g.0, (&a.1, &a.0), (&s.1, &s.0), Some((&z.1, &z.0)))
        .map_err(refused)?;
    Ok((plan, shard))
}
pub(crate) fn served_header(r: &PalwKernelRouteStateV1, request: &NativeSourceRequestV16) -> Result<HeaderV16, PalwStateV2Error> {
    let mut q = request.clone();
    q.kind = 0;
    q.axis = 0;
    q.level = 0;
    q.index = 0;
    let raw = r
        .aux
        .get(&(EXECUTION_SERVED_TABLE_V11, pending_key(unit_key(&q))))
        .ok_or_else(|| refused("source tree request requires previously authenticated public header"))?;
    match decode_answer_v16(raw).map_err(refused)? {
        AnswerV16::Header(h) => Ok(h),
        _ => Err(refused("source header cache type")),
    }
}
pub(super) fn request(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    if bytes.len() != 147 {
        return Err(refused("native source request exact fixed framing"));
    }
    let req: NativeSourceRequestV16 = borsh::from_slice(bytes).map_err(|_| refused("native source request framing"))?;
    if req.lane > 2
        || req.slot >= 2
        || req.kind > 2
        || req.layer_code > 65536
        || req.kind == 0 && (req.axis != 0 || req.level != 0 || req.index != 0)
        || req.kind > 0 && req.axis != AXIS_ROW && req.axis != AXIS_COL
        || req.kind == 2 && req.level != 0
    {
        return Err(refused("native source request canonical coordinate"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = r.execution_job_header_v11(&req.job).ok_or_else(|| refused("native source original job absent"))?;
    let actor = palw_kernel_bond_id_v1(signer);
    let payer = builder.state.bonds.get(signer).unwrap();
    if h.terminal != ExecutionTerminalV11::Challengeable
        || payer.operator_id.as_bytes() == h.producer_operator
        || ctx.daa_score > h.deadline_daa
    {
        return Err(refused("native source requester/operator/window differs"));
    }
    if slot_occupied(r, signer, req.slot)?
        || r.aux_row::<Hash64>(EXECUTION_DEMAND_SLOT_TABLE_V11, &key(&(*signer, req.slot)))
            .and_then(|id| r.execution_demand_v11(&id))
            .is_some_and(|d| !d.body.settled)
        || r.flat_vector_slot_v9(signer, req.slot).and_then(|id| r.flat_vector_demand_v9(&id)).is_some_and(|d| !d.settled)
        || r.flat_vector_slot_v7(signer, req.slot).and_then(|id| r.flat_vector_demand_v7(&id)).is_some_and(|d| !d.settled)
        || r.flat_vector_slot_v3(signer, req.slot).and_then(|id| r.flat_vector_demand_v3(&id)).is_some_and(|d| !d.settled)
        || r.model_vector_demand_slot_v2(signer, req.slot).and_then(|id| r.model_vector_demand_v2(&id)).is_some_and(|d| !d.settled)
    {
        return Err(refused("native original source/trace/shared requester slot occupied"));
    }
    if req.lane == 0 && req.entitlement != [0; 64] {
        return Err(refused("source ordinary request has no paid ticket"));
    }
    if req.lane == 0 && !prepay(builder, ctx, signer, class_work(builder, h.class_id, bytes.len())?)? {
        return Ok(());
    }
    if req.lane == 1 {
        let seal = court::seal_row(builder.state.kernel_route.as_ref().unwrap(), &req.entitlement)?;
        if seal.actor != actor || seal.job_id != req.job || seal.spent || ctx.daa_score < seal.mature_daa {
            return Err(refused("source request lacks original mature paid proof"));
        }
    } else if req.lane == 2 {
        discovery::check_request_owner(builder, ctx, actor, req.job, req.entitlement)?;
    }
    debit_trace_request(builder, ctx, p, actor, req.job, req.lane, req.entitlement)?;
    let (_, class, book, job_plan) = original_job(builder, req.job)?;
    let claim = book.claim().ok_or_else(|| refused("native source original claim absent"))?;
    let mut state = pursuit(builder.state.kernel_route.as_ref().unwrap(), &req.job)?;
    let mut counts = da::counters(builder.state.kernel_route.as_ref().unwrap(), req.job)?;
    let deadline =
        ctx.daa_score.checked_add(class.record().horizons.da_answer).ok_or_else(|| refused("source answer clock overflow"))?;
    let until = deadline.checked_add(class.record().horizons.proof_grace).ok_or_else(|| refused("source answer grace overflow"))?;
    if book.terminal() != ExecutionTerminalV11::Challengeable
        || ctx.daa_score > claim.challenge_end
        || until > claim.retention_until
        || state.open_demands >= NATIVE_EXECUTION_OPEN_TOTAL_V13
        || counts.next_ordinal == u32::MAX
        || req.lane == 0
            && (state.ordinary_requests >= p.discovery.ordinary_lifetime_requests
                || counts.open_ordinary >= NATIVE_EXECUTION_OPEN_ORDINARY_V13)
    {
        return Err(refused("native source original request lifetime/window/full bound"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    match req.lane {
        0 => {
            let mut s = quota_shard(builder, p, &epoch, actor, req.job)?;
            let binding = book.job().quota_subject(class.record()).map_err(refused)?;
            let action = job_plan.action(QuotaActionKindV11::DARequest, bytes).map_err(refused)?;
            s.debit_public_action(&p.quota, &epoch, &binding, &job_plan, &action).map_err(refused)?;
            write_shard(builder, &s, req.job)?;
        }
        1 => {
            let mut s = quota_shard_seal(builder, p, &epoch, actor, req.job, Some(req.entitlement))?;
            s.debit_protected(&p.quota, &epoch, &job_plan, ProtectedActionKindV11::DARequest, bytes).map_err(refused)?;
            write_shard(builder, &s, req.job)?;
        }
        2 => discovery::debit_request(builder, ctx, p, actor, req.job, req.entitlement, &job_plan, bytes)?,
        _ => unreachable!(),
    }
    let full = checked_class(builder, ctx, p, h.class_id, false)?;
    let program = &full.view().program;
    let decl = program.params.get(req.param as usize).ok_or_else(|| refused("source request parameter outside original program"))?;
    let layer = if req.layer_code == 0 { None } else { Some((req.layer_code - 1) as u16) };
    let pc = *full
        .parameters()
        .by_instance
        .get(&(req.param, layer))
        .ok_or_else(|| refused("source request outside original real PC map"))?;
    let mut shape = [0; 4];
    if decl.shape.len() > 4 {
        return Err(refused("source original declared rank"));
    }
    for (i, n) in decl.shape.iter().enumerate() {
        shape[i] = *n as u64;
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.aux.contains_key(&(EXECUTION_SERVED_TABLE_V11, pending_key(unit_key(&req)))) {
        return Err(refused("source original unit already served"));
    }
    if req.kind > 0 {
        let hdr = served_header(r, &req)?;
        if !hdr.authenticates(&pc) || !hdr.matches_decl(decl.dtype, &decl.shape) {
            return Err(refused("source tree header disagrees with original declared PC; file authenticated header fault"));
        }
        let layout = LayoutV3::try_of(&hdr.dims().ok_or_else(|| refused("source header dimensions"))?)
            .ok_or_else(|| refused("source header original layout"))?;
        let mut n = layout.leaves(req.axis);
        for _ in 0..req.level {
            if n <= 1 {
                return Err(refused("source subtree beyond original depth"));
            }
            n = n.div_ceil(2);
        }
        if req.index >= n {
            return Err(refused("source subtree/leaf outside original layout"));
        }
    }
    let policy = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    let bond =
        policy.demand_bond.max(policy.dismissal_fee_v1(
            crate::palw_model_artifact_v2::model_artifact_work_v2(0, SOURCE_ANSWER_MAX_BYTES_V16 as usize).unwrap(),
        ));
    let payer = builder.state.bonds.get(signer).unwrap();
    if (payer.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < bond as u128 {
        return Err(refused("source original requester bond unavailable"));
    }
    let d = NativeSourceDemandV16 {
        request: req.clone(),
        requester: *signer,
        requester_payout: payer.payout_payload,
        producer: h.producer,
        producer_payout: h.producer_payout,
        pc,
        declared_dtype: decl.dtype.tag(),
        declared_rank: decl.shape.len() as u8,
        declared_shape: shape,
        filed: ctx.daa_score,
        deadline,
        proof_until: until,
        bond,
        attempts: 0,
        settled: false,
    };
    let pending = d.id();
    let (source_plan, mut source) = original_source(builder, p, req.job)?;
    source.admit_valid_request(&p.quota, &epoch, &source_plan, &req.job).map_err(refused)?;
    capacity::request(builder, req.job, req.lane, req.entitlement, req.slot, 0, pending, ctx.daa_score, deadline)?;
    state.open_demands += 1;
    if req.lane == 0 {
        state.ordinary_requests += 1;
        counts.open_ordinary += 1;
    }
    state.latest_answer_proof_until = state.latest_answer_proof_until.max(until);
    state.required_retained_until = state.required_retained_until.max(until);
    write_source_shard(builder, &source, req.job)?;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, pending_key(pending), Some(key(&d)))?;
    builder.write_kernel_row(EXECUTION_DEMAND_SLOT_TABLE_V11, slot_key(signer, req.slot), Some(key(&pending)))?;
    builder.write_kernel_row(
        EXECUTION_DEMAND_COUNT_TABLE_V11,
        key(&(14u8, req.job, counts.next_ordinal)),
        Some(key(&(16u8, pending))),
    )?;
    counts.next_ordinal += 1;
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&(13u8, req.job)), Some(key(&counts)))?;
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&req.job), Some(key(&state)))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}
fn close(builder: &mut TransitionBuilder<'_>, mut d: NativeSourceDemandV16) -> Result<(), PalwStateV2Error> {
    let mut state = pursuit(builder.state.kernel_route.as_ref().unwrap(), &d.request.job)?;
    let mut c = da::counters(builder.state.kernel_route.as_ref().unwrap(), d.request.job)?;
    state.open_demands = state.open_demands.checked_sub(1).ok_or_else(|| refused("native source open count underflow"))?;
    if d.request.lane == 0 {
        c.open_ordinary = c.open_ordinary.checked_sub(1).ok_or_else(|| refused("native source ordinary count underflow"))?;
    }
    d.settled = true;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, pending_key(d.id()), Some(key(&d)))?;
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&d.request.job), Some(key(&state)))?;
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&(13u8, d.request.job)), Some(key(&c)))?;
    Ok(())
}
pub(super) fn answer(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id = fixed_id(bytes.get(..64).ok_or_else(|| refused("source answer fixed pending prefix"))?)?;
    if bytes.len() > 68 + SOURCE_ANSWER_MAX_BYTES_V16 as usize {
        return Err(refused("source answer exceeds original owned wire cap"));
    }
    let mut d = demand(builder.state.kernel_route.as_ref().unwrap(), id)?;
    if d.producer != *signer || d.settled || ctx.daa_score > d.deadline || d.attempts >= 2 {
        return Err(refused("source answer original producer/pending/attempt/window differs"));
    }
    // The pending-unit namespace is shared with typed Stage14. A text answer
    // must authenticate its own original book before it can debit a source
    // guarantee or collect the requester's delivery fee.
    let h = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .execution_job_header_v11(&d.request.job)
        .ok_or_else(|| refused("text source answer has no original text job"))?;
    if h.producer != d.producer || h.terminal != ExecutionTerminalV11::Challengeable {
        return Err(refused("text source answer original job/producer is not challengeable"));
    }
    let (plan, mut shard) = original_source(builder, p, d.request.job)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    shard
        .debit_answer(&p.quota, &epoch, &plan, &palw_kernel_bond_id_v1(signer), bytes.len().saturating_sub(68) as u64)
        .map_err(refused)?;
    d.attempts += 1;
    write_source_shard(builder, &shard, d.request.job)?;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, pending_key(id), Some(key(&d)))?;
    // Even a hostile nested Vec count consumes this original attempt. The
    // processor borrows/authenticates only the fixed pending identity first.
    let raw = bytes.get(64..68).ok_or_else(|| refused("source answer nested count absent"))?;
    let count = u32::from_le_bytes(raw.try_into().unwrap()) as usize;
    if count != bytes.len() - 68 {
        return Err(refused("source answer nested exact count differs"));
    }
    let answer = decode_answer_v16(&bytes[68..]).map_err(refused)?;
    let valid = match &answer {
        AnswerV16::Header(h) => d.request.kind == 0 && h.authenticates(&d.pc),
        AnswerV16::Subtree(s) => {
            d.request.kind == 1
                && s.axis == d.request.axis
                && s.level == d.request.level
                && s.index == d.request.index
                && s.authenticates(&d.pc)
        }
        AnswerV16::Leaf(l) => {
            let shape: Vec<usize> = l
                .shape
                .iter()
                .map(|n| usize::try_from(*n))
                .collect::<Result<_, _>>()
                .map_err(|_| refused("source original leaf shape overflow"))?;
            let layout = LayoutV3::try_of(&shape).ok_or_else(|| refused("source original leaf layout overflow"))?;
            d.request.kind == 2
                && l.axis == d.request.axis
                && layout.leaf_index(l.axis, l.line, l.tile) == d.request.index
                && l.authenticates(&d.pc)
        }
    };
    if !valid {
        return Err(refused("source answer differs from original bound PC unit"));
    }
    let paid = builder.slash_bond(d.requester, d.bond as u128)?;
    if paid != d.bond {
        return Err(refused("source original requester debit differs"));
    }
    pay_delivery_fee(builder, ctx, d.request.job, d.producer, d.producer_payout, paid, true)?;
    builder.write_kernel_row(EXECUTION_SERVED_TABLE_V11, pending_key(unit_key(&d.request)), Some(bytes[68..].to_vec()))?;
    close(builder, d)
}
pub(super) fn settle(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id = fixed_id(bytes)?;
    let d = demand(builder.state.kernel_route.as_ref().unwrap(), id)?;
    if d.settled {
        return Err(refused("source original demand already settled"));
    }
    let h = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .execution_job_header_v11(&d.request.job)
        .ok_or_else(|| refused("text source settlement has no original text job"))?;
    if h.producer != d.producer {
        return Err(refused("text source settlement original producer differs"));
    }
    if !prepay(builder, ctx, signer, crate::palw_model_artifact_v2::model_artifact_work_v2(0, 16 << 10).unwrap())? {
        return Ok(());
    }
    let (mut row, class, mut book, plan) = original_job(builder, d.request.job)?;
    if book.terminal() != ExecutionTerminalV11::Challengeable {
        return close(builder, d);
    }
    if ctx.daa_score <= d.deadline {
        return Err(refused("source original producer answer window open"));
    }
    let claim = book.claim().unwrap();
    let established = d.deadline.checked_add(1).ok_or_else(|| refused("source default clock overflow"))?;
    let result = NativeExecutionJudgmentV11 {
        job_id: d.request.job,
        claim_id: claim.id(),
        vector: claim.vector,
        class_id: class.record().id(),
        inventory_root: class.record().inventory_root,
        parameter_root: class.record().parameter_root,
        current_daa: ctx.daa_score,
        established_daa: established,
        proof_id: object_id(b"misaka-palw/native/original-bound-PC-DA-default/v16", &(id, d.pc, d.deadline)),
        finder: palw_kernel_bond_id_v1(&d.requester),
        kind: NativeAdjudicationKindV11::DataUnavailable,
    };
    // G14 lane closure G-14: a default is never refused for short collateral; what the bond still holds of the reserve is collected.
    let reserve = crate::palw_g14_lane_closure_v1::palw_native_producer_collectible_v1(
        builder.state.bonds.get(&row.header.producer).map(|b| b.collateral),
        claim.reserved_original,
    );
    let settlement = book.adjudicate(class.record(), &result, reserve).map_err(refused)?;
    close_adjudicated_quota(builder, ctx, p, d.request.job, &class, &book, &plan, *signer)?;
    let collected = if reserve == 0 { 0 } else { builder.slash_bond(row.header.producer, reserve as u128)? };
    if collected != reserve {
        return Err(refused("source original producer slash differs"));
    }
    credit(builder, ctx, &mut row, &book, &settlement)?;
    close(builder, d)
}
