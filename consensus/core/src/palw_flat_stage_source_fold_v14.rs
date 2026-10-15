//! Acquisition of ORIGINAL producer-bound real PC. Typed job inputs are never
//! source weights, and missing external R never causes a producer default.
use super::super::palw_flat_execution_fold_v11 as native;
use super::*;
use crate::palw_flat_execution_v11::*;
use misaka_palw_kernel::{
    flat_quota11::source16::{SOURCE_ANSWER_MAX_BYTES_V16, opening::*},
    merkle::{AXIS_COL, AXIS_ROW},
    merkle3::LayoutV3,
};
#[derive(Clone, borsh::BorshSerialize, borsh::BorshDeserialize)]
struct StageSourcePendingV14 {
    version: u16,
    job: Digest,
    original_book: Digest,
    consent18: Digest,
    fee_plan: Digest,
    original_demand: Digest,
    requester_operator: Digest,
    producer_operator: Digest,
}
fn original_demand(d: &NativeSourceDemandV16) -> Digest {
    let mut original = d.clone();
    original.attempts = 0;
    original.settled = false;
    misaka_palw_kernel::hash::object_id(b"misaka-palw/native/stage-original-source-pending/v14", &original)
}
fn pending_authority(
    r: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    d: &NativeSourceDemandV16,
) -> Result<StageSourcePendingV14, PalwStateV2Error> {
    let a: StageSourcePendingV14 = r
        .aux_row(STAGE_TABLE_V14, &stage_key_v14(18, &d.id()))
        .ok_or_else(|| refused("stage source original operator authority absent"))?;
    let h = r.stage_job_header_v14(&d.request.job).ok_or_else(|| refused("stage source original typed header absent"))?;
    if a.version != 14
        || a.job != d.request.job
        || a.original_book != h.original_book_root
        || a.consent18 != h.acceptance
        || a.original_demand != original_demand(d)
        || a.requester_operator == [0; 64]
        || a.requester_operator == h.producer_operator
        || a.producer_operator != h.producer_operator
    {
        return Err(refused("stage source pending operator/consent/fee/source provenance differs"));
    }
    Ok(a)
}
impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    /// Fixed authenticated original pending metadata only, before the native
    /// signature/body path allocates or checks a 128-KiB source answer.
    pub fn stage_source_answer_owner_v14(&self, id: &Digest, now: u64) -> Option<PalwBondKeyV2> {
        use misaka_palw_kernel::flat_quota11::source16::{SOURCE_ROW_BYTES_V16, SourceObligationV16};
        let d = native::source::demand(self, *id).ok()?;
        let h = self.stage_job_header_v14(&d.request.job)?;
        let a = pending_authority(self, &d).ok()?;
        if d.settled
            || d.attempts >= 2
            || now > d.deadline
            || d.producer != h.producer
            || h.terminal != StageTerminalV14::Challengeable
        {
            return None;
        }
        let raw = self.aux.get(&(EXECUTION_QUOTA_TABLE_V11, bytes(&(16u8, d.request.job))))?;
        if raw.len() != 64 + SOURCE_ROW_BYTES_V16 as usize {
            return None;
        }
        let (root, row): (Digest, SourceObligationV16) = borsh::from_slice(raw).ok()?;
        if root != row.root()
            || row.retired
            || row.job_id != d.request.job
            || row.producer != palw_kernel_bond_id_v1(&d.producer)
            || row.original_consent_root != a.consent18
            || row.answers_used >= row.requests_admitted.checked_mul(2)?
        {
            return None;
        }
        Some(d.producer)
    }
}
fn pending_key(id: Digest) -> Vec<u8> {
    bytes(&(16u8, id))
}
fn slot_key(bond: PalwBondKeyV2, slot: u8) -> Vec<u8> {
    bytes(&(16u8, bond, slot))
}
pub(super) fn pursuit(builder: &TransitionBuilder<'_>, id: Digest) -> Result<NativeStagePursuitV14, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = r.stage_job_header_v14(&id).ok_or_else(|| refused("stage source original job absent"))?;
    let s: NativeStagePursuitV14 =
        r.aux_row(STAGE_TABLE_V14, &stage_key_v14(16, &id)).ok_or_else(|| refused("stage source pursuit counters absent"))?;
    if s.version != 14
        || s.job != id
        || s.class != h.class
        || s.original_book != h.original_book_root
        || s.consent18 != h.acceptance
        || s.open_demands > 32
        || s.retained_seals > 4
        || s.retained_discovery > 4
        || s.seal_keys.iter().filter(|k| **k != [0; 64]).count() != s.retained_seals as usize
        || s.discovery_keys.iter().filter(|k| **k != [0; 64]).count() != s.retained_discovery as usize
        || s.retained_seals + s.retained_discovery > 4
        || s.requests_admitted > 32
        || s.source_requests.iter().filter(|k| **k != [0; 64]).count() != s.requests_admitted as usize
        || s.source_units.iter().filter(|k| **k != [0; 64]).count() != s.requests_admitted as usize
    {
        return Err(refused("stage source original counters rebound"));
    }
    Ok(s)
}
pub(super) fn request(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() != 147 {
        return Err(refused("stage source exact fixed request frame"));
    }
    let req: NativeSourceRequestV16 = borsh::from_slice(body).map_err(|_| refused("stage source request fixed framing"))?;
    // Reserved owner lanes require the stage-specific paid seal/discovery
    // adapters; an ordinary request cannot manufacture those guarantees.
    if req.lane != 0
        || req.entitlement != [0; 64]
        || req.slot >= 2
        || req.kind > 2
        || req.layer_code > 65536
        || req.kind == 0 && (req.axis != 0 || req.level != 0 || req.index != 0)
        || req.kind > 0 && req.axis != AXIS_ROW && req.axis != AXIS_COL
        || req.kind == 2 && req.level != 0
    {
        return Err(refused("stage source request has unsupported entitlement/coordinate"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = r.stage_job_header_v14(&req.job).ok_or_else(|| refused("stage source original typed job absent"))?;
    let payer = builder.state.bonds.get(signer).ok_or_else(|| refused("stage source requester absent"))?;
    if h.terminal != StageTerminalV14::Challengeable
        || payer.operator_id.as_bytes() == h.producer_operator
        || ctx.daa_score > h.deadline_daa
    {
        return Err(refused("stage source original independent operator/claim/window differs"));
    }
    if native::source::slot_occupied(r, signer, req.slot)?
        || r.aux_row::<Hash64>(EXECUTION_DEMAND_SLOT_TABLE_V11, &bytes(&(*signer, req.slot)))
            .and_then(|id| r.execution_demand_v11(&id))
            .is_some_and(|d| !d.body.settled)
        || r.flat_vector_slot_v9(signer, req.slot).and_then(|id| r.flat_vector_demand_v9(&id)).is_some_and(|d| !d.settled)
        || r.flat_vector_slot_v7(signer, req.slot).and_then(|id| r.flat_vector_demand_v7(&id)).is_some_and(|d| !d.settled)
        || r.flat_vector_slot_v3(signer, req.slot).and_then(|id| r.flat_vector_demand_v3(&id)).is_some_and(|d| !d.settled)
        || r.model_vector_demand_slot_v2(signer, req.slot).and_then(|id| r.model_vector_demand_v2(&id)).is_some_and(|d| !d.settled)
    {
        return Err(refused("stage source shared original requester slot occupied"));
    }
    let ch = r.stage_class_header_v14(&h.class).ok_or_else(|| refused("stage source original class absent"))?;
    let sh = r.stage_source_header_v14(&ch.source_binding).ok_or_else(|| refused("stage source original source absent"))?;
    if !prepay_restore(builder, ctx, signer, p, sh.source_bytes_len, ch.wire_bytes_len, 147, restore_tariff(builder, h.class, 147)?)? {
        return Ok(());
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let restore = derive_stage_restore_metadata_v14(0, 0, h.book_bytes_len, h.input_raw_bytes, 0).map_err(refused)?;
    let mut restored = native::quota_shard(builder, &p.execution, &epoch, palw_kernel_bond_id_v1(signer), req.job)?;
    restored.debit_ordinary(&p.execution.quota, &epoch, &restore).map_err(refused)?;
    native::write_shard(builder, &restored, req.job)?;
    let class = checked_class(builder, ctx, p, h.class, false)?;
    let row = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_job_row_v14(&req.job)
        .ok_or_else(|| refused("stage source original bounded book corrupt"))?;
    let book = StageJobBookV14::from_rooted_bytes(&row.book_bytes, h.book_root).map_err(refused)?;
    let mut state = pursuit(builder, req.job)?;
    let deadline = ctx
        .daa_score
        .checked_add(p.execution.execution.horizons.da_answer)
        .ok_or_else(|| refused("stage source answer clock overflow"))?;
    let proof_until =
        deadline.checked_add(p.execution.execution.horizons.proof_grace).ok_or_else(|| refused("stage source grace overflow"))?;
    if state.open_demands >= 8
        || state.requests_admitted >= p.execution.discovery.ordinary_lifetime_requests
        || ctx.daa_score > book.challenge_end_daa().ok_or_else(|| refused("stage source challenge clock absent"))?
        || proof_until > book.deadline_daa()
    {
        return Err(refused("stage source ordinary lifetime/retention exhausted"));
    }
    let decl = class
        .source()
        .program()
        .params
        .get(req.param as usize)
        .ok_or_else(|| refused("stage source request is not a real source weight"))?;
    let layer = if req.layer_code == 0 { None } else { Some((req.layer_code - 1) as u16) };
    let pc = *class
        .source()
        .parameters()
        .by_instance
        .get(&(req.param, layer))
        .ok_or_else(|| refused("stage source real PC instance absent"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.aux.contains_key(&(EXECUTION_SERVED_TABLE_V11, pending_key(native::source::unit_key(&req)))) {
        return Err(refused("stage source original unit already served"));
    }
    if req.kind > 0 {
        let hdr = native::source::served_header(r, &req)?;
        if !hdr.authenticates(&pc) || !hdr.matches_decl(decl.dtype, &decl.shape) {
            return Err(refused("stage source authenticated header conflicts with original declaration"));
        }
        let layout = LayoutV3::try_of(&hdr.dims().ok_or_else(|| refused("stage source header dimensions"))?)
            .ok_or_else(|| refused("stage source original layout"))?;
        let mut count = layout.leaves(req.axis);
        for _ in 0..req.level {
            if count <= 1 {
                return Err(refused("stage source subtree beyond original depth"));
            }
            count = count.div_ceil(2);
        }
        if req.index >= count {
            return Err(refused("stage source coordinate expands original header"));
        }
    }
    let fee: NativeStageFeesV20 = r
        .aux_row(STAGE_TABLE_V14, &stage_key_v14(12, &req.job))
        .ok_or_else(|| refused("stage source original delivery fee contract absent"))?;
    if fee.version != 20
        || fee.job != req.job
        || fee.original_book != h.original_book_root
        || fee.consent18 != h.acceptance
        || fee.plan.root() != book.fee_plan_root()
    {
        return Err(refused("stage source delivery fee contract rebinds original18 book"));
    }
    let bond = fee.plan.source_unit_fee;
    let payer = builder.state.bonds.get(signer).unwrap();
    if bond == 0 || (payer.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < bond as u128 {
        return Err(refused("stage source original requester delivery bond unavailable"));
    }
    let mut shape = [0; 4];
    for (i, n) in decl.shape.iter().enumerate() {
        shape[i] = *n as u64;
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
        proof_until,
        bond,
        attempts: 0,
        settled: false,
    };
    let authority = StageSourcePendingV14 {
        version: 14,
        job: req.job,
        original_book: h.original_book_root,
        consent18: h.acceptance,
        fee_plan: fee.plan.root(),
        original_demand: original_demand(&d),
        requester_operator: payer.operator_id.as_bytes(),
        producer_operator: h.producer_operator,
    };
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let (plan, mut shard) = native::source::original_source(builder, &p.execution, req.job)?;
    if plan.root() != book.source_plan_root() {
        return Err(refused("stage source guarantee changes original18 quote"));
    }
    shard.admit_valid_request(&p.execution.quota, &epoch, &plan, &req.job).map_err(refused)?;
    state.open_demands += 1;
    state.source_requests[state.requests_admitted as usize] = d.id();
    state.source_units[state.requests_admitted as usize] = native::source::unit_key(&req);
    state.requests_admitted += 1;
    state.required_until = state.required_until.max(proof_until);
    native::write_source_shard(builder, &shard, req.job)?;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, pending_key(d.id()), Some(bytes(&d)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(18, &d.id()), Some(bytes(&authority)))?;
    builder.write_kernel_row(EXECUTION_DEMAND_SLOT_TABLE_V11, slot_key(*signer, req.slot), Some(bytes(&d.id())))?;
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(16, &req.job), Some(bytes(&state)))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}
fn close(builder: &mut TransitionBuilder<'_>, mut d: NativeSourceDemandV16) -> Result<(), PalwStateV2Error> {
    let mut s = pursuit(builder, d.request.job)?;
    s.open_demands = s.open_demands.checked_sub(1).ok_or_else(|| refused("stage source pending counter underflow"))?;
    d.settled = true;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, pending_key(d.id()), Some(bytes(&d)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(16, &d.request.job), Some(bytes(&s)))?;
    Ok(())
}
pub(super) fn answer(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() < 68 {
        return Err(refused("stage source answer exact fixed prefix"));
    }
    let id: Digest = body[..64].try_into().unwrap();
    let n = u32::from_le_bytes(body[64..68].try_into().unwrap()) as usize;
    if n != body.len() - 68 || n as u64 > SOURCE_ANSWER_MAX_BYTES_V16 {
        return Err(refused("stage source answer exceeds original128KiB unit"));
    }
    let mut d = native::source::demand(builder.state.kernel_route.as_ref().unwrap(), id)?;
    let authority = pending_authority(builder.state.kernel_route.as_ref().unwrap(), &d)?;
    let h = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_job_header_v14(&d.request.job)
        .ok_or_else(|| refused("stage source answer is not an original typed job"))?;
    if d.producer != *signer
        || d.producer != h.producer
        || d.settled
        || d.attempts >= 2
        || ctx.daa_score > d.deadline
        || h.terminal != StageTerminalV14::Challengeable
        || builder.state.bonds.get(signer).is_none_or(|b| b.operator_id.as_bytes() != authority.producer_operator)
    {
        return Err(refused("stage source original own producer/pending/attempt/window differs"));
    }
    let (plan, mut shard) = native::source::original_source(builder, &p.execution, d.request.job)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    shard.debit_answer(&p.execution.quota, &epoch, &plan, &palw_kernel_bond_id_v1(signer), n as u64).map_err(refused)?;
    d.attempts += 1;
    native::write_source_shard(builder, &shard, d.request.job)?;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, pending_key(id), Some(bytes(&d)))?;
    let answer = decode_answer_v16(&body[68..]).map_err(refused)?;
    let valid = match &answer {
        AnswerV16::Header(v) => d.request.kind == 0 && v.authenticates(&d.pc),
        AnswerV16::Subtree(v) => {
            d.request.kind == 1
                && v.axis == d.request.axis
                && v.level == d.request.level
                && v.index == d.request.index
                && v.authenticates(&d.pc)
        }
        AnswerV16::Leaf(v) => {
            let shape: Vec<_> = v
                .shape
                .iter()
                .map(|n| usize::try_from(*n))
                .collect::<Result<_, _>>()
                .map_err(|_| refused("stage source leaf dimensions"))?;
            let layout = LayoutV3::try_of(&shape).ok_or_else(|| refused("stage source leaf layout"))?;
            d.request.kind == 2
                && v.axis == d.request.axis
                && layout.leaf_index(v.axis, v.line, v.tile) == d.request.index
                && v.authenticates(&d.pc)
        }
    };
    if !valid {
        return Err(refused("stage source answer fails original bound-PC authentication"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut fee: NativeStageFeesV20 = r
        .aux_row(STAGE_TABLE_V14, &stage_key_v14(12, &d.request.job))
        .ok_or_else(|| refused("stage source original fee hold absent"))?;
    if fee.version != 20
        || fee.original_book != h.original_book_root
        || fee.consent18 != h.acceptance
        || fee.plan.root() != authority.fee_plan
        || fee.earned > fee.plan.maximum
        || d.bond != fee.plan.source_unit_fee
    {
        return Err(refused("stage source actual fee is outside original20 income envelope"));
    }
    if builder.state.bonds.get(&d.requester).is_none_or(|b| b.collateral < d.bond) {
        return Err(refused("stage source original requester fee is not fully collectible"));
    }
    let mut closed_pursuit = pursuit(builder, d.request.job)?;
    closed_pursuit.open_demands =
        closed_pursuit.open_demands.checked_sub(1).ok_or_else(|| refused("stage source pending counter underflow"))?;
    let expected_credit = d.bond.min(fee.plan.maximum - fee.earned);
    let payout_queue = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&h.producer_payout);
    if builder.state.pending_payouts.get(&payout_queue).is_some_and(|p| p.amount.checked_add(expected_credit).is_none()) {
        return Err(refused("stage source original producer payout sum overflow"));
    }
    let paid = builder.slash_bond(d.requester, d.bond as u128)?;
    if paid != d.bond {
        return Err(refused("stage source original requester fee debit differs"));
    }
    let credit = paid.min(fee.plan.maximum - fee.earned);
    if credit > 0 {
        let queue = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&h.producer_payout);
        builder.pay_native_recipient_v1(Hash64::from_bytes(d.request.job), h.producer, 19, ctx.daa_score, credit, queue, false)?;
        builder.add_kernel_payout(h.producer_payout, credit)?;
        fee.earned += credit;
    }
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(12, &d.request.job), Some(bytes(&fee)))?;
    builder.write_kernel_row(EXECUTION_SERVED_TABLE_V11, pending_key(native::source::unit_key(&d.request)), Some(body[68..].to_vec()))?;
    d.settled = true;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, pending_key(d.id()), Some(bytes(&d)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(16, &d.request.job), Some(bytes(&closed_pursuit)))?;
    Ok(())
}
pub(super) fn settle(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id: Digest = body.try_into().map_err(|_| refused("stage source default exact original pending id"))?;
    let d = native::source::demand(builder.state.kernel_route.as_ref().unwrap(), id)?;
    let authority = pending_authority(builder.state.kernel_route.as_ref().unwrap(), &d)?;
    let h = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_job_header_v14(&d.request.job)
        .ok_or_else(|| refused("stage source default original typed job absent"))?;
    if d.settled || d.producer != h.producer || ctx.daa_score <= d.deadline {
        return Err(refused("stage source default is settled/rebound/answer window open"));
    }
    if !prepay(builder, ctx, signer, restore_tariff(builder, h.class, 64)?)? {
        return Ok(());
    }
    if h.terminal != StageTerminalV14::Challengeable {
        return close(builder, d);
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let ch = r.stage_class_header_v14(&h.class).ok_or_else(|| refused("stage source default original class absent"))?;
    let sh = r.stage_source_header_v14(&ch.source_binding).ok_or_else(|| refused("stage source default original source absent"))?;
    let restore = derive_stage_restore_metadata_v14(sh.source_bytes_len, ch.wire_bytes_len, h.book_bytes_len, h.input_raw_bytes, 64)
        .map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut paid = native::quota_shard(builder, &p.execution, &epoch, palw_kernel_bond_id_v1(signer), d.request.job)?;
    paid.debit_ordinary(&p.execution.quota, &epoch, &restore).map_err(refused)?;
    native::write_shard(builder, &paid, d.request.job)?;
    let class = checked_class(builder, ctx, p, h.class, false)?;
    let mut row = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_job_row_v14(&d.request.job)
        .ok_or_else(|| refused("stage source default original bounded book corrupt"))?;
    let mut book = StageJobBookV14::from_rooted_bytes(&row.book_bytes, h.book_root).map_err(refused)?;
    let actual = class.check_job(book.job()).map_err(refused)?;
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
    let verdict_debit = paid.debit_stage_verdict_v14(&p.execution.quota, &epoch, &class, &book, &plan).map_err(refused)?;
    native::write_shard(builder, &paid, d.request.job)?;
    let layer = if d.request.layer_code == 0 { None } else { Some((d.request.layer_code - 1) as u16) };
    if class.source().parameters().by_instance.get(&(d.request.param, layer)) != Some(&d.pc)
        || d.producer_payout != h.producer_payout
        || d.request.lane != 0
    {
        return Err(refused("stage source default pending PC/producer is not the original real source namespace"));
    }
    let finder = builder.state.bonds.get(&d.requester).ok_or_else(|| refused("stage source original requester absent"))?;
    if finder.operator_id.as_bytes() != authority.requester_operator {
        return Err(refused("stage source original requester operator changed"));
    }
    let established = d.deadline.checked_add(1).ok_or_else(|| refused("stage source default clock overflow"))?;
    let proof =
        misaka_palw_kernel::hash::object_id(b"misaka-palw/native/stage-original-bound-PC-default/v14", &(id, d.pc, d.deadline));
    let reserve = book.producer_reserved();
    if builder.state.bonds.get(&h.producer).is_none_or(|p| p.collateral < reserve) {
        return Err(refused("stage source default original producer liability is not collectible"));
    }
    let outcome = book
        .adjudicate(
            &class,
            &plan,
            &p.execution.execution,
            &NativeStageJudgmentV14 {
                job: d.request.job,
                original_book: h.original_book_root,
                book: h.book_root,
                class: h.class,
                claim: book.claim_root().ok_or_else(|| refused("stage source default original claim absent"))?,
                source: class.source().id(),
                source_parameters: class.source().parameters().root(),
                inventory: class.source().inventory_root(),
                consent18: h.acceptance,
                proof,
                finder: palw_kernel_bond_id_v1(&d.requester),
                finder_operator: authority.requester_operator,
                established_daa: established,
                current_daa: ctx.daa_score,
                kind: StageAdjudicationKindV14::BoundSourceUnavailable,
            },
            reserve,
        )
        .map_err(refused)?;
    let mut queue = misaka_palw_kernel::hash::object_id(
        b"misaka-palw/native/stage-original-principal-refund/v14",
        &(d.request.job, h.original_book_root, h.requester_payout),
    );
    queue[0] = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_PAYOUT_KEY_PREFIX_V1;
    let queue = Hash64::from_bytes(queue);
    if builder.state.pending_payouts.contains_key(&queue)
        || builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(STAGE_TABLE_V14, stage_key_v14(11, &d.request.job)))
    {
        return Err(refused("stage source default original principal already credited"));
    }
    let counter = builder.state.kernel_route.as_ref().unwrap().execution_nonce_counter_v11(&h.requester_operator).map_err(refused)?;
    let terminal = check_stage_conviction_terminal_v14(
        &class,
        &book,
        &plan,
        &p.execution.quota,
        &NativeStageTerminalAnchorV14 {
            job_id: d.request.job,
            original_funded_book_root: h.original_book_root,
            terminal_book_root: book.root(),
            acceptance18: h.acceptance,
            nonce: NativeRequesterNonceWitnessV11 {
                requester: book.job().requester,
                requester_operator_id: h.requester_operator,
                next_nonce: counter.next_nonce,
                counter_root: counter.root(),
            },
            established_daa: established,
            principal_refund: outcome.requester_refund,
            principal_queue: queue.as_bytes(),
        },
        &outcome,
        reserve,
    )
    .map_err(refused)?;
    paid.mark_stage_verdict_closed_v14(&p.execution.quota, &epoch, &terminal, &verdict_debit).map_err(refused)?;
    let mut closed_pursuit = pursuit(builder, d.request.job)?;
    closed_pursuit.open_demands =
        closed_pursuit.open_demands.checked_sub(1).ok_or_else(|| refused("stage source original pending counter underflow"))?;
    if !builder.params.reporter_income_active_at(ctx.daa_score) {
        let finder_queue = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&d.requester_payout);
        if builder.state.pending_payouts.get(&finder_queue).is_some_and(|p| p.amount.checked_add(outcome.finder_credit).is_none()) {
            return Err(refused("stage source original finder payout sum overflow"));
        }
    }
    let collected = builder.slash_bond(h.producer, reserve as u128)?;
    if collected != outcome.collected_original_slash {
        return Err(refused("stage source default actual original producer slash differs"));
    }
    builder.write_payout(queue, Some(PalwPayoutV2 { payload: h.requester_payout, amount: outcome.requester_refund }))?;
    if outcome.finder_credit > 0 {
        if builder.params.reporter_income_active_at(ctx.daa_score) {
            let award = Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
                b"misaka-palw/native/stage-original-source-award/v14",
                &(d.request.job, outcome.root(), outcome.finder),
            ));
            builder.stage_reporter_recipient_v1(
                award,
                d.requester,
                ctx.daa_score,
                outcome.finder_credit,
                Hash64::from_bytes(d.request.job),
            )?;
        } else {
            builder.add_kernel_payout(d.requester_payout, outcome.finder_credit)?;
        }
    }
    builder.cancel_native_recipient_v1(Hash64::from_bytes(d.request.job), h.producer, 14);
    builder.cancel_native_recipient_v1(Hash64::from_bytes(d.request.job), h.producer, 19);
    builder.bond_budget_close_v1(
        &Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
            b"misaka-palw/native/stage-original-fixed-budget/v20",
            &d.request.job,
        )),
        ctx.daa_score,
    );
    row.header.book_root = book.root();
    row.header.terminal = book.terminal();
    row.header.producer_reserved = book.producer_reserved();
    row.book_bytes = bytes(&book);
    row.header.book_bytes_len = row.book_bytes.len() as u32;
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(5, &d.request.job), Some(bytes(&row)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(13, &d.request.job), Some(bytes(&outcome)))?;
    let credit = NativeStagePrincipalRefundV14 {
        version: 14,
        job: d.request.job,
        original_book: h.original_book_root,
        terminal_book: book.root(),
        acceptance18: h.acceptance,
        requester: h.requester,
        payout: h.requester_payout,
        queue,
        amount: outcome.requester_refund,
        established_daa: established,
    };
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(11, &d.request.job), Some(bytes(&credit)))?;
    native::write_shard(builder, &paid, d.request.job)?;
    let mut settled = d;
    settled.settled = true;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, pending_key(settled.id()), Some(bytes(&settled)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(16, &settled.request.job), Some(bytes(&closed_pursuit)))?;
    Ok(())
}
