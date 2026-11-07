//! Original paid own-proof slots and exact funded computation/source courts.
use super::*;
pub(super) fn seal_row(route: &PalwKernelRouteStateV1, id: &Digest) -> Result<NativeExecutionSealV11, PalwStateV2Error> {
    let bytes = route.aux.get(&(EXECUTION_SEAL_TABLE_V11, key(id))).ok_or_else(|| refused("native paid own seal absent"))?;
    if bytes.len() > 2048 {
        return Err(refused("native own seal fixed extent"));
    }
    let row: NativeExecutionSealV11 = borsh::from_slice(bytes).map_err(|_| refused("native own seal decode"))?;
    let k = ProtectedSealKeyV11 {
        job_id: row.job_id,
        vector_id: row.vector,
        seal_actor: row.actor,
        proof_root: row.proof_root,
        horizon_daa: row.horizon_daa,
    };
    if row.checked_original_paid_root().is_err()
        || row.version != 11
        || k.root() != *id
        || palw_kernel_bond_id_v1(&row.bond) != row.actor
    {
        return Err(refused("native own seal namespace"));
    }
    Ok(row)
}
pub(super) fn seal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    if bytes.len() != 192 {
        return Err(refused("native own seal exact frame"));
    }
    let id = fixed_id(&bytes[..64])?;
    let proof_root = fixed_id(&bytes[64..128])?;
    let seal = fixed_id(&bytes[128..])?;
    let h = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .execution_job_header_v11(&id)
        .ok_or_else(|| refused("native sealed job absent"))?;
    let work = class_work(builder, h.class_id, bytes.len())?;
    let fee = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?.dismissal_fee_v1(work);
    if fee == 0 {
        return Err(refused("native protected admission requires original paid fee"));
    }
    if !prepay(builder, ctx, signer, work)? {
        return Ok(());
    }
    let (_, class, book, plan) = original_job(builder, id)?;
    let claim = book.claim().ok_or_else(|| refused("native own seal has no claim"))?;
    capacity::check_fresh_start(builder, id, ctx.daa_score, claim.challenge_end)?;
    let actor = palw_kernel_bond_id_v1(signer);
    let operator = builder.state.bonds.get(signer).unwrap().operator_id.as_bytes();
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mature =
        ctx.daa_score.checked_add(class.record().horizons.seal_maturity).ok_or_else(|| refused("native maturity overflow"))?;
    let reveal_until =
        ctx.daa_score.checked_add(class.record().horizons.seal_ttl).ok_or_else(|| refused("native seal clock overflow"))?;
    let horizon =
        reveal_until.checked_add(class.record().horizons.proof_grace).ok_or_else(|| refused("native seal horizon overflow"))?;
    let protected_key =
        ProtectedSealKeyV11 { job_id: id, vector_id: claim.vector, seal_actor: actor, proof_root, horizon_daa: horizon };
    let mut shard = quota_shard_seal(builder, p, &epoch, actor, id, Some(protected_key.root()))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let g = shard.global_row();
    let a = shard.actor_row();
    let global = g
        .protected_seal_count
        .checked_add(r.flat_model_seal_count_v6())
        .and_then(|v| v.checked_add(r.flat_model_seal_count_v7()))
        .and_then(|v| v.checked_add(r.flat_model_seal_count_v9()))
        .and_then(|v| v.checked_add(1))
        .ok_or_else(|| refused("native shared seal count overflow"))?;
    let actor_count = a.retained_protected_seals as usize
        + r.flat_model_seals_of_v6(signer).len()
        + r.flat_model_seals_of_v7(signer).len()
        + r.flat_model_seals_of_v9(signer).len()
        + 1;
    let debit_receipt =
        object_id(b"misaka-palw/native/original-paid-own-seal-debit/v11", &(ctx.block.as_bytes(), id, actor, proof_root, seal, fee));
    let mut native = NativePaidSealAnchorV11 {
        job_id: id,
        funded_job_root: book.job().root(),
        book_root: book.root(),
        claim_id: claim.id(),
        vector_id: claim.vector,
        seal_actor: actor,
        seal_operator_id: operator,
        producer_operator_id: h.producer_operator,
        proof_root,
        posted_daa: ctx.daa_score,
        matures_daa: mature,
        horizon_daa: horizon,
        original_paid_seal_root: [0; 64],
        original_debit: fee,
        original_debit_receipt: debit_receipt,
        shared_live_seals: global,
        shared_actor_live_seals: actor_count as u32,
    };
    native.original_paid_seal_root = native_paid_seal_record_root_v11(&native);
    let checked = check_native_paid_seal_v11(&class, &book, &epoch, &native).map_err(refused)?;
    let mut state = pursuit(r, &id)?;
    if state.retained_seal_keys.len() + state.retained_discovery_keys.len() >= 4
        || state.retained_seal_keys.contains(&protected_key.root())
    {
        return Err(refused("native shared retained job tickets exhausted"));
    }
    if r.aux.contains_key(&(EXECUTION_SEAL_TABLE_V11, key(&protected_key.root()))) {
        return Err(refused("native own seal duplicate"));
    }
    shard.admit_protected_seal(&p.quota, &epoch, &checked, &plan).map_err(refused)?;
    let trace_plan = trace_plan(builder, p, id)?;
    let checked_trace = check_trace_seal_v17(&class, &book, &epoch, &trace_plan, &native).map_err(refused)?;
    let mut trace =
        trace_shard_from_rows(p, &trace_plan, &shard.global_row(), &shard.actor_row(), shard.subject_row().unwrap(), None)?;
    trace.admit(&p.quota, &epoch, &trace_plan, &checked_trace).map_err(refused)?;
    let row = NativeExecutionSealV11 {
        version: 11,
        job_id: id,
        vector: claim.vector,
        claim_id: claim.id(),
        original_book_root: book.root(),
        funded_job_root: book.job().root(),
        bond: *signer,
        actor,
        operator,
        producer_operator: h.producer_operator,
        class_scope9_root: class.record().class_scope9_root,
        proof_root,
        seal,
        posted_daa: ctx.daa_score,
        mature_daa: mature,
        reveal_until,
        horizon_daa: horizon,
        original_paid_debit: fee,
        original_paid_receipt: debit_receipt,
        original_paid_record_root: native.original_paid_seal_root,
        spent: false,
    };
    state.retained_seal_keys.push(protected_key.root());
    state.required_retained_until = state.required_retained_until.max(horizon);
    let physical = capacity::plan_owner(builder, id, protected_key.root(), horizon)?;
    write_shard(builder, &shard, id)?;
    write_trace(builder, &trace, 1, protected_key.root())?;
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, key(&protected_key.root()), Some(key(&row)))?;
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&id), Some(key(&state)))?;
    capacity::commit_owner(builder, id, physical)?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}
pub(super) fn reveal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    if bytes.len() < 132 {
        return Err(refused("native reveal fixed prefix"));
    }
    let seal_id = fixed_id(&bytes[..64])?;
    let salt = fixed_id(&bytes[64..128])?;
    let n = u32::from_le_bytes(bytes[128..132].try_into().unwrap()) as usize;
    if n != bytes.len() - 132 {
        return Err(refused("native reveal exact filing count"));
    }
    let actor = palw_kernel_bond_id_v1(signer);
    let mut seal = seal_row(builder.state.kernel_route.as_ref().unwrap(), &seal_id)?;
    if seal.bond != *signer {
        return Err(refused("native own reveal actor differs"));
    }
    let id = seal.job_id;
    let (row, class, book, plan) = original_job(builder, id)?;
    if book.terminal() != ExecutionTerminalV11::Challengeable
        || seal.funded_job_root != book.job().root()
        || seal.original_book_root != book.root()
    {
        return Err(refused("native own seal original book differs"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = quota_shard_seal(builder, p, &epoch, actor, id, Some(seal_id))?;
    // Paid future allowance is debited BEFORE any proof/program restoration or
    // verifier callback. A bad salt/body consumes one of its original attempts.
    if shard.seal_row().is_none_or(|s| s.original_paid_seal_root != seal.original_paid_record_root) {
        return Err(refused("native paid proof/quota original money roots differ"));
    }
    shard.debit_protected(&p.quota, &epoch, &plan, ProtectedActionKindV11::Court, &bytes[132..]).map_err(refused)?;
    write_shard(builder, &shard, id)?;
    seal.check_reveal(&actor, &salt, &bytes[132..], ctx.daa_score).map_err(refused)?;
    convict(builder, ctx, p, signer, row, class, book, plan, &bytes[132..], seal.proof_root, seal.operator, None)?;
    seal.spent = true;
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, key(&seal_id), Some(key(&seal)))?;
    Ok(())
}
/// **G14 lane closure G-N1 (v11): a self-contained public filing needs no owner slot.** `job ‖ u32 n ‖ filing` (the reveal's
/// filing grammar, kinds 0 / 1 / 2) from any Active bond whose operator is not the producer's. The four retained job tickets
/// (`retained_seal_keys + retained_discovery_keys ≤ 4`) bound the producer's prepaid PROTECTED answers — material a ticket owner
/// may compel. A filer that already holds the material (every DA answer is public on chain: `EXECUTION_SERVED_TABLE_V11`) compels
/// nothing, so it takes no ticket and adds no producer obligation: four colluding owners that seal and never reveal cannot lock it
/// out. It pays the work fee up front (kept if the filing is wrong) and a public challenger Court envelope from its own pool
/// (`debit_public_action`, never the producer's reservation), then runs exactly the reveal's verification and conviction. No
/// commit-reveal: a copied filing convicts the same producer (the copier, at most the capped finder share, is paid instead).
/// In force exactly where `palw_funded_execution_v11` is (dormant on every preset); below it the action is not decoded.
pub(super) fn file(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    if bytes.len() < 68 {
        return Err(refused("native public filing fixed prefix"));
    }
    let id = fixed_id(&bytes[..64])?;
    let n = u32::from_le_bytes(bytes[64..68].try_into().unwrap()) as usize;
    if n != bytes.len() - 68 {
        return Err(refused("native public filing exact count"));
    }
    let filing = &bytes[68..];
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = r.execution_job_header_v11(&id).ok_or_else(|| refused("native publicly filed job absent"))?;
    let bond = builder.state.bonds.get(signer).ok_or_else(|| refused("native public filer bond absent"))?;
    let operator = bond.operator_id.as_bytes();
    // RFC08 08-B (ADR-0184 §1, dormant `palw_native_liability_horizon_v1`): a Final whose fee is held stays accusable by public
    // filing through its liability horizon. Only the self-contained public filing reaches it (a Final had no live seal or demand).
    let held_horizon = if h.terminal == ExecutionTerminalV11::Final {
        super::super::palw_native_liability_hold_fold_v1::native_accusable_after_final_v1(
            &builder.state,
            crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_FUNDED_V11,
            &id,
            ctx.daa_score,
        )
        .map(|row| row.horizon_daa)
    } else {
        None
    };
    if !matches!(bond.status, PalwBondStatusV2::Active)
        || operator == h.producer_operator
        || *signer == h.producer
        || (held_horizon.is_none() && (h.terminal != ExecutionTerminalV11::Challengeable || ctx.daa_score > h.deadline_daa))
    {
        return Err(refused("native public filer is the producer or the job is not challengeable"));
    }
    let work = class_work(builder, h.class_id, bytes.len())?;
    if !prepay(builder, ctx, signer, work)? {
        return Ok(());
    }
    let (row, class, book, plan) = original_job(builder, id)?;
    if let Some(horizon) = held_horizon {
        // The Final closed the job's quota subject; the post-Final filing pays only its work fee (prepaid above, kept if wrong).
        if book.terminal() != ExecutionTerminalV11::Final {
            return Err(refused("native post-Final filing original book is not a held Final"));
        }
        let proof_id = object_id(b"misaka-palw/native/g14-public-filing/v11", &(id, filing));
        return convict(builder, ctx, p, signer, row, class, book, plan, filing, proof_id, operator, Some(horizon));
    }
    if book.terminal() != ExecutionTerminalV11::Challengeable {
        return Err(refused("native public filing original book is not challengeable"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let binding = book.job().quota_subject(class.record()).map_err(refused)?;
    let action = plan.action(QuotaActionKindV11::Court, filing).map_err(refused)?;
    let mut shard = quota_shard(builder, p, &epoch, palw_kernel_bond_id_v1(signer), id)?;
    shard.debit_public_action(&p.quota, &epoch, &binding, &plan, &action).map_err(refused)?;
    write_shard(builder, &shard, id)?;
    let proof_id = object_id(b"misaka-palw/native/g14-public-filing/v11", &(id, filing));
    convict(builder, ctx, p, signer, row, class, book, plan, filing, proof_id, operator, None)
}
/// **G14 lane closure G-N1 (v11): a lapsed seal gives its retained job ticket back** (`job ‖ seal`, anyone, work fee prepaid). A
/// seal never revealed whose reveal window has closed (`now ≥ reveal_until`, so it can no longer be revealed: `check_reveal`) and
/// that compelled no protected answer leaves the job's catalogue before the job is terminal: the pursuit's four retained tickets,
/// the kernel subject / owner / global counters, the owner's trace supplement and its physical owner place. The producer's prepaid
/// protected-answer bound holds unchanged (every seal still counted holds ≤ 2 protected answers, a released one held none); the
/// lapsed owner's fee and unused guarantee stay spent. A squatting owner therefore holds a ticket at most `seal_ttl` per paid seal;
/// one that compels answers instead keeps its ticket — and its answers are public material for every outsider's public filing.
pub(super) fn release_lapsed(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    if bytes.len() != 128 {
        return Err(refused("native lapse exact job and seal"));
    }
    let job = fixed_id(&bytes[..64])?;
    let id = fixed_id(&bytes[64..])?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let own = seal_row(r, &id)?;
    let h = r.execution_job_header_v11(&job).ok_or_else(|| refused("native lapsed seal's job absent"))?;
    let state = pursuit(r, &job)?;
    if own.job_id != job
        || own.spent
        || ctx.daa_score < own.reveal_until
        || h.terminal != ExecutionTerminalV11::Challengeable
        || !state.retained_seal_keys.contains(&id)
    {
        return Err(refused("native seal has not lapsed (spent, revealable, terminal job or not retained)"));
    }
    if !prepay(builder, ctx, signer, class_work(builder, h.class_id, bytes.len() + 2048)?)? {
        return Ok(());
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = quota_shard_seal(builder, p, &epoch, own.actor, job, Some(id))?;
    shard.release_lapsed_protected_seal_v41(&p.quota, &epoch).map_err(refused)?;
    let physical = capacity::plan_release_lapsed_owner(builder, job, id)?;
    write_shard(builder, &shard, job)?;
    let plan = trace_plan(builder, p, job)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let live_trace = read_rooted::<TraceSupplementV17>(r, trace_row_key(1, id), TRACE_ROW_BYTES_V17 as usize)?
        .is_some_and(|(_, t)| t.interval == epoch.network_interval && !t.retired);
    if live_trace {
        let mut trace = trace_shard(builder, p, &plan, own.actor, job, 1, id)?;
        trace.retire_lapsed_v41(&p.quota, &epoch, &plan, &id).map_err(refused)?;
        write_trace(builder, &trace, 1, id)?;
    }
    let mut state = state;
    state.retained_seal_keys.retain(|v| *v != id);
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&job), Some(key(&state)))?;
    capacity::commit_owner(builder, job, physical)?;
    for k in [quota_key(3, &id), trace_row_key(1, id)] {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, k, None)?;
    }
    for k in [key(&id), key(&(12u8, id))] {
        builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, k, None)?;
    }
    for index in 0u8..64 {
        builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, key(&(15u8, id, index)), None)?;
    }
    Ok(())
}
/// The verification and conviction a reveal and a public filing share: decode the filing, verify it against the committed post
/// (kind 0 computation fault, 1 / 2 source mismatch), adjudicate (G-14: never refused for short collateral), slash what was
/// collected, record a verified source fact, credit the requester's refund and the finder.
#[allow(clippy::too_many_arguments)]
fn convict(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV11,
    signer: &PalwBondKeyV2,
    mut row: NativeExecutionJobRowV11,
    class: CheckedNativeExecutionClassHeaderV11,
    mut book: ExecutionJobStateV11,
    plan: CheckedJobQuotaPlanV11,
    filing: &[u8],
    proof_id: Digest,
    finder_operator: Digest,
    held_horizon: Option<u64>,
) -> Result<(), PalwStateV2Error> {
    let id = row.header.job_id;
    let actor = palw_kernel_bond_id_v1(signer);
    let cap = plan.action_carrier_cap(QuotaActionKindV11::Court).map_err(refused)?;
    let (filing_kind, filing_body) = if filing.starts_with(&16u16.to_le_bytes()) {
        let f = decode_native_source_header_filing_v16(filing, cap).map_err(refused)?;
        (f.kind, f.body)
    } else {
        let f = decode_flat_execution_filing_v11(filing, cap).map_err(refused)?;
        (f.kind, f.body)
    };
    let native_class = checked_class(builder, ctx, p, row.header.class_id, false)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let post_bytes = r.aux.get(&(EXECUTION_PROTECTED_TABLE_V11, key(&id))).ok_or_else(|| refused("native committed post absent"))?;
    if post_bytes.len() as u64 > row.header.stored_post_cap {
        return Err(refused("native stored post cap"));
    }
    let post = decode_flat_execution_post_v11(post_bytes).map_err(|e| refused(e.to_string()))?;
    let view = checked_flat_execution_post_v11(r, &native_class, &book, &post).map_err(refused)?;
    let kind = match filing_kind {
        0 => {
            let fault = misaka_palw_kernel::flat_court9::FlatFaultV9::from_bytes(&filing_body).map_err(|e| refused(e.to_string()))?;
            misaka_palw_kernel::flat_court9::verify_flat_fault_v9(&view.context(), &fault).map_err(|e| refused(format!("{e:?}")))?;
            NativeAdjudicationKindV11::ComputationFault
        }
        1 => {
            let binding = r
                .model_artifact_binding_header_v2(&Hash64::from_bytes(class.record().binding_id))
                .ok_or_else(|| refused("native original source absent"))?;
            let proof = crate::palw_model_artifact_v2::decode_model_artifact_proof_v2(
                &filing_body,
                binding.pc_instances,
                binding.max_proof_bytes,
            )
            .map_err(|e| refused(e.to_string()))?;
            let d = [misaka_palw_kernel::descriptor::k2_tir_v4_descriptor(), misaka_palw_kernel::descriptor::k2_tir_v5_descriptor()]
                .into_iter()
                .find(|d| d.digest() == class.record().descriptor)
                .ok_or_else(|| refused("native source descriptor"))?;
            let inventory = crate::palw_tir_artifact_v1::PalwTirModelInventoryV2::new(&d, &view.trace.program).map_err(refused)?;
            crate::palw_onboarding_v1::verify_artifact_mismatch_v2(
                &inventory,
                Hash64::from_bytes(class.record().inventory_root),
                Hash64::from_bytes(class.record().parameter_root),
                &proof,
            )
            .map_err(refused)?;
            NativeAdjudicationKindV11::SourceMismatch
        }
        2 => {
            let fault: NativeSourceHeaderFaultV16 =
                borsh::from_slice(&filing_body).map_err(|_| refused("native source header fault framing"))?;
            if fault.layer_code > 65536 {
                return Err(refused("native source header layer framing"));
            }
            let layer = if fault.layer_code == 0 { None } else { Some((fault.layer_code - 1) as u16) };
            let pc = native_class
                .parameters()
                .by_instance
                .get(&(fault.param, layer))
                .ok_or_else(|| refused("source header fault outside original real PC map"))?;
            let decl = native_class
                .view()
                .program
                .params
                .get(fault.param as usize)
                .ok_or_else(|| refused("source header fault original declaration absent"))?;
            if !fault.header.authenticates(pc) || fault.header.matches_decl(decl.dtype, &decl.shape) {
                return Err(refused("source header fault is not authenticated original PC declaration mismatch"));
            }
            NativeAdjudicationKindV11::SourceMismatch
        }
        _ => return Err(refused("native filing grammar")),
    };
    let claim = book.claim().unwrap();
    let result = NativeExecutionJudgmentV11 {
        job_id: id,
        claim_id: claim.id(),
        vector: claim.vector,
        class_id: class.record().id(),
        inventory_root: class.record().inventory_root,
        parameter_root: class.record().parameter_root,
        current_daa: ctx.daa_score,
        established_daa: ctx.daa_score,
        proof_id,
        finder: actor,
        kind,
    };
    // G14 lane closure G-14: a conviction is never refused for short collateral (another lane may have drained the bond): the claim
    // is voided and the requester refunded from its own escrow, and what the producer's bond still holds of the reserve is collected.
    let collectible = crate::palw_g14_lane_closure_v1::palw_native_producer_collectible_v1(
        builder.state.bonds.get(&row.header.producer).map(|b| b.collateral),
        claim.reserved_original,
    );
    // RFC08 08-B: a held Final is re-adjudicated (its unpaid fee moves to the requester); its quota subject closed at Final.
    let s = match held_horizon {
        Some(horizon) => book.adjudicate_held_final(class.record(), &result, collectible, horizon).map_err(refused)?,
        None => {
            let s = book.adjudicate(class.record(), &result, collectible).map_err(refused)?;
            close_adjudicated_quota(builder, ctx, p, id, &class, &book, &plan, *signer)?;
            s
        }
    };
    let collected = if s.collected_original_slash == 0 {
        0
    } else {
        builder.slash_bond(row.header.producer, s.collected_original_slash as u128)?
    };
    if collected != s.collected_original_slash {
        return Err(refused("native court collected a different original slash"));
    }
    if kind == NativeAdjudicationKindV11::SourceMismatch && filing_kind == 2 {
        let fact = NativeVerifiedHeaderFactV16 {
            version: 16,
            binding: class.record().binding_id,
            program: class.record().program_root,
            pc: class.record().parameter_root,
            proof_root: proof_id,
            established: ctx.daa_score,
            finder: actor,
            fault: borsh::from_slice(&filing_body).map_err(|_| refused("native verified header fact framing"))?,
        };
        let k = key(&(16u8, fact.binding));
        if !builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(EXECUTION_SOURCE_FACT_TABLE_V11, k.clone())) {
            builder.write_kernel_row(EXECUTION_SOURCE_FACT_TABLE_V11, k, Some(key(&fact)))?;
        }
    } else if kind == NativeAdjudicationKindV11::SourceMismatch {
        let header = NativeVerifiedSourceFactHeaderV11 {
            version: 11,
            binding_id: class.record().binding_id,
            descriptor: class.record().descriptor,
            program_root: class.record().program_root,
            inventory_root: class.record().inventory_root,
            parameter_root: class.record().parameter_root,
            proof_root: proof_id,
            established_daa: ctx.daa_score,
            finder: actor,
            finder_operator,
            proof_bytes_len: filing_body.len() as u32,
        };
        // The first original verified theorem owns its finder and clock.
        if !builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(EXECUTION_SOURCE_FACT_TABLE_V11, key(&header.binding_id)))
        {
            builder.write_kernel_row(
                EXECUTION_SOURCE_FACT_TABLE_V11,
                key(&header.binding_id),
                Some(key(&NativeVerifiedSourceFactV11 { header, proof_bytes: filing_body })),
            )?;
        }
    }
    if held_horizon.is_some() {
        return credit_after_final_v11(builder, ctx, &mut row, &book, &s);
    }
    credit(builder, ctx, &mut row, &book, &s)?;
    Ok(())
}
