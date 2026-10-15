//! Explicit opt-in consent23/Fund24. Legacy mechanics18 never get physical or
//! complete-pursuit credit from the presence of this child module.
use super::*;
use crate::palw_flat_execution_capacity_v22::*;
use crate::palw_flat_execution_pursuit_v19::*;

fn plan_key(job: Digest) -> Vec<u8> {
    key(&(22u8, job, 1u8))
}
fn lease_key(job: Digest) -> Vec<u8> {
    key(&(22u8, job))
}
fn global_key() -> Vec<u8> {
    key(&(22u8, 0u8))
}
pub(super) fn quote(
    builder: &TransitionBuilder<'_>,
    class: &CheckedNativeExecutionClassV11,
    job: &CheckedJobQuotaPlanV11,
    p: &NativeExecutionPoliciesV11,
) -> Result<
    (
        NativePublicPursuitV19,
        NativeCarriagePlanV22,
        NativeCarriageServiceV22,
        crate::palw_native_carriage_v22::NativeCarriageMassParamsV22,
    ),
    PalwStateV2Error,
> {
    let e = builder.extras.kernel_route.as_ref().ok_or_else(|| refused("native23 configured network absent"))?;
    let opv = &e.opv.as_ref().ok_or_else(|| refused("native23 original OPV clocks absent"))?.policy;
    let mass = e.native_carriage_mass.clone().ok_or_else(|| refused("native23 original physical mass absent"))?;
    let service = e.native_carriage_service.clone().ok_or_else(|| refused("native23 configured physical service absent"))?;
    let ledger = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    let pursuit = derive_native_public_pursuit_v19(class, job, p, &ledger, opv).map_err(refused)?;
    if !pursuit.final_ready {
        return Err(refused("native23 original clock/request allowance cannot fund a complete public pursuit"));
    }
    let plan = derive_native_carriage_plan_v22(class, job, &pursuit, &service, &mass).map_err(refused)?;
    if plan.requires_source_parts {
        return Err(refused("native23 exact original source opening requires an unconnected protected source transport"));
    }
    Ok((pursuit, plan, service, mass))
}
pub(super) fn accept(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id = fixed_id(bytes)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw =
        r.aux.get(&(EXECUTION_PREPARATION_TABLE_V11, key(&id))).ok_or_else(|| refused("native23 original preparation absent"))?;
    if raw.len() > 8192 {
        return Err(refused("native23 bounded preparation extent"));
    }
    let prep: NativeExecutionPreparationV11 = borsh::from_slice(raw).map_err(|_| refused("native23 preparation framing"))?;
    if prep.id() != id || prep.record.producer != palw_kernel_bond_id_v1(signer) {
        return Err(refused("native23 only original producer may authorize"));
    }
    if r.aux.contains_key(&(EXECUTION_PREPARATION_TABLE_V11, key(&(23u8, id)))) {
        return Err(refused("native23 producer authorization exists"));
    }
    let work = class_work(builder, prep.record.class_id, bytes.len())?;
    if !prepay(builder, ctx, signer, work)? {
        return Ok(());
    }
    let class = checked_class(builder, ctx, p, prep.record.class_id, true)?;
    let job = prep.checked_plan(class.record()).map_err(refused)?;
    // All new expensive assumptions precede any implicit old18 authorization.
    let (pursuit, plan, service, _) = quote(builder, &class, &job, p)?;
    if !builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(EXECUTION_PREPARATION_TABLE_V11, key(&(18u8, id)))) {
        super::accept(builder, ctx, signer, p, bytes, true)?;
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let (root13, c13): (Digest, NativeExecutionConsentV13) = r
        .aux_row(EXECUTION_PREPARATION_TABLE_V11, &key(&(13u8, id)))
        .ok_or_else(|| refused("native23 original13 authorization absent"))?;
    let (root18, c18): (Digest, NativeExecutionConsentV18) = r
        .aux_row(EXECUTION_PREPARATION_TABLE_V11, &key(&(18u8, id)))
        .ok_or_else(|| refused("native23 original18 availability absent"))?;
    let bond = builder.state.bonds.get(signer).ok_or_else(|| refused("native23 active producer absent"))?;
    if c13.root() != root13
        || c18.root() != root18
        || c18.original_consent13_root != root13
        || c13.producer != *signer
        || c13.producer_operator != bond.operator_id.as_bytes()
        || c13.preparation != id
        || ctx.daa_score > c13.expires_daa
        || c13.policies_root != p.root()
        || c13.class_id != class.record().id()
        || c13.quota_plan_root != job.record().root()
    {
        return Err(refused("native23 differs from exact unconsumed original producer authorization"));
    }
    let c = NativeExecutionConsentV23 {
        version: 23,
        preparation: id,
        original_consent13: root13,
        original_availability18: root18,
        public_pursuit19: pursuit.root(),
        carriage_plan22: plan.root(),
        physical_service22: service.root(),
        slot_cooldown_terms: NativeExecutionConsentV23::cooldown_terms(),
    };
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(&(23u8, id)), Some(key(&(c.root(), c))))?;
    Ok(())
}
pub(super) struct NativeCapacityAdmissionV23 {
    global: NativeCarriageGlobalV22,
    lease: NativeCarriageLeaseV22,
    plan: NativeCarriagePlanV22,
    pursuit: NativePublicPursuitV19,
    consent: NativeExecutionConsentV23,
    queue: Option<(crate::palw_native_queue_v30::NativeQueueGlobalV30, crate::palw_native_queue_v30::NativeQueueLeaseV30)>,
}
pub(super) fn plan_fund(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV11,
    request: &FlatExecutionFundV11,
    class: &CheckedNativeExecutionClassV11,
    job: &CheckedJobQuotaPlanV11,
    job_id: Digest,
    root13: Digest,
    root18: Digest,
    final27: bool,
    funding_root: Digest,
    deadline: u64,
    now: u64,
) -> Result<NativeCapacityAdmissionV23, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw = r
        .aux
        .get(&(EXECUTION_PREPARATION_TABLE_V11, key(&(23u8, request.preparation))))
        .ok_or_else(|| refused("Fund24 requires original producer-signed complete public pursuit23"))?;
    if raw.len() != 64 + 2 + 7 * 64 {
        return Err(refused("native23 authorization fixed extent"));
    }
    let (root, c): (Digest, NativeExecutionConsentV23) =
        borsh::from_slice(raw).map_err(|_| refused("native23 authorization framing"))?;
    let (pursuit, plan, service, mass) = quote(builder, class, job, p)?;
    if c.version != 23
        || c.root() != root
        || c.preparation != request.preparation
        || c.original_consent13 != root13
        || c.original_availability18 != root18
        || c.public_pursuit19 != pursuit.root()
        || c.carriage_plan22 != plan.root()
        || c.physical_service22 != service.root()
        || c.slot_cooldown_terms != NativeExecutionConsentV23::cooldown_terms()
    {
        return Err(refused("Fund24 changes original pursuit/physical service/cooldown consent"));
    }
    if r.aux.contains_key(&(EXECUTION_QUOTA_TABLE_V11, lease_key(job_id))) {
        return Err(refused("native22 original physical lease exists"));
    }
    let mut global = match r.aux.get(&(EXECUTION_QUOTA_TABLE_V11, global_key())) {
        None => NativeCarriageGlobalV22::empty(&service),
        Some(raw) => {
            if raw.len() != 64 + 2 + 64 + 4 + 8 + 8 {
                return Err(refused("native22 shared global fixed extent"));
            }
            let (root, g): (Digest, NativeCarriageGlobalV22) =
                borsh::from_slice(raw).map_err(|_| refused("native22 shared global framing"))?;
            if g.root() != root {
                return Err(refused("native22 shared global root differs"));
            }
            g
        }
    };
    let (lease, queue) = if final27 {
        use crate::palw_native_queue_v30::*;
        let qp = &builder
            .extras
            .kernel_route
            .as_ref()
            .unwrap()
            .native_final
            .as_ref()
            .ok_or_else(|| refused("native Final27 trusted queue policy absent"))?
            .policy
            .queue_policy;
        let checked = CheckedNativeQueuePlanV30::derive_fixed(qp, class, job, &pursuit, &service, &mass).map_err(refused)?;
        let mut qglobal = match r.aux.get(&(EXECUTION_QUOTA_TABLE_V11, key(&(30u8, 0u8)))) {
            None => NativeQueueGlobalV30::empty(qp, &service).map_err(refused)?,
            Some(raw) => {
                if raw.len() > NATIVE_QUEUE_ROW_LIMIT_V30 {
                    return Err(refused("native queue30 fixed global cap"));
                }
                let (r, g): (Digest, NativeQueueGlobalV30) =
                    borsh::from_slice(raw).map_err(|_| refused("native queue30 global grammar"))?;
                if g.root() != r {
                    return Err(refused("native queue30 global root differs"));
                }
                g
            }
        };
        let (queue, lease) =
            qglobal.admit(&mut global, qp, &service, &mass, &checked, job_id, funding_root, root, now, deadline).map_err(refused)?;
        (lease, Some((qglobal, queue)))
    } else {
        (global.admit(&plan, &service, &mass, job_id, root).map_err(refused)?, None)
    };
    Ok(NativeCapacityAdmissionV23 { global, lease, plan, pursuit, consent: c, queue })
}
pub(super) fn write(builder: &mut TransitionBuilder<'_>, admission: NativeCapacityAdmissionV23, job: Digest) {
    if let Some((global, lease)) = admission.queue {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(30u8, 0u8)), Some(key(&(global.root(), global))));
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(30u8, job, 1u8)), Some(key(&(lease.root(), lease))));
    }
    let slots = NativePhysicalSlotsV22::empty(job, admission.consent.root());
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(22u8, job, 2u8)), Some(key(&(slots.root(), slots))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, global_key(), Some(key(&(admission.global.root(), admission.global))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, lease_key(job), Some(key(&(admission.lease.root(), admission.lease))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, plan_key(job), Some(key(&admission.plan)));
    builder.write_kernel_row(EXECUTION_POLICY_TABLE_V11, key(&(19u8, job)), Some(key(&admission.pursuit)));
    builder.write_kernel_row(EXECUTION_POLICY_TABLE_V11, key(&(23u8, job)), Some(key(&admission.consent)));
}

fn slots(builder: &TransitionBuilder<'_>, job: Digest) -> Result<Option<NativePhysicalSlotsV22>, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    if !r.aux.contains_key(&(EXECUTION_POLICY_TABLE_V11, key(&(23u8, job)))) {
        return Ok(None);
    }
    let c: NativeExecutionConsentV23 = r
        .aux_row(EXECUTION_POLICY_TABLE_V11, &key(&(23u8, job)))
        .ok_or_else(|| refused("native23 original physical consent framing"))?;
    let raw = r
        .aux
        .get(&(EXECUTION_QUOTA_TABLE_V11, key(&(22u8, job, 2u8))))
        .ok_or_else(|| refused("native23 original physical slotbook absent"))?;
    if raw.len() > 2048 {
        return Err(refused("native22 original physical slotbook fixed extent"));
    }
    let (root, s): (Digest, NativePhysicalSlotsV22) =
        borsh::from_slice(raw).map_err(|_| refused("native22 physical slotbook framing"))?;
    if s.version != 22 || s.root() != root || s.job != job || s.original_consent23 != c.root() {
        return Err(refused("native22 physical slotbook original identity differs"));
    }
    Ok(Some(s))
}
fn write_slots(builder: &mut TransitionBuilder<'_>, job: Digest, slots: Option<NativePhysicalSlotsV22>) {
    if let Some(s) = slots {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(22u8, job, 2u8)), Some(key(&(s.root(), s))));
    }
}
pub(super) fn plan_owner(
    builder: &TransitionBuilder<'_>,
    job: Digest,
    owner: Digest,
    horizon: u64,
) -> Result<Option<NativePhysicalSlotsV22>, PalwStateV2Error> {
    let mut slots = slots(builder, job)?;
    if let Some(s) = &mut slots {
        s.reserve_owner(owner, horizon).map_err(refused)?;
    }
    Ok(slots)
}
pub(super) fn plan_transfer(
    builder: &TransitionBuilder<'_>,
    job: Digest,
    old: Digest,
    new: Digest,
    horizon: u64,
) -> Result<Option<NativePhysicalSlotsV22>, PalwStateV2Error> {
    let mut slots = slots(builder, job)?;
    if let Some(s) = &mut slots {
        s.transfer_owner(old, new, horizon).map_err(refused)?;
    }
    Ok(slots)
}
pub(super) fn request(
    builder: &mut TransitionBuilder<'_>,
    job: Digest,
    lane: u8,
    owner: Digest,
    slot: u8,
    kind: u8,
    pending: Digest,
    now: u64,
    deadline: u64,
) -> Result<(), PalwStateV2Error> {
    let mut slots = slots(builder, job)?;
    if let Some(s) = &mut slots {
        s.reserve_request(lane, owner, slot, kind, pending, now, deadline).map_err(refused)?;
    }
    if slots.is_some() && kind == 1 {
        builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, key(&(22u8, pending)), Some(vec![0]))?;
    }
    write_slots(builder, job, slots);
    Ok(())
}
pub(super) fn commit_owner(builder: &mut TransitionBuilder<'_>, job: Digest, slots: Option<NativePhysicalSlotsV22>) {
    write_slots(builder, job, slots);
}
pub(super) fn debit_trace_attempt(builder: &mut TransitionBuilder<'_>, job: Digest, pending: Digest) -> Result<(), PalwStateV2Error> {
    if slots(builder, job)?.is_none() {
        return Ok(());
    }
    let raw = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(EXECUTION_DEMAND_TABLE_V11, key(&(22u8, pending))))
        .ok_or_else(|| refused("native23 original pending trace attempt counter absent"))?;
    if raw.len() != 1 || raw[0] >= 2 {
        return Err(refused("native23 original trace answer attempts exhausted"));
    }
    let next = raw[0] + 1;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, key(&(22u8, pending)), Some(vec![next]))?;
    Ok(())
}
pub(super) fn check_fresh_start(builder: &TransitionBuilder<'_>, job: Digest, now: u64, horizon: u64) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let Some(raw) = r.aux.get(&(EXECUTION_POLICY_TABLE_V11, key(&(23u8, job)))) else {
        return Ok(());
    };
    if raw.len() > 512 {
        return Err(refused("native23 bounded original pursuit consent"));
    }
    let c: NativeExecutionConsentV23 = borsh::from_slice(raw).map_err(|_| refused("native23 original pursuit consent framing"))?;
    let raw = r
        .aux
        .get(&(EXECUTION_POLICY_TABLE_V11, key(&(19u8, job))))
        .ok_or_else(|| refused("native23 cached original pursuit absent"))?;
    if raw.len() > 512 {
        return Err(refused("native19 original fixed pursuit extent"));
    }
    let p: NativePublicPursuitV19 = borsh::from_slice(raw).map_err(|_| refused("native19 original pursuit framing"))?;
    if p.root() != c.public_pursuit19 || !p.final_ready || p.latest_start(horizon).is_none_or(|cutoff| now > cutoff) {
        return Err(refused("native fresh owner lacks FULL original cold acquisition/locator/prove/maturity/carriage horizon"));
    }
    Ok(())
}

pub(super) fn plan_retire_owner(
    builder: &TransitionBuilder<'_>,
    job: Digest,
    owner: Digest,
    now: u64,
) -> Result<Option<NativePhysicalSlotsV22>, PalwStateV2Error> {
    let mut slots = slots(builder, job)?;
    if let Some(s) = &mut slots {
        s.retire_owner(owner, now).map_err(refused)?;
    }
    Ok(slots)
}
