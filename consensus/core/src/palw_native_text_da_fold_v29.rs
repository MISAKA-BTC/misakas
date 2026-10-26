//! Original paid public trace25 and real-PC acquisition. Requester restoration
//! consumes its own caller26 reservation, producer response consumes its own
//! Source16 promise, and neither can borrow the other's counters or holdings.
use super::*;
use crate::{palw_native_reply_parts_v29::*, palw_native_trace_da_v29::*};
use misaka_palw_kernel::{
    flat_quota11::source16::opening::*,
    flat_quota11::stage14::source26::*,
    merkle::{AXIS_COL, AXIS_ROW},
    merkle3::LayoutV3,
};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Demand {
    version: u16,
    id: Digest,
    request: NativePublicRequestV29,
    requester: PalwBondKeyV2,
    operator: Digest,
    producer: PalwBondKeyV2,
    producer_operator: Digest,
    funding: Digest,
    claim: Digest,
    source: Digest,
    lease: Digest,
    filed: u64,
    deadline: u64,
    proof_until: u64,
    bond: u64,
    trace: Option<crate::palw_flat_model_vector_da_v9::FlatVectorDemandV9>,
    pc: Digest,
    attempts: u8,
    settled: bool,
    ordinal: u32,
}
impl Demand {
    fn id(&self) -> Digest {
        object_id(
            b"misaka-palw/native/text-original-public-demand/v29",
            &(29u16, &self.request, self.requester, self.operator, self.funding, self.claim, self.filed),
        )
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Counts {
    ordinary: u32,
    open_ordinary: u32,
    open: u32,
    next: u32,
    collected: u32,
    latest_proof: u64,
}
fn counts(r: &PalwKernelRouteStateV1, job: Digest) -> Result<Counts, PalwStateV2Error> {
    let c: Counts = read(r, EXECUTION_DEMAND_COUNT_TABLE_V11, 0, &job, 64).unwrap_or_default();
    if c.open > 11 || c.open_ordinary > 3 || c.collected > c.next {
        return Err(refused("text29 bounded original public demand counters differ"));
    }
    Ok(c)
}
fn demand(r: &PalwKernelRouteStateV1, id: Digest) -> Result<Demand, PalwStateV2Error> {
    let d: Demand =
        read(r, EXECUTION_DEMAND_TABLE_V11, 0, &id, 2048).ok_or_else(|| refused("text29 original public pending unit absent"))?;
    if d.version != 29 || d.id != id || d.id() != id || d.attempts > 2 || d.request.slot >= 2 {
        return Err(refused("text29 original pending identity/counters differ"));
    }
    Ok(d)
}
pub(super) fn caller_plan(
    r: &PalwKernelRouteStateV1,
    job: Digest,
    b: &NativeTextBookV29,
    p: &NativeExecutionPoliciesV11,
) -> Result<CheckedStageSourcePlanV26, PalwStateV2Error> {
    let raw = r
        .aux
        .get(&(EXECUTION_QUOTA_TABLE_V11, key(&(26u8, job, 0u8))))
        .ok_or_else(|| refused("text29 original caller acquisition quote absent"))?;
    CheckedStageSourcePlanV26::restore_native_record(raw, b.funding().maximum.source_caller_plan, &p.quota).map_err(refused)
}
pub(super) fn caller(
    r: &PalwKernelRouteStateV1,
    p: &NativeExecutionPoliciesV11,
    plan: &CheckedStageSourcePlanV26,
    own: &NativePagedSealV29,
    global: &QuotaGlobalRowV11,
    actor: &QuotaActorRowV11,
) -> Result<StageSourceCallerShardV26, PalwStateV2Error> {
    let (subjectroot, subject): (Digest, QuotaSubjectSummaryV11) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &quota_key(2, &own.job))
        .ok_or_else(|| refused("text29 caller original subject absent"))?;
    let raw = r
        .aux
        .get(&(EXECUTION_QUOTA_TABLE_V11, key(&(26u8, own.job, own.id()))))
        .ok_or_else(|| refused("text29 original paid caller row absent"))?;
    if raw.len() > 2048 {
        return Err(refused("text29 original paid caller row extent"));
    }
    let (root, row): (Digest, StageSourceCallerRowV26) =
        borsh::from_slice(raw).map_err(|_| refused("text29 original caller grammar"))?;
    if row.original_paid != own.original_paid_receipt || row.operator != own.operator || row.owner != own.id() {
        return Err(refused("text29 original caller owner/payment/operator differs"));
    }
    StageSourceCallerShardV26::from_native_rows(
        &p.quota,
        plan,
        global,
        &global.root(),
        own.actor,
        Some((actor, &actor.root())),
        (&subject, &subjectroot),
        Some((&row, &root)),
    )
    .map_err(refused)
}
pub(super) fn reserved(r: &PalwKernelRouteStateV1, bond: &PalwBondKeyV2) -> u128 {
    (0u8..2)
        .filter_map(|slot| read::<Digest>(r, EXECUTION_DEMAND_SLOT_TABLE_V11, 0, &(*bond, slot), 64))
        .filter_map(|id| demand(r, id).ok())
        .filter(|d| !d.settled && d.requester == *bond)
        .map(|d| d.bond as u128)
        .sum()
}
pub(super) fn request(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() < 132 || raw.len() > 256 {
        return Err(refused("text29 bounded original request grammar"));
    }
    let job: Digest = raw[2..66].try_into().unwrap();
    let owner: Digest = raw[66..130].try_into().unwrap();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = original_header(r, job)?;
    let book = original_book(r, &h)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    // Paid tickets prepay restoration. Invalid public coordinates consume only
    // this caller attempt; producer answer guarantee is untouched until valid.
    let mut paid_caller = if owner != [0; 64] {
        let own = r.native_paged_seal_v29(job, owner).ok_or_else(|| refused("text29 original paid discovery/proof owner absent"))?;
        if own.bond != *signer
            || own.actor != palw_kernel_bond_id_v1(signer)
            || ctx.daa_score < own.posted
            || ctx.daa_score > own.horizon
            || own.retired
        {
            return Err(refused("text29 request does not own original prepaid caller"));
        }
        let plan = caller_plan(r, job, &book, p)?;
        let shard = quota_shard(builder, p, &epoch, own.actor, job)?;
        let mut caller = caller(r, p, &plan, &own, &shard.global_row(), &shard.actor_row())?;
        caller.debit_request(&p.quota, &epoch, &plan, owner, own.operator).map_err(refused)?;
        write_caller(builder, &caller);
        Some((plan, caller))
    } else {
        if !paid_restore(builder, ctx, signer, p, Some(h.class), raw.len() + h.book_bytes as usize)? {
            return Ok(());
        }
        None
    };
    let request: NativePublicRequestV29 = borsh::from_slice(raw).map_err(|_| refused("text29 fixed public request bytes"))?;
    if request.version != 29
        || request.job != job
        || request.paid_owner != owner
        || request.slot >= 2
        || book.terminal() != NativeTextTerminalV29::Challengeable
    {
        return Err(refused("text29 public request original job/lane/slot differs"));
    }
    let claim = book.claim().unwrap();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut c = counts(r, job)?;
    let unit = request.unit_id();
    if r.aux.contains_key(&(EXECUTION_SERVED_TABLE_V11, k(0, &unit)))
        || r.aux.contains_key(&(EXECUTION_DEMAND_TABLE_V11, k(1, &unit)))
        || c.open >= 11
        || owner == [0; 64] && (c.open_ordinary >= 3 || c.ordinary >= p.discovery.ordinary_lifetime_requests)
        || read::<Digest>(r, EXECUTION_DEMAND_SLOT_TABLE_V11, 0, &(*signer, request.slot), 64)
            .is_some_and(|id| demand(r, id).is_ok_and(|d| !d.settled))
    {
        return Err(refused("text29 original unit already promised/served or independently bounded public slots exhausted"));
    }
    let (_, registered, program) = class(builder, h.class)?;
    let post_raw =
        r.aux.get(&(EXECUTION_PROTECTED_TABLE_V11, k(0, &job))).ok_or_else(|| refused("text29 original public post absent"))?;
    let post = decode_post(post_raw, book.funding().request.maximum_new_tokens, book.funding().maximum.maximum_positions)?;
    let bond = builder.state.bonds.get(signer).ok_or_else(|| refused("text29 original demander absent"))?;
    let operator = bond.operator_id.as_bytes();
    let ledger = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    let amount = ledger.demand_bond.max(1);
    let fee = ledger.job_fee.max(1);
    if (bond.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < (amount as u128 + fee as u128) {
        return Err(refused("text29 original request actual fee/obligation unavailable"));
    }
    let deadline =
        ctx.daa_score.checked_add(p.execution.horizons.da_answer).ok_or_else(|| refused("text29 response clock overflow"))?;
    let proof_until = deadline
        .checked_add(p.execution.horizons.proof_grace)
        .ok_or_else(|| refused("text29 public response proof horizon overflow"))?;
    let horizon = if owner == [0; 64] { claim.retained_until } else { r.native_paged_seal_v29(job, owner).unwrap().horizon };
    if proof_until >= book.funding().deadline || proof_until > horizon {
        return Err(refused("text29 public request late for original full response/proof horizon"));
    }
    let (trace, pc) = match &request.unit {
        NativePublicUnitV29::Trace(unit) => {
            let geometry =
                misaka_palw_kernel::flat_resources29::flat_node_geometry_v29(&program, unit.position, unit.occurrence, unit.node)
                    .map_err(refused)?;
            if unit.position >= claim.actual_positions {
                return Err(refused("text29 public trace position outside actual stopped extent"));
            }
            let dims = &geometry.shape;
            if dims.len() > 4 {
                return Err(refused("text29 trace rank exceeds bounded geometry"));
            }
            let mut shape = [0; 4];
            for (i, n) in dims.iter().enumerate() {
                shape[i] = *n as u64;
            }
            match unit.kind {
                crate::palw_flat_model_vector_da_v9::FlatVectorUnitKindV9::Hash(index)
                    if index
                        >= misaka_palw_kernel::merkle5::FlatLayoutV5::of(&shape[..dims.len()]).map(|l| l.leaves()).unwrap_or(0) =>
                {
                    return Err(refused("text29 output leaf hash outside original geometry"));
                }
                crate::palw_flat_model_vector_da_v9::FlatVectorUnitKindV9::Value(index)
                    if index
                        >= dims
                            .iter()
                            .try_fold(1u64, |n, d| n.checked_mul(*d as u64))
                            .ok_or_else(|| refused("text29 trace cell extent overflow"))? =>
                {
                    return Err(refused("text29 selected cell outside original geometry"));
                }
                _ => {}
            }
            (
                Some(crate::palw_flat_model_vector_da_v9::FlatVectorDemandV9 {
                    vector: Hash64::from_bytes(claim.vector),
                    unit: unit.clone(),
                    requester: *signer,
                    requester_payout: bond.payout_payload,
                    poster: h.producer,
                    poster_payout: h.producer_payout,
                    slot: request.slot,
                    filed_daa: ctx.daa_score,
                    deadline_daa: deadline,
                    proof_until,
                    bond: amount,
                    node_index: geometry.node_index,
                    node_count: geometry.node_count,
                    positions: claim.actual_positions,
                    segment_root: *post
                        .segment_roots
                        .get((unit.position / misaka_palw_kernel::seg::SEG_LEN_V4) as usize)
                        .ok_or_else(|| refused("text29 original segment absent"))?,
                    scope_root: claim.actual_scope25,
                    dtype: geometry.dtype.tag(),
                    rank: dims.len() as u8,
                    shape,
                    settled: false,
                }),
                [0; 64],
            )
        }
        NativePublicUnitV29::Source { param, layer_code, kind, axis, level, index } => {
            let layer = if *layer_code == 0 {
                None
            } else {
                Some(u16::try_from(layer_code - 1).map_err(|_| refused("text29 source layer range"))?)
            };
            let pc = *registered
                .real_parameters
                .by_instance
                .get(&(*param, layer))
                .ok_or_else(|| refused("text29 source real instance absent"))?;
            let decl = program.params.get(*param as usize).ok_or_else(|| refused("text29 original source declaration absent"))?;
            if *kind > 2 || (*kind > 0 && *axis != AXIS_ROW && *axis != AXIS_COL) {
                return Err(refused("text29 source unit kind/axis differs"));
            }
            let dims: Vec<_> = decl.shape.iter().map(|n| *n as usize).collect();
            let layout = LayoutV3::try_of(&dims).ok_or_else(|| refused("text29 source original layout overflow"))?;
            if *kind == 0 && (*axis != 0 || *level != 0 || *index != 0) || *kind == 2 && *level != 0 {
                return Err(refused("text29 source noncanonical coordinate"));
            }
            if *kind > 0 {
                let mut leaves = layout.leaves(*axis);
                for _ in 0..*level {
                    if leaves <= 1 {
                        return Err(refused("text29 source level outside original tree"));
                    }
                    leaves = leaves.div_ceil(2);
                }
                if *index >= leaves {
                    return Err(refused("text29 source original subtree/leaf index outside"));
                }
            }
            (None, pc)
        }
    };
    let mut d = Demand {
        version: 29,
        id: [0; 64],
        request,
        requester: *signer,
        operator,
        producer: h.producer,
        producer_operator: book.funding().request.producer_operator,
        funding: h.funding,
        claim: claim.root(),
        source: book.funding().maximum.source_acquisition,
        lease: {
            let (root, l): (Digest, NativePagedLeaseJournalV29) = r
                .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(29u8, job, 22u8)))
                .ok_or_else(|| refused("text29 original response lease absent"))?;
            if root != l.root() {
                return Err(refused("text29 response lease root differs"));
            }
            l.lease_root
        },
        filed: ctx.daa_score,
        deadline,
        proof_until,
        bond: amount,
        trace,
        pc,
        attempts: 0,
        settled: false,
        ordinal: c.next,
    };
    d.id = d.id();
    let (slotroot, mut slots): (Digest, crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(22u8, job, 2u8)))
        .ok_or_else(|| refused("text29 original physical stream book absent"))?;
    if slots.root() != slotroot || slots.job != job || slots.original_consent23 != book.funding().consent {
        return Err(refused("text29 original stream book differs"));
    }
    slots
        .reserve_request(
            if owner == [0; 64] { 0 } else { 1 },
            owner,
            d.request.slot,
            if d.trace.is_some() { 1 } else { 0 },
            d.id,
            ctx.daa_score,
            proof_until,
        )
        .map_err(refused)?;
    if let Some((plan, caller)) = &mut paid_caller {
        caller.admit_request(plan).map_err(refused)?;
    }
    let (source_plan, mut source_shard) = super::super::source::original_source(builder, p, job)?;
    source_shard.admit_valid_request(&p.quota, &epoch, &source_plan, &job).map_err(refused)?;
    if builder.slash_bond(*signer, fee as u128)? != fee {
        return Err(PalwStateV2Error::BondBudgetRefused("text29 original request fee changed after preflight".into()));
    }
    if let Some((_, caller)) = &paid_caller {
        write_caller(builder, caller);
    }
    write_source_shard(builder, &source_shard, job);
    c.open += 1;
    if owner == [0; 64] {
        c.open_ordinary += 1;
        c.ordinary += 1;
    }
    c.next = c.next.checked_add(1).ok_or_else(|| refused("text29 lifetime request ordinal overflow"))?;
    c.latest_proof = c.latest_proof.max(proof_until);
    for (table, kk, raw) in [
        (EXECUTION_DEMAND_TABLE_V11, k(0, &d.id), key(&d)),
        (EXECUTION_DEMAND_TABLE_V11, k(1, &unit), key(&d.id)),
        (EXECUTION_DEMAND_TABLE_V11, k(2, &(job, d.ordinal)), key(&(d.id, unit))),
        (EXECUTION_DEMAND_SLOT_TABLE_V11, k(0, &(*signer, d.request.slot)), key(&d.id)),
        (EXECUTION_DEMAND_COUNT_TABLE_V11, k(0, &job), key(&c)),
        (EXECUTION_QUOTA_TABLE_V11, key(&(22u8, job, 2u8)), key(&(slots.root(), slots))),
    ] {
        builder.write_kernel_row(table, kk, Some(raw));
    }
    builder.note_kernel_bond(signer);
    Ok(())
}
fn progress(r: &PalwKernelRouteStateV1, id: Digest) -> Option<NativeReplyProgressV29> {
    let raw = r.aux.get(&(EXECUTION_DEMAND_TABLE_V11, k(3, &id)))?;
    if raw.len() > 1024 {
        return None;
    }
    let (root, p): (Digest, NativeReplyProgressV29) = borsh::from_slice(raw).ok()?;
    (p.root() == root && p.pending == id && p.check().is_ok()).then_some(p)
}
fn carriage(r: &PalwKernelRouteStateV1, d: &Demand) -> Option<crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29> {
    let raw = r.aux.get(&(EXECUTION_QUOTA_TABLE_V11, key(&(29u8, d.request.job, 23u8))))?;
    if raw.len() > 2048 {
        return None;
    }
    let (root, p): (Digest, crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29) = borsh::from_slice(raw).ok()?;
    let (lr, l): (Digest, NativePagedLeaseJournalV29) = r.aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(29u8, d.request.job, 22u8)))?;
    (p.version == 29
        && p.root() == root
        && l.root() == lr
        && l.namespace == 29
        && l.job == d.request.job
        && l.funding == d.funding
        && l.lease_root == d.lease
        && l.lease.plan == root
        && p.answer_parts <= 4)
        .then_some(p)
}
impl PalwKernelRouteStateV1 {
    /// Only fixed original rows are read before signature/body decoding. A
    /// malformed nested page count remains for the prepaid native attempt.
    pub fn native_text_reply_owner_v29(
        &self,
        pending: Digest,
        index: u16,
        count: u16,
        total: u32,
        group: Digest,
        now: u64,
    ) -> Option<PalwBondKeyV2> {
        let d = demand(self, pending).ok()?;
        let h = original_header(self, d.request.job).ok()?;
        let p = carriage(self, &d)?;
        if d.settled
            || now > d.deadline
            || h.terminal != NativeTextTerminalV29::Challengeable
            || h.producer != d.producer
            || h.funding != d.funding
            || group == [0; 64]
            || count == 0
            || count > p.answer_parts
            || index >= count
            || total == 0
            || total > p.original_answer_bytes
            || total.div_ceil(32 << 10) != count as u32
        {
            return None;
        }
        match progress(self, pending) {
            Some(a) if !a.burned && !a.completed => {
                if a.attempt != d.attempts
                    || a.producer != palw_kernel_bond_id_v1(&d.producer)
                    || a.original_claim != d.claim
                    || a.original_source != d.source
                    || a.original_lease != d.lease
                    || a.group != group
                    || a.total != total
                    || a.count != count
                    || a.accepted & (1 << index) != 0
                {
                    return None;
                }
            }
            _ => {
                if d.attempts >= 2 || index != 0 {
                    return None;
                }
            }
        }
        Some(d.producer)
    }
    pub fn native_paid_text_reply_eligibility_v29(
        &self,
        pending: Digest,
        index: u16,
        count: u16,
        total: u32,
        group: Digest,
        now: u64,
        mass: &crate::palw_native_carriage_v22::NativeCarriageMassParamsV22,
        service: &crate::palw_flat_execution_capacity_v22::NativeCarriageServiceV22,
    ) -> Option<crate::palw_flat_execution_capacity_v22::NativePaidCarrierEligibilityV22> {
        let owner = self.native_text_reply_owner_v29(pending, index, count, total, group, now)?;
        let d = demand(self, pending).ok()?;
        let p = carriage(self, &d)?;
        let (qr, q): (Digest, NativeQueueLeaseV30) = self.aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(30u8, d.request.job, 1u8)))?;
        let (lr, l): (Digest, NativePagedLeaseJournalV29) =
            self.aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(29u8, d.request.job, 22u8)))?;
        if !service.implemented
            || service.mass_params != mass.root()
            || p.service != service.root()
            || l.root() != lr
            || l.lease_root != d.lease
            || l.service != p.service
            || l.lease.retired
            || q.root() != qr
            || q.retired
            || q.job != d.request.job
            || q.original_funding != d.funding
            || q.original_physical_lease != d.lease
            || q.not_before < d.proof_until
            || mass.packet_mass(&p.answer_envelope).ok()? > service.slot_mass
        {
            return None;
        }
        Some(crate::palw_flat_execution_capacity_v22::NativePaidCarrierEligibilityV22 {
            original_job: d.request.job,
            unit: pending,
            owner,
            deadline: d.deadline,
            kind: 4,
            lease_root: d.lease,
            original_service: p.service,
            envelope: p.answer_envelope,
        })
    }
}
fn burn_progress(builder: &mut TransitionBuilder<'_>, p: &mut NativeReplyProgressV29) {
    p.burned = true;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, k(3, &p.pending), Some(key(&(p.root(), &*p))));
    for index in 0..p.count {
        builder.write_kernel_row(EXECUTION_SERVED_TABLE_V11, k(3, &(p.pending, p.attempt, index)), None);
    }
}
fn close(builder: &mut TransitionBuilder<'_>, mut d: Demand) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut c = counts(r, d.request.job)?;
    c.open = c.open.checked_sub(1).ok_or_else(|| refused("text29 original public open count underflow"))?;
    if d.request.paid_owner == [0; 64] {
        c.open_ordinary = c.open_ordinary.checked_sub(1).ok_or_else(|| refused("text29 original ordinary count underflow"))?;
    }
    d.settled = true;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, k(0, &d.id), Some(key(&d)));
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, k(0, &d.request.job), Some(key(&c)));
    builder.write_kernel_row(EXECUTION_DEMAND_SLOT_TABLE_V11, k(0, &(d.requester, d.request.slot)), None);
    Ok(())
}
pub(super) fn answer(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    let Some(meta) = borrowed_native_reply_metadata_v29(raw) else { return Ok(()) };
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.native_text_reply_owner_v29(meta.pending, meta.index, meta.count, meta.total, meta.group, ctx.daa_score) != Some(*signer) {
        return Ok(());
    }
    let mut d = demand(r, meta.pending)?;
    let mut a = match progress(r, d.id) {
        Some(a) if !a.burned && !a.completed => a,
        _ => {
            let (plan, mut shard) = super::super::source::original_source(builder, p, d.request.job)?;
            let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
            shard.debit_answer(&p.quota, &epoch, &plan, &palw_kernel_bond_id_v1(signer), meta.total as u64).map_err(refused)?;
            d.attempts = d
                .attempts
                .checked_add(1)
                .filter(|n| *n <= 2)
                .ok_or_else(|| refused("text29 complete original answer attempts exhausted"))?;
            let a = NativeReplyProgressV29 {
                version: 29,
                pending: d.id,
                producer: palw_kernel_bond_id_v1(signer),
                original_claim: d.claim,
                original_source: d.source,
                original_lease: d.lease,
                attempt: d.attempts,
                group: meta.group,
                count: meta.count,
                total: meta.total,
                accepted: 0,
                burned: false,
                completed: false,
            };
            a.check().map_err(refused)?;
            write_source_shard(builder, &shard, d.request.job);
            builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, k(0, &d.id), Some(key(&d)));
            builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, k(3, &d.id), Some(key(&(a.root(), &a))));
            a
        }
    }; // Full4-fragment+assembly price persisted BEFORE ANY nested body decode/allocation.
    let Some(part) = borrowed_native_reply_part_v29(raw) else {
        burn_progress(builder, &mut a);
        return Ok(());
    };
    a.accepted |= 1 << part.index;
    builder.write_kernel_row(EXECUTION_SERVED_TABLE_V11, k(3, &(d.id, a.attempt, part.index)), Some(part.bytes.to_vec()));
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, k(3, &d.id), Some(key(&(a.root(), &a))));
    if a.accepted != (1u8 << a.count) - 1 {
        return Ok(());
    }
    let mut bytes = Vec::with_capacity(a.total as usize);
    for index in 0..a.count {
        let Some(part) =
            builder.state.kernel_route.as_ref().unwrap().aux.get(&(EXECUTION_SERVED_TABLE_V11, k(3, &(d.id, a.attempt, index))))
        else {
            burn_progress(builder, &mut a);
            return Ok(());
        };
        let expected = if index + 1 == a.count { a.total as usize - index as usize * (32 << 10) } else { 32 << 10 };
        if part.len() != expected {
            burn_progress(builder, &mut a);
            return Ok(());
        }
        bytes.extend_from_slice(part);
    }
    if bytes.len() != a.total as usize || native_reply_group_v29(d.id, &bytes) != a.group {
        burn_progress(builder, &mut a);
        return Ok(());
    }
    let valid = match &d.request.unit {
        NativePublicUnitV29::Trace(_) => {
            NativeTraceAnswerV29::from_bytes(&bytes).is_ok_and(|a| d.trace.as_ref().is_some_and(|t| a.authenticates(t)))
        }
        NativePublicUnitV29::Source { kind, axis, level, index, .. } => match decode_answer_v16(&bytes) {
            Ok(AnswerV16::Header(h)) => *kind == 0 && h.authenticates(&d.pc),
            Ok(AnswerV16::Subtree(s)) => {
                *kind == 1 && s.axis == *axis && s.level == *level && s.index == *index && s.authenticates(&d.pc)
            }
            Ok(AnswerV16::Leaf(l)) => {
                let shape: Vec<_> = l.shape.iter().map(|n| usize::try_from(*n).ok()).collect::<Option<_>>().unwrap_or_default();
                *kind == 2
                    && l.axis == *axis
                    && LayoutV3::try_of(&shape).map(|layout| layout.leaf_index(l.axis, l.line, l.tile)) == Some(*index)
                    && l.authenticates(&d.pc)
            }
            Err(_) => false,
        },
    };
    if !valid {
        burn_progress(builder, &mut a);
        return Ok(());
    } // malformed proof retains original full answer expenditure.
    // No unsolicited producer income: exact initial29 consent burns delivery
    // bonds/fees. Source and reference gains remain under original reward cap.
    if builder.state.bonds.get(&d.requester).is_none_or(|v| v.collateral < d.bond) {
        return Err(PalwStateV2Error::BondBudgetRefused("text29 original requester obligation no longer collectible".into()));
    }
    if builder.slash_bond(d.requester, d.bond as u128)? != d.bond {
        return Err(PalwStateV2Error::BondBudgetRefused("text29 original requester debit changed after valid reply".into()));
    }
    a.completed = true;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, k(3, &d.id), Some(key(&(a.root(), &a))));
    builder.write_kernel_row(EXECUTION_SERVED_TABLE_V11, k(0, &d.request.unit_id()), Some(bytes));
    for index in 0..a.count {
        builder.write_kernel_row(EXECUTION_SERVED_TABLE_V11, k(3, &(d.id, a.attempt, index)), None);
    }
    close(builder, d)
}
pub(super) fn default(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id = fixed_id(raw)?;
    let d = demand(builder.state.kernel_route.as_ref().unwrap(), id)?;
    if d.settled || ctx.daa_score <= d.deadline || ctx.daa_score > d.proof_until {
        return Err(refused("text29 original default unit is answered/immature/late"));
    }
    let h = original_header(builder.state.kernel_route.as_ref().unwrap(), d.request.job)?;
    if !paid_restore(builder, ctx, signer, p, Some(h.class), raw.len() + h.book_bytes as usize)? {
        return Ok(());
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut h = original_header(r, d.request.job)?;
    let mut b = original_book(r, &h)?;
    let claim = b.claim().ok_or_else(|| refused("text29 original default claim absent"))?;
    let finder = builder.state.bonds.get(signer).ok_or_else(|| refused("text29 default finder absent"))?;
    if b.terminal() != NativeTextTerminalV29::Challengeable
        || claim.root() != d.claim
        || b.funding().root() != d.funding
        || h.producer != d.producer
        || finder.operator_id.as_bytes() == d.producer_operator
    {
        return Err(refused("text29 actual default book/producer/independent finder differs"));
    }
    let proof = object_id(
        b"misaka-palw/native/text-objective-original-response-default/v29",
        &(d.id, d.claim, d.source, d.lease, d.deadline, d.proof_until, ctx.block.as_bytes(), ctx.daa_score),
    );
    let s = b
        .convict(
            proof,
            palw_kernel_bond_id_v1(signer),
            finder.operator_id.as_bytes(),
            b.funding().maximum.producer_reserve,
            ctx.daa_score,
        )
        .map_err(refused)?;
    settle(builder, ctx, p, &mut h, &b, &s, Some(signer))?;
    close(builder, d)
}
pub(super) fn open(r: &PalwKernelRouteStateV1, job: Digest) -> Result<(u32, u64), PalwStateV2Error> {
    let c = counts(r, job)?;
    Ok((c.open, c.latest_proof))
}
pub(super) fn owner_open(r: &PalwKernelRouteStateV1, job: Digest, owner: Digest) -> Result<u32, PalwStateV2Error> {
    let (slotsroot, s): (Digest, crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(22u8, job, 2u8)))
        .ok_or_else(|| refused("text29 original owner stream book absent"))?;
    if s.root() != slotsroot || s.job != job {
        return Err(refused("text29 original owner stream root differs"));
    }
    let i = s.owners.iter().position(|v| *v == owner).ok_or_else(|| refused("text29 original physical owner absent"))?;
    let mut n = 0;
    for id in &s.units[3 + i * 2..5 + i * 2] {
        if *id != [0; 64] && demand(r, *id)?.settled == false {
            n += 1;
        }
    }
    Ok(n)
}
pub(super) fn collect_step(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    job: Digest,
) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = original_header(r, job)?;
    let mut c = counts(r, job)?;
    if c.collected == c.next {
        return Err(refused("text29 original indexed public cache already collected"));
    }
    let (id, unit): (Digest, Digest) = read(r, EXECUTION_DEMAND_TABLE_V11, 2, &(job, c.collected), 128)
        .ok_or_else(|| refused("text29 original indexed pending row absent"))?;
    let mut d = demand(r, id)?;
    if d.request.job != job || d.ordinal != c.collected || d.request.unit_id() != unit || ctx.daa_score <= d.proof_until {
        return Err(refused("text29 original indexed answer/default cache retained"));
    }
    let mut bytes = 4096usize;
    for attempt in 1u8..=2 {
        for index in 0u16..4 {
            if let Some(row) = r.aux.get(&(EXECUTION_SERVED_TABLE_V11, k(3, &(id, attempt, index)))) {
                if row.len() > 32 << 10 {
                    return Err(refused("text29 original reply cache extent differs"));
                }
                bytes = bytes.checked_add(row.len()).ok_or_else(|| refused("text29 original reply cache journal overflow"))?;
            }
        }
    }
    if let Some(row) = r.aux.get(&(EXECUTION_SERVED_TABLE_V11, k(0, &unit))) {
        if row.len() > 128 << 10 {
            return Err(refused("text29 original served response extent differs"));
        }
        bytes = bytes.checked_add(row.len()).ok_or_else(|| refused("text29 original response cache journal overflow"))?;
    }
    if !paid_restore(builder, ctx, signer, p, Some(h.class), bytes)? {
        return Ok(());
    } // current actual restore/journal, no prior resource refund.
    if !d.settled {
        let r = builder.state.kernel_route.as_ref().unwrap();
        let b = original_book(r, &h)?;
        if b.terminal() == NativeTextTerminalV29::Challengeable {
            return Err(refused("text29 unresolved original default must adjudicate before collection"));
        }
        d.settled = true;
        c.open = c.open.checked_sub(1).ok_or_else(|| refused("text29 original open collection count underflow"))?;
        if d.request.paid_owner == [0; 64] {
            c.open_ordinary =
                c.open_ordinary.checked_sub(1).ok_or_else(|| refused("text29 original ordinary collection count underflow"))?;
        }
        builder.write_kernel_row(EXECUTION_DEMAND_SLOT_TABLE_V11, k(0, &(d.requester, d.request.slot)), None);
    }
    c.collected += 1;
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, k(0, &job), Some(key(&c)));
    for (table, kk) in [
        (EXECUTION_DEMAND_TABLE_V11, k(0, &id)),
        (EXECUTION_DEMAND_TABLE_V11, k(1, &unit)),
        (EXECUTION_DEMAND_TABLE_V11, k(2, &(job, d.ordinal))),
        (EXECUTION_DEMAND_TABLE_V11, k(3, &id)),
        (EXECUTION_SERVED_TABLE_V11, k(0, &unit)),
    ] {
        builder.write_kernel_row(table, kk, None);
    }
    for attempt in 1u8..=2 {
        for index in 0u16..4 {
            builder.write_kernel_row(EXECUTION_SERVED_TABLE_V11, k(3, &(id, attempt, index)), None);
        }
    }
    Ok(())
}
pub(super) fn all_collected(r: &PalwKernelRouteStateV1, job: Digest) -> Result<bool, PalwStateV2Error> {
    let c = counts(r, job)?;
    Ok(c.open == 0 && c.collected == c.next)
}
