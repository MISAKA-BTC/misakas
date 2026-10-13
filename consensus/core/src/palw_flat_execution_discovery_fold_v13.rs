//! Native paid pre-proof tickets share exactly the protected quota aggregate.
use super::*;
use misaka_palw_kernel::flat_quota11::discovery13::*;
const DISCOVERY_FIXED_BYTES: usize = 2 + 9 * 64 + 4 * 8 + 4 + 3 * 21 * 16;
fn native_key(id: &Digest) -> Vec<u8> {
    key(&(13u8, *id))
}
fn native_row(r: &PalwKernelRouteStateV1, id: &Digest) -> Result<NativeExecutionDiscoveryV13, PalwStateV2Error> {
    let bytes =
        r.aux.get(&(EXECUTION_SEAL_TABLE_V11, native_key(id))).ok_or_else(|| refused("native original discovery ticket absent"))?;
    if bytes.len() > 2048 {
        return Err(refused("native discovery original fixed framing"));
    }
    let row: NativeExecutionDiscoveryV13 = borsh::from_slice(bytes).map_err(|_| refused("native discovery original decode"))?;
    if row.key() != *id || row.native().record_root() != row.original_paid_root {
        return Err(refused("native discovery original identity"));
    }
    Ok(row)
}
fn shard(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV11,
    plan: &DiscoveryPlanV13,
    actor: Digest,
    job: Digest,
    id: Digest,
) -> Result<DiscoveryShardV13, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let g = read_rooted::<QuotaGlobalRowV11>(r, vec![0], QUOTA_GLOBAL_ROW_BYTES_V11 as usize)?
        .ok_or_else(|| refused("native discovery has no original global reserve"))?;
    let a = read_rooted::<QuotaActorRowV11>(r, quota_key(1, &actor), QUOTA_SELECTED_ACTOR_ROW_BYTES_V11 as usize)?;
    let s = read_rooted::<QuotaSubjectSummaryV11>(r, quota_key(2, &job), QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11 as usize)?
        .ok_or_else(|| refused("native discovery has no original producer reserve"))?;
    let d = read_rooted::<DiscoveryRowV13>(r, quota_key(4, &id), DISCOVERY_FIXED_BYTES)?;
    DiscoveryShardV13::from_native_rows(
        &p.discovery,
        &p.quota,
        plan,
        &g.1,
        &g.0,
        actor,
        a.as_ref().map(|(root, row)| (row, root)),
        (&s.1, &s.0),
        id,
        d.as_ref().map(|(root, row)| (row, root)),
    )
    .map_err(refused)
}
fn write(builder: &mut TransitionBuilder<'_>, s: &DiscoveryShardV13, id: Digest, job: Digest) {
    let g = s.global_row();
    let a = s.actor_row();
    let j = s.subject_row();
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, vec![0], Some(key(&(g.root(), g))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(1, &a.actor), Some(key(&(a.root(), a))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(2, &job), Some(key(&(j.row_root(), j))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(4, &id), s.discovery_row().map(|d| key(&(d.root(), d))));
}
pub(super) fn admit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    if bytes.len() != 128 {
        return Err(refused("native discovery admission has only job and canonical nonce"));
    }
    let id = fixed_id(&bytes[..64])?;
    let nonce = fixed_id(&bytes[64..])?;
    let h = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .execution_job_header_v11(&id)
        .ok_or_else(|| refused("native discovery job absent"))?;
    let work = class_work(builder, h.class_id, bytes.len())?;
    let fee = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?.dismissal_fee_v1(work);
    if fee == 0 {
        return Err(refused("native discovery requires an actual original paid debit"));
    }
    if !prepay(builder, ctx, signer, work)? {
        return Ok(());
    }
    let (_, class, book, job_plan) = original_job(builder, id)?;
    let actor = palw_kernel_bond_id_v1(signer);
    let operator = builder.state.bonds.get(signer).unwrap().operator_id.as_bytes();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let counter_key = key(&(13u8, operator));
    let counter: NativeRequesterNonceRowV11 =
        r.aux_row(EXECUTION_REQUEST_INDEX_TABLE_V11, &counter_key).unwrap_or(NativeRequesterNonceRowV11 { operator, next_nonce: 0 });
    if counter.operator != operator {
        return Err(refused("native discovery monotone counter namespace"));
    }
    let next = counter.advance(&nonce).map_err(refused)?;
    let claim = book.claim().ok_or_else(|| refused("native discovery no original claim"))?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let matures = ctx
        .daa_score
        .checked_add(class.record().horizons.seal_maturity)
        .ok_or_else(|| refused("native discovery maturity overflow"))?;
    let debit_receipt =
        object_id(b"misaka-palw/native/original-paid-discovery-debit/v13", &(ctx.block.as_bytes(), id, actor, operator, nonce, fee));
    let mut native = NativePaidDiscoveryV13 {
        job_id: id,
        funded_job_root: book.job().root(),
        book_root: book.root(),
        claim_id: claim.id(),
        vector_id: claim.vector,
        actor,
        actor_operator: operator,
        producer_operator: h.producer_operator,
        nonce,
        posted_daa: ctx.daa_score,
        matures_daa: matures,
        horizon_daa: claim.challenge_end,
        debit: fee,
        debit_receipt,
        original_root: [0; 64],
    };
    native.original_root = native.record_root();
    let ticket = native.key();
    if r.aux.contains_key(&(EXECUTION_SEAL_TABLE_V11, native_key(&ticket))) {
        return Err(refused("native discovery original ticket duplicate"));
    }
    let checked = check_native_paid_discovery_v13(&class, &book, &epoch, &native).map_err(refused)?;
    let plan = derive_discovery_plan_v13(&p.discovery, &p.quota, &job_plan).map_err(refused)?;
    let mut shard = shard(builder, p, &plan, actor, id, ticket)?;
    let global = shard.global_row().protected_seal_count as usize
        + r.flat_model_seal_count_v6() as usize
        + r.flat_model_seal_count_v7() as usize
        + r.flat_model_seal_count_v9() as usize;
    let total_actor = shard.actor_row().retained_protected_seals as usize
        + r.flat_model_seals_of_v6(signer).len()
        + r.flat_model_seals_of_v7(signer).len()
        + r.flat_model_seals_of_v9(signer).len();
    if global >= PROTECTED_SEALS_MAX_V11 as usize || total_actor >= PROTECTED_SEALS_PER_ACTOR_V11 as usize {
        return Err(refused("native discovery shared old/new seal slots full"));
    }
    let mut state = pursuit(r, &id)?;
    if state.retained_discovery_keys.len() + state.retained_seal_keys.len() >= PROTECTED_SEALS_PER_JOB_V11 as usize {
        return Err(refused("native discovery shared job slots full"));
    }
    shard.admit(&p.quota, &epoch, &plan, &checked).map_err(refused)?;
    let trace_plan = trace_plan(builder, p, id)?;
    let checked_trace = check_trace_discovery_v17(&class, &book, &epoch, &trace_plan, &p.discovery, &native).map_err(refused)?;
    let mut trace = trace_shard_from_rows(p, &trace_plan, shard.global_row(), shard.actor_row(), shard.subject_row(), None)?;
    trace.admit(&p.quota, &epoch, &trace_plan, &checked_trace).map_err(refused)?;
    let row = NativeExecutionDiscoveryV13 {
        job_id: id,
        funded_job_root: native.funded_job_root,
        book_root: native.book_root,
        claim_id: native.claim_id,
        vector: native.vector_id,
        actor,
        operator,
        producer_operator: h.producer_operator,
        nonce,
        posted_daa: ctx.daa_score,
        matures_daa: matures,
        horizon_daa: claim.challenge_end,
        original_debit: fee,
        original_debit_receipt: debit_receipt,
        original_paid_root: native.original_root,
        converted_to: None,
    };
    state.retained_discovery_keys.push(ticket);
    write(builder, &shard, ticket, id);
    write_trace(builder, &trace, 2, ticket);
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, native_key(&ticket), Some(key(&row)));
    builder.write_kernel_row(EXECUTION_REQUEST_INDEX_TABLE_V11, counter_key, Some(key(&next)));
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&id), Some(key(&state)));
    builder.note_kernel_bond(signer);
    Ok(())
}
pub(super) fn debit_request(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV11,
    actor: Digest,
    job: Digest,
    ticket: Digest,
    job_plan: &CheckedJobQuotaPlanV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    let native = native_row(builder.state.kernel_route.as_ref().unwrap(), &ticket)?;
    if native.actor != actor || native.job_id != job || native.converted_to.is_some() {
        return Err(refused("native paid discovery original entitlement unavailable"));
    }
    let plan = derive_discovery_plan_v13(&p.discovery, &p.quota, job_plan).map_err(refused)?;
    let mut shard = shard(builder, p, &plan, actor, job, ticket)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    shard.debit_request(&p.quota, &epoch, &plan, job_plan, actor, bytes).map_err(refused)?;
    write(builder, &shard, ticket, job);
    Ok(())
}
pub(super) fn check_request_owner(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    actor: Digest,
    job: Digest,
    ticket: Digest,
) -> Result<(), PalwStateV2Error> {
    let n = native_row(builder.state.kernel_route.as_ref().unwrap(), &ticket)?;
    if n.actor != actor
        || n.job_id != job
        || n.converted_to.is_some()
        || ctx.daa_score < n.matures_daa
        || ctx.daa_score > n.horizon_daa
    {
        return Err(refused("native paid discovery original owner/window differs"));
    }
    Ok(())
}
pub(super) fn convert(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    if bytes.len() != 192 {
        return Err(refused("native discovery conversion exact ticket/proof/seal"));
    }
    let ticket = fixed_id(&bytes[..64])?;
    let proof = fixed_id(&bytes[64..128])?;
    let sealed = fixed_id(&bytes[128..])?;
    let mut original = native_row(builder.state.kernel_route.as_ref().unwrap(), &ticket)?;
    let actor = palw_kernel_bond_id_v1(signer);
    if original.actor != actor || original.converted_to.is_some() {
        return Err(refused("native original discovery ticket converted or actor differs"));
    }
    let id = original.job_id;
    let (_, class, book, job_plan) = original_job(builder, id)?;
    if original.funded_job_root != book.job().root() || original.book_root != book.root() {
        return Err(refused("native conversion original book differs"));
    }
    let plan = derive_discovery_plan_v13(&p.discovery, &p.quota, &job_plan).map_err(refused)?;
    let mut shard = shard(builder, p, &plan, actor, id, ticket)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let maturity =
        ctx.daa_score.checked_add(class.record().horizons.seal_maturity).ok_or_else(|| refused("native converted proof maturity"))?;
    let reveal_until =
        ctx.daa_score.checked_add(class.record().horizons.seal_ttl).ok_or_else(|| refused("native converted proof reveal"))?;
    let horizon =
        reveal_until.checked_add(class.record().horizons.proof_grace).ok_or_else(|| refused("native converted proof horizon"))?;
    let mut native = NativePaidSealAnchorV11 {
        job_id: id,
        funded_job_root: original.funded_job_root,
        book_root: original.book_root,
        claim_id: original.claim_id,
        vector_id: original.vector,
        seal_actor: actor,
        seal_operator_id: original.operator,
        producer_operator_id: original.producer_operator,
        proof_root: proof,
        posted_daa: ctx.daa_score,
        matures_daa: maturity,
        horizon_daa: horizon,
        original_paid_seal_root: [0; 64],
        original_debit: original.original_debit,
        original_debit_receipt: original.original_debit_receipt,
        shared_live_seals: shard.global_row().protected_seal_count,
        shared_actor_live_seals: shard.actor_row().retained_protected_seals,
    };
    native.original_paid_seal_root = native_paid_seal_record_root_v11(&native);
    let seal_key =
        ProtectedSealKeyV11 { job_id: id, vector_id: original.vector, seal_actor: actor, proof_root: proof, horizon_daa: horizon }
            .root();
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.aux.contains_key(&(EXECUTION_SEAL_TABLE_V11, key(&seal_key)))
        || r.aux.contains_key(&(EXECUTION_QUOTA_TABLE_V11, quota_key(3, &seal_key)))
    {
        return Err(refused("native conversion original proof key occupied"));
    }
    let mut state = pursuit(r, &id)?;
    if !state.retained_discovery_keys.contains(&ticket) {
        return Err(refused("native converted ticket lost original retained index"));
    }
    let summary = shard.convert(&p.quota, &epoch, &plan, &class, &book, &native).map_err(refused)?;
    let trace_plan = trace_plan(builder, p, id)?;
    let checked_trace = check_trace_seal_v17(&class, &book, &epoch, &trace_plan, &native).map_err(refused)?;
    let mut trace = trace_shard(builder, p, &trace_plan, actor, id, 2, ticket)?;
    trace.handoff_discovery(&p.quota, &epoch, &trace_plan, &ticket, &checked_trace).map_err(refused)?;
    let seal = NativeExecutionSealV11 {
        version: 11,
        job_id: id,
        vector: original.vector,
        claim_id: original.claim_id,
        original_book_root: original.book_root,
        funded_job_root: original.funded_job_root,
        bond: *signer,
        actor,
        operator: original.operator,
        producer_operator: original.producer_operator,
        class_scope9_root: class.record().class_scope9_root,
        proof_root: proof,
        seal: sealed,
        posted_daa: ctx.daa_score,
        mature_daa: maturity,
        reveal_until,
        horizon_daa: horizon,
        original_paid_debit: original.original_debit,
        original_paid_receipt: original.original_debit_receipt,
        original_paid_record_root: native.original_paid_seal_root,
        spent: false,
    };
    original.converted_to = Some(seal_key);
    state.retained_discovery_keys.retain(|k| *k != ticket);
    state.retained_seal_keys.push(seal_key);
    state.required_retained_until = state.required_retained_until.max(horizon);
    write(builder, &shard, ticket, id);
    write_trace(builder, &trace, 1, seal_key);
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, trace_row_key(2, ticket), None);
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(3, &seal_key), Some(key(&(summary.row_root(), summary))));
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, key(&seal_key), Some(key(&seal)));
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, native_key(&ticket), Some(key(&original)));
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&id), Some(key(&state)));
    Ok(())
}
