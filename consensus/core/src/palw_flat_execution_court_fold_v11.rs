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
    write_shard(builder, &shard, id);
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, key(&protected_key.root()), Some(key(&row)));
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&id), Some(key(&state)));
    builder.note_kernel_bond(signer);
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
    let (mut row, class, mut book, plan) = original_job(builder, id)?;
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
    write_shard(builder, &shard, id);
    seal.check_reveal(&actor, &salt, &bytes[132..], ctx.daa_score).map_err(refused)?;
    let filing = decode_flat_execution_filing_v11(&bytes[132..], plan.action_carrier_cap(QuotaActionKindV11::Court).map_err(refused)?)
        .map_err(refused)?;
    let native_class = checked_class(builder, ctx, p, row.header.class_id, false)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let post_bytes = r.aux.get(&(EXECUTION_PROTECTED_TABLE_V11, key(&id))).ok_or_else(|| refused("native committed post absent"))?;
    if post_bytes.len() as u64 > row.header.stored_post_cap {
        return Err(refused("native stored post cap"));
    }
    let post = decode_flat_execution_post_v11(post_bytes).map_err(|e| refused(e.to_string()))?;
    let view = checked_flat_execution_post_v11(r, &native_class, &book, &post).map_err(refused)?;
    let kind = match filing.kind {
        0 => {
            let fault = misaka_palw_kernel::flat_court9::FlatFaultV9::from_bytes(&filing.body).map_err(|e| refused(e.to_string()))?;
            misaka_palw_kernel::flat_court9::verify_flat_fault_v9(&view.context(), &fault).map_err(|e| refused(format!("{e:?}")))?;
            NativeAdjudicationKindV11::ComputationFault
        }
        1 => {
            let binding = r
                .model_artifact_binding_header_v2(&Hash64::from_bytes(class.record().binding_id))
                .ok_or_else(|| refused("native original source absent"))?;
            let proof = crate::palw_model_artifact_v2::decode_model_artifact_proof_v2(
                &filing.body,
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
        proof_id: seal.proof_root,
        finder: actor,
        kind,
    };
    let collateral =
        builder.state.bonds.get(&row.header.producer).ok_or_else(|| refused("native original producer absent"))?.collateral;
    if collateral < claim.reserved_original {
        return Err(refused("native original slash no longer conserved"));
    }
    let s = book.adjudicate(class.record(), &result, claim.reserved_original).map_err(refused)?;
    let collected = builder.slash_bond(row.header.producer, s.collected_original_slash as u128)?;
    if collected != s.collected_original_slash {
        return Err(refused("native court collected a different original slash"));
    }
    if kind == NativeAdjudicationKindV11::SourceMismatch {
        let header = NativeVerifiedSourceFactHeaderV11 {
            version: 11,
            binding_id: class.record().binding_id,
            descriptor: class.record().descriptor,
            program_root: class.record().program_root,
            inventory_root: class.record().inventory_root,
            parameter_root: class.record().parameter_root,
            proof_root: seal.proof_root,
            established_daa: ctx.daa_score,
            finder: actor,
            finder_operator: seal.operator,
            proof_bytes_len: filing.body.len() as u32,
        };
        // The first original verified theorem owns its finder and clock.
        if !builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(EXECUTION_SOURCE_FACT_TABLE_V11, key(&header.binding_id)))
        {
            builder.write_kernel_row(
                EXECUTION_SOURCE_FACT_TABLE_V11,
                key(&header.binding_id),
                Some(key(&NativeVerifiedSourceFactV11 { header, proof_bytes: filing.body })),
            );
        }
    }
    credit(builder, ctx, &mut row, &book, &s)?;
    seal.spent = true;
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, key(&seal_id), Some(key(&seal)));
    Ok(())
}
