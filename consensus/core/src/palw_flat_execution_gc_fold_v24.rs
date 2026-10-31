//! Bounded, caller-priced cleanup of genuine original native terminals.
//! Each demand step removes exactly one authenticated ordinal, including the
//! actual served payload/journal. Historical guarantees are never refunded.
use super::*;
use misaka_palw_kernel::flat_quota11::source16::*;

fn charge_removed(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV11,
    signer: &PalwBondKeyV2,
    job: Digest,
    removed: u64,
) -> Result<bool, PalwStateV2Error> {
    use misaka_palw_kernel::flat_resources10::ResourceAxisV10 as A;
    // All source answers/proof carriers have original caps; the count comes
    // only from retained native row lengths, not an action's price field.
    let bytes = removed
        .checked_add(16 << 10)
        .filter(|n| *n <= 4 << 20)
        .ok_or_else(|| refused("native GC original row/journal extent overflow"))?;
    let work = crate::palw_model_artifact_v2::model_artifact_work_v2(0, bytes as usize)
        .ok_or_else(|| refused("native GC actual removed metadata tariff overflow"))?;
    if !prepay(builder, ctx, signer, work)? {
        return Ok(false);
    }
    let b = bytes as u128;
    let mut q = QuotaQuantitiesV11::default();
    for (axis, n) in [
        (A::ScalarSteps, b * 16),
        (A::IndexSteps, b * 16),
        (A::ValidatedElements, b),
        (A::CopiedReferenceBytes, b * 3),
        (A::TraceHashBytes, b * 3),
        (A::ContainerAllocatorAllowance, b + (16 << 10)),
        (A::StoredEvidenceBytes, b),
        (A::DeliveryBytes, b),
        (A::ReplyCalls, 1),
        (A::Attempts, 1),
        (A::MetadataWork, work as u128),
        (A::OwnedPeakBytes, b * 4 + (16 << 10)),
        (A::RawParameterStorage, b),
        (A::ParameterTreeScratch, b * 3),
    ] {
        q.0[axis as usize] = n;
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = quota_shard(builder, p, &epoch, palw_kernel_bond_id_v1(signer), job)?;
    shard.debit_ordinary(&p.quota, &epoch, &DerivedOrdinaryMetadataV11::from_native_envelope(q)).map_err(refused)?;
    write_shard(builder, &shard, job)?;
    Ok(true)
}
fn terminal_header(builder: &TransitionBuilder<'_>, job: Digest, now: u64) -> Result<NativeExecutionJobHeaderV11, PalwStateV2Error> {
    let h = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .execution_job_header_v11(&job)
        .ok_or_else(|| refused("native GC original terminal job absent"))?;
    if matches!(h.terminal, ExecutionTerminalV11::Funded | ExecutionTerminalV11::Challengeable) || now <= h.deadline_daa {
        return Err(refused("native GC original job is live or complete horizon has not expired"));
    }
    // RFC08 08-B: a Final whose fee is still held stays accusable to its liability horizon; its material is not collected.
    if super::super::palw_native_liability_hold_fold_v1::held_native_fee_v1(
        &builder.state,
        crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_FUNDED_V11,
        &job,
    )
    .is_some()
    {
        return Err(refused("native GC original Final's fee is held to its liability horizon"));
    }
    let state = pursuit(builder.state.kernel_route.as_ref().unwrap(), &job)?;
    if state.open_demands != 0 || now <= state.required_retained_until {
        return Err(refused("native GC original pending/delivery retention remains"));
    }
    Ok(h)
}
fn demand_step(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    job: Digest,
) -> Result<(), PalwStateV2Error> {
    terminal_header(builder, job, ctx.daa_score)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut counts = da::counters(r, job)?;
    if counts.collected_ordinal >= counts.next_ordinal {
        return Err(refused("native indexed demand cleanup already complete"));
    }
    let index = key(&(14u8, job, counts.collected_ordinal));
    let raw = r
        .aux
        .get(&(EXECUTION_DEMAND_COUNT_TABLE_V11, index.clone()))
        .ok_or_else(|| refused("native original demand ordinal absent"))?;
    let (pending_key, served_key, slot_key, slot_value, pending_id) = match raw.len() {
        64 => {
            let id = Hash64::from_bytes(fixed_id(raw)?);
            let d = r.execution_demand_v11(&id).ok_or_else(|| refused("native original indexed trace absent"))?;
            if d.job_id != job || !d.body.settled {
                return Err(refused("native indexed trace is another job/live"));
            }
            (key(&id), d.body.unit.key(&d.body.vector), key(&(d.body.requester, d.body.slot)), key(&id), id.as_bytes())
        }
        65 if raw[0] == 16 => {
            let id = fixed_id(&raw[1..])?;
            let d = source::demand(r, id)?;
            if d.request.job != job || !d.settled {
                return Err(refused("native indexed source is another job/live"));
            }
            (key(&(16u8, id)), key(&(16u8, source::unit_key(&d.request))), key(&(16u8, d.requester, d.request.slot)), key(&id), id)
        }
        _ => return Err(refused("native original demand ordinal grammar")),
    };
    let removed = r
        .aux
        .get(&(EXECUTION_DEMAND_TABLE_V11, pending_key.clone()))
        .map_or(0, |v| v.len())
        .checked_add(r.aux.get(&(EXECUTION_SERVED_TABLE_V11, served_key.clone())).map_or(0, |v| v.len()))
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(|| refused("native indexed removed payload extent overflow"))?;
    let clear_slot = r.aux.get(&(EXECUTION_DEMAND_SLOT_TABLE_V11, slot_key.clone())).is_some_and(|v| *v == slot_value);
    if !charge_removed(builder, ctx, p, signer, job, removed as u64)? {
        return Ok(());
    }
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, pending_key, None)?;
    builder.write_kernel_row(EXECUTION_DEMAND_TABLE_V11, key(&(22u8, pending_id)), None)?;
    builder.write_kernel_row(EXECUTION_SERVED_TABLE_V11, served_key, None)?;
    if clear_slot {
        builder.write_kernel_row(EXECUTION_DEMAND_SLOT_TABLE_V11, slot_key, None)?;
    }
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, index, None)?;
    counts.collected_ordinal += 1;
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&(13u8, job)), Some(key(&counts)))?;
    Ok(())
}
#[derive(borsh::BorshSerialize)]
struct NativeCollectedV24 {
    version: u16,
    job: Digest,
    class: Digest,
    funded: Digest,
    terminal: Digest,
    operator: Digest,
    nonce: Digest,
    credit: Digest,
    source_retired: Digest,
    trace_retired: Digest,
    collected_daa: u64,
}
fn job_step(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    job: Digest,
) -> Result<(), PalwStateV2Error> {
    let h = terminal_header(builder, job, ctx.daa_score)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let state = pursuit(r, &job)?;
    let counts = da::counters(r, job)?;
    if !state.retained_seal_keys.is_empty()
        || !state.retained_discovery_keys.is_empty()
        || counts.collected_ordinal != counts.next_ordinal
    {
        return Err(refused("native GC original retained proofs/tickets/indexed answers remain"));
    }
    if !prepay(builder, ctx, signer, class_work(builder, h.class_id, 129 + h.book_bytes_len as usize + h.prompt_len as usize * 4)?)? {
        return Ok(());
    }
    let (row, class, book, plan) = original_job(builder, job)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let credit: NativeExecutionCreditV11 =
        r.aux_row(EXECUTION_CREDIT_TABLE_V11, &key(&job)).ok_or_else(|| refused("native GC genuine original settlement absent"))?;
    if credit.job_id != job
        || credit.original_funded_job_root != h.funded_job_root
        || credit.terminal_book_root != book.root()
        || book.escrow_remaining() != 0
    {
        return Err(refused("native GC original settlement/book/coins differ"));
    }
    let counter = r.execution_nonce_counter_v11(&h.requester_operator).map_err(refused)?;
    let terminal = check_native_quota_terminal_v11(
        &class,
        &book,
        &NativeQuotaTerminalAnchorV11 {
            job_id: job,
            funded_job_root: h.funded_job_root,
            terminal_book_root: book.root(),
            request_key: book.job().request_key(),
            nonce_witness: nonce_witness(book.job().requester, &counter),
        },
    )
    .map_err(refused)?;
    let (source_root, source) = read_rooted::<SourceObligationV16>(r, key(&(16u8, job)), SOURCE_ROW_BYTES_V16 as usize)?
        .ok_or_else(|| refused("native GC original source obligation absent"))?;
    let (trace_root, trace) = read_rooted::<TraceSupplementV17>(r, trace_row_key(0, job), TRACE_ROW_BYTES_V17 as usize)?
        .ok_or_else(|| refused("native GC original producer trace obligation absent"))?;
    let raw = r
        .aux
        .get(&(EXECUTION_QUOTA_TABLE_V11, key(&(16u8, job, 1u8))))
        .ok_or_else(|| refused("native GC original source quote absent"))?;
    let record = SourcePlanRecordV16::from_rooted_bytes(raw, &source.plan_root).map_err(refused)?;
    let source_plan = SourcePlanV16::restore(&record, &source.plan_root, &p.quota, &h.quota_plan_root).map_err(refused)?;
    let trace_plan = trace_plan(builder, p, job)?;
    if source.original_consent_root != trace.original_paid_root || trace.plan_root != trace_plan.root() {
        return Err(refused("native GC original source/trace authorization differs"));
    }
    let capacity_release = if r.aux.contains_key(&(EXECUTION_POLICY_TABLE_V11, key(&(23u8, job)))) {
        use crate::palw_flat_execution_capacity_v22::*;
        let (lease_root, mut lease): (Digest, NativeCarriageLeaseV22) = r
            .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(22u8, job)))
            .ok_or_else(|| refused("native GC original physical lease absent"))?;
        let (global_root, mut global): (Digest, NativeCarriageGlobalV22) = r
            .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(22u8, 0u8)))
            .ok_or_else(|| refused("native GC original physical global absent"))?;
        let (slot_root, slots): (Digest, NativePhysicalSlotsV22) = r
            .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(22u8, job, 2u8)))
            .ok_or_else(|| refused("native GC original physical streams absent"))?;
        let c: NativeExecutionConsentV23 =
            r.aux_row(EXECUTION_POLICY_TABLE_V11, &key(&(23u8, job))).ok_or_else(|| refused("native GC original23 consent absent"))?;
        if lease.root() != lease_root
            || global.root() != global_root
            || slots.root() != slot_root
            || lease.original_job != job
            || lease.original_consent != c.root()
            || global.service != c.physical_service22
            || slots.original_consent23 != c.root()
            || slots.job != job
            || slots.owners.iter().any(|v| *v != [0; 64])
            || slots.not_before.iter().any(|d| ctx.daa_score <= *d)
        {
            return Err(refused("native GC physical owner/cooldown/original consent remains or differs"));
        }
        let queue = if r.aux.contains_key(&(EXECUTION_POLICY_TABLE_V11, key(&(27u8, job)))) {
            use crate::palw_native_queue_v30::*;
            let qp = &builder
                .extras
                .kernel_route
                .as_ref()
                .and_then(|e| e.native_final.as_ref())
                .ok_or_else(|| refused("native queue30 trusted original policy absent"))?
                .policy
                .queue_policy;
            let qraw = r
                .aux
                .get(&(EXECUTION_QUOTA_TABLE_V11, key(&(30u8, 0u8))))
                .ok_or_else(|| refused("native GC original queue30 global absent"))?;
            if qraw.len() > NATIVE_QUEUE_ROW_LIMIT_V30 {
                return Err(refused("native GC queue30 global cap"));
            }
            let (qr, mut q): (Digest, NativeQueueGlobalV30) =
                borsh::from_slice(qraw).map_err(|_| refused("native GC queue30 global grammar"))?;
            let (lr, mut l): (Digest, NativeQueueLeaseV30) = r
                .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(30u8, job, 1u8)))
                .ok_or_else(|| refused("native GC original queue30 lease absent"))?;
            if q.root() != qr || l.root() != lr {
                return Err(refused("native GC original queue30 roots differ"));
            }
            let closed = CheckedNativeQueueClosureV30::from_current_native(
                job,
                h.funded_job_root,
                lr,
                lease_root,
                book.root(),
                ctx.daa_score,
                false,
            )
            .map_err(refused)?;
            q.retire(&mut global, qp, &mut l, &mut lease, &closed).map_err(refused)?;
            Some(q)
        } else {
            if r.aux.contains_key(&(EXECUTION_QUOTA_TABLE_V11, key(&(30u8, job, 1u8)))) {
                return Err(refused("native GC old23 cannot consume unowned queue30 lease"));
            }
            global.retire(&mut lease, lease_root).map_err(refused)?;
            None
        };
        Some((global, queue))
    } else {
        None
    };
    let mut requester = bounded_ids(r, EXECUTION_JOB_OWNER_TABLE_V11, &h.requester, EXECUTION_JOBS_PER_REQUESTER_V11)?;
    let mut producer = bounded_ids_key(r, EXECUTION_JOB_OWNER_TABLE_V11, key(&(1u8, h.producer)), EXECUTION_JOBS_PER_REQUESTER_V11)?;
    if !requester.contains(&job) || !producer.contains(&job) {
        return Err(refused("native GC original catalog membership absent"));
    }
    let post_bytes = r.aux.get(&(EXECUTION_PROTECTED_TABLE_V11, key(&job))).map_or(0, Vec::len);
    let count: u32 = r.aux_row(EXECUTION_JOB_COUNT_TABLE_V11, &[]).ok_or_else(|| refused("native GC original live count absent"))?;
    if count == 0 {
        return Err(refused("native GC live count underflow"));
    }
    if !charge_removed(builder, ctx, p, signer, job, (row.book_bytes.len() + row.prompt.len() * 4 + post_bytes + 8192) as u64)? {
        return Ok(());
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = quota_shard(builder, p, &epoch, palw_kernel_bond_id_v1(signer), job)?;
    let source_retired =
        retire_expired_source_v16(&mut shard, &p.quota, &epoch, &source_plan, &source, &source_root, &terminal).map_err(refused)?;
    let trace_retired =
        retire_expired_trace_v17(&mut shard, &p.quota, &epoch, &trace_plan, &trace, &trace_root, &terminal).map_err(refused)?;
    shard.collect_closed_expired(&p.quota, &epoch, &terminal).map_err(refused)?;
    requester.retain(|v| *v != job);
    producer.retain(|v| *v != job);
    let tomb = NativeCollectedV24 {
        version: 24,
        job,
        class: h.class_id,
        funded: h.funded_job_root,
        terminal: book.root(),
        operator: h.requester_operator,
        nonce: book.job().nonce,
        credit: object_id(b"misaka-palw/native/original-collected-credit/v24", &credit),
        source_retired: source_retired.root(),
        trace_retired: trace_retired.root(),
        collected_daa: ctx.daa_score,
    };
    write_shard(builder, &shard, job)?;
    if let Some((global, queue)) = capacity_release {
        if let Some(q) = queue {
            builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(30u8, 0u8)), Some(key(&(q.root(), q))))?;
            builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(30u8, job, 1u8)), None)?;
        }
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(22u8, 0u8)), Some(key(&(global.root(), global))))?;
        for k in [key(&(22u8, job)), key(&(22u8, job, 1u8)), key(&(22u8, job, 2u8))] {
            builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, k, None)?;
        }
    }
    builder.write_kernel_row(EXECUTION_JOB_OWNER_TABLE_V11, key(&h.requester), Some(key(&requester)))?;
    builder.write_kernel_row(EXECUTION_JOB_OWNER_TABLE_V11, key(&(1u8, h.producer)), Some(key(&producer)))?;
    builder.write_kernel_row(EXECUTION_JOB_COUNT_TABLE_V11, Vec::new(), Some(key(&(count - 1))))?;
    builder.write_kernel_row(EXECUTION_CREDIT_TABLE_V11, key(&(24u8, job)), Some(key(&tomb)))?;
    for k in [quota_key(2, &job), key(&(16u8, job)), key(&(16u8, job, 1u8)), trace_row_key(0, job)] {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, k, None)?;
    }
    for (table, k) in [
        (EXECUTION_JOB_TABLE_V11, key(&job)),
        (EXECUTION_PROTECTED_TABLE_V11, key(&job)),
        (EXECUTION_DEMAND_COUNT_TABLE_V11, key(&job)),
        (EXECUTION_DEMAND_COUNT_TABLE_V11, key(&(13u8, job))),
    ] {
        builder.write_kernel_row(table, k, None)?;
    }
    if let Some(claim) = book.claim() {
        builder.write_kernel_row(EXECUTION_VECTOR_JOB_TABLE_V11, key(&claim.vector), None)?;
    }
    let _ = plan;
    Ok(())
}
fn proof_step(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    job: Digest,
    id: Digest,
) -> Result<(), PalwStateV2Error> {
    let h = terminal_header(builder, job, ctx.daa_score)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let own = court::seal_row(r, &id)?;
    if own.job_id != job || ctx.daa_score <= own.horizon_daa {
        return Err(refused("native proof GC original owner/job/horizon differs"));
    }
    let mut retained = pursuit(r, &job)?;
    if !retained.retained_seal_keys.contains(&id) {
        return Err(refused("native proof GC original retained index absent"));
    }
    let mut removed = 8192u64;
    for index in 0u8..64 {
        if let Some(part) = r.aux.get(&(EXECUTION_SEAL_TABLE_V11, key(&(15u8, id, index)))) {
            if part.len() > EXECUTION_FRAGMENT_MAX_BYTES_V11 {
                return Err(refused("native proof GC original fragment cap differs"));
            }
            removed = removed.checked_add(part.len() as u64).ok_or_else(|| refused("native proof GC journal overflow"))?;
        }
    }
    if !prepay(builder, ctx, signer, class_work(builder, h.class_id, h.book_bytes_len as usize + h.prompt_len as usize * 4)?)? {
        return Ok(());
    }
    if !charge_removed(builder, ctx, p, signer, job, removed)? {
        return Ok(());
    }
    let (_, class, book, _) = original_job(builder, job)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let credit: NativeExecutionCreditV11 =
        r.aux_row(EXECUTION_CREDIT_TABLE_V11, &key(&job)).ok_or_else(|| refused("native proof GC original settlement absent"))?;
    if credit.job_id != job || credit.terminal_book_root != book.root() || credit.original_funded_job_root != h.funded_job_root {
        return Err(refused("native proof GC original collected terminal credit differs"));
    }
    let counter = r.execution_nonce_counter_v11(&h.requester_operator).map_err(refused)?;
    let terminal = check_native_quota_terminal_v11(
        &class,
        &book,
        &NativeQuotaTerminalAnchorV11 {
            job_id: job,
            funded_job_root: h.funded_job_root,
            terminal_book_root: book.root(),
            request_key: book.job().request_key(),
            nonce_witness: nonce_witness(book.job().requester, &counter),
        },
    )
    .map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let plan = trace_plan(builder, p, job)?;
    let (trace_root, trace) = read_rooted::<TraceSupplementV17>(r, trace_row_key(1, id), TRACE_ROW_BYTES_V17 as usize)?
        .ok_or_else(|| refused("native proof GC original trace supplement absent"))?;
    if trace.role != 1
        || trace.key != id
        || trace.actor != own.actor
        || trace.original_paid_root != own.original_paid_record_root
        || trace.horizon != own.horizon_daa
    {
        return Err(refused("native proof GC original supplement lineage differs"));
    }
    let actor = palw_kernel_bond_id_v1(signer);
    let mut caller = quota_shard(builder, p, &epoch, actor, job)?;
    let mut owner_projection = quota_shard_seal(builder, p, &epoch, own.actor, job, Some(id))?;
    let seal = owner_projection.seal_row().ok_or_else(|| refused("native proof GC original protected row absent"))?.clone();
    if seal.original_paid_seal_root != own.original_paid_record_root {
        return Err(refused("native proof GC original paid root differs"));
    }
    let owner = if actor == own.actor {
        owner_projection.collect_expired_protected_seal(&p.quota, &epoch, &terminal).map_err(refused)?;
        caller = owner_projection;
        None
    } else {
        let owner = owner_projection.actor_row();
        Some(
            caller
                .collect_expired_protected_seal_for(&p.quota, &epoch, &terminal, &seal, &seal.row_root(), &owner, &owner.root())
                .map_err(refused)?
                .1,
        )
    };
    let retired = retire_expired_trace_v17(&mut caller, &p.quota, &epoch, &plan, &trace, &trace_root, &terminal).map_err(refused)?;
    let physical = capacity::plan_retire_owner(builder, job, id, ctx.daa_score)?;
    retained.retained_seal_keys.retain(|v| *v != id);
    write_shard(builder, &caller, job)?;
    if let Some(owner) = owner {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(1, &owner.actor), Some(key(&(owner.root(), owner))))?;
    }
    for k in [quota_key(3, &id), trace_row_key(1, id)] {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, k, None)?;
    }
    for k in [key(&id), key(&(12u8, id))] {
        builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, k, None)?;
    }
    for index in 0u8..64 {
        builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, key(&(15u8, id, index)), None)?;
    }
    builder.write_kernel_row(
        EXECUTION_CREDIT_TABLE_V11,
        key(&(24u8, job, id)),
        Some(key(&(own.original_paid_record_root, retired.root(), ctx.daa_score))),
    )?;
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&job), Some(key(&retained)))?;
    capacity::commit_owner(builder, job, physical)?;
    Ok(())
}
fn discovery_step(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    job: Digest,
    id: Digest,
) -> Result<(), PalwStateV2Error> {
    use misaka_palw_kernel::flat_quota11::discovery13::*;
    let h = terminal_header(builder, job, ctx.daa_score)?;
    let own = discovery::native_row(builder.state.kernel_route.as_ref().unwrap(), &id)?;
    if own.job_id != job || own.converted_to.is_some() || ctx.daa_score <= own.horizon_daa {
        return Err(refused("native discovery GC original live/converted ticket differs"));
    }
    let mut retained = pursuit(builder.state.kernel_route.as_ref().unwrap(), &job)?;
    if !retained.retained_discovery_keys.contains(&id) {
        return Err(refused("native discovery GC retained original index absent"));
    }
    if !prepay(builder, ctx, signer, class_work(builder, h.class_id, h.book_bytes_len as usize + h.prompt_len as usize * 4)?)? {
        return Ok(());
    }
    if !charge_removed(builder, ctx, p, signer, job, 8192)? {
        return Ok(());
    }
    let (_, class, book, job_plan) = original_job(builder, job)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let credit: NativeExecutionCreditV11 =
        r.aux_row(EXECUTION_CREDIT_TABLE_V11, &key(&job)).ok_or_else(|| refused("native discovery GC settlement absent"))?;
    if credit.terminal_book_root != book.root() || credit.original_funded_job_root != h.funded_job_root {
        return Err(refused("native discovery GC original credit differs"));
    }
    let counter = r.execution_nonce_counter_v11(&h.requester_operator).map_err(refused)?;
    let terminal = check_native_quota_terminal_v11(
        &class,
        &book,
        &NativeQuotaTerminalAnchorV11 {
            job_id: job,
            funded_job_root: h.funded_job_root,
            terminal_book_root: book.root(),
            request_key: book.job().request_key(),
            nonce_witness: nonce_witness(book.job().requester, &counter),
        },
    )
    .map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let dplan = derive_discovery_plan_v13(&p.discovery, &p.quota, &job_plan).map_err(refused)?;
    let mut selected = discovery::shard(builder, p, &dplan, own.actor, job, id)?;
    let actor = palw_kernel_bond_id_v1(signer);
    let caller = quota_shard(builder, p, &epoch, actor, job)?;
    let (global, payer, owner, subject) = if actor == own.actor {
        selected.retire_expired(&p.quota, &epoch, &dplan).map_err(refused)?;
        (selected.global_row().clone(), selected.actor_row().clone(), None, selected.subject_row().clone())
    } else {
        let retired = selected.retire_expired_for(&caller, &p.quota, &epoch, &dplan, &terminal).map_err(refused)?;
        (retired.global, retired.payer, Some(retired.owner), retired.subject)
    };
    let tplan = trace_plan(builder, p, job)?;
    let (troot, trace) = read_rooted::<TraceSupplementV17>(r, trace_row_key(2, id), TRACE_ROW_BYTES_V17 as usize)?
        .ok_or_else(|| refused("native discovery GC original supplement absent"))?;
    if trace.role != 2 || trace.key != id || trace.actor != own.actor || trace.original_paid_root != own.original_paid_root {
        return Err(refused("native discovery GC original supplement owner differs"));
    }
    let mut merged = CheckedQuotaShardV11::from_native_rows(
        &p.quota,
        &global,
        &global.root(),
        actor,
        job,
        None,
        Some((&payer, &payer.root())),
        Some((&subject, &subject.row_root())),
        None,
    )
    .map_err(refused)?;
    let retired = retire_expired_trace_v17(&mut merged, &p.quota, &epoch, &tplan, &trace, &troot, &terminal).map_err(refused)?;
    let physical = capacity::plan_retire_owner(builder, job, id, ctx.daa_score)?;
    retained.retained_discovery_keys.retain(|v| *v != id);
    write_shard(builder, &merged, job)?;
    if let Some(owner) = owner {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(1, &owner.actor), Some(key(&(owner.root(), owner))))?;
    }
    for k in [quota_key(4, &id), trace_row_key(2, id)] {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, k, None)?;
    }
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, key(&(13u8, id)), None)?;
    builder.write_kernel_row(
        EXECUTION_CREDIT_TABLE_V11,
        key(&(24u8, job, id)),
        Some(key(&(own.original_paid_root, retired.root(), ctx.daa_score))),
    )?;
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&job), Some(key(&retained)))?;
    capacity::commit_owner(builder, job, physical)?;
    Ok(())
}
pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    if bytes.len() != 129 {
        return Err(refused("native GC exact kind/job/item frame"));
    }
    let job = fixed_id(&bytes[1..65])?;
    let item = fixed_id(&bytes[65..])?;
    if bytes[0] < 2 && item != [0; 64] {
        return Err(refused("native indexed/job GC has no caller-selected price or target"));
    }
    match bytes[0] {
        0 => demand_step(builder, ctx, signer, p, job),
        1 => job_step(builder, ctx, signer, p, job),
        2 => proof_step(builder, ctx, signer, p, job, item),
        3 => discovery_step(builder, ctx, signer, p, job, item),
        _ => Err(refused("native GC unknown typed cleanup kind")),
    }
}
