//! Stage-native paid proof ownership. Its source/input namespaces and signed
//! commitments never enter text proof target6 or text execution87.
use super::super::palw_flat_execution_fold_v11 as native;
use super::*;
use crate::palw_flat_execution_v11::NativeExecutionProofGroupV11;
use misaka_palw_kernel::flat_quota11::{
    PROTECTED_FRAGMENT_BYTES_V11, ProtectedActionKindV11, ProtectedSealKeyV11, derive_protected_seal_plan_v11,
};

pub(super) fn row(
    r: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    id: &Digest,
) -> Result<NativeStageSealV14, PalwStateV2Error> {
    let raw = r.aux.get(&(STAGE_TABLE_V14, stage_key_v14(19, id))).ok_or_else(|| refused("stage original own paid seal absent"))?;
    if raw.len() > 2048 {
        return Err(refused("stage original paid seal fixed extent"));
    }
    let s: NativeStageSealV14 = borsh::from_slice(raw).map_err(|_| refused("stage original paid seal fixed framing"))?;
    let h = r.stage_job_header_v14(&s.job).ok_or_else(|| refused("stage original paid seal typed job absent"))?;
    if s.version != 14
        || s.id != *id
        || s.original_book != h.original_book_root
        || s.class != h.class
        || s.consent18 != h.acceptance
        || s.stage_quota != h.stage_quota_root
        || s.producer_operator != h.producer_operator
        || s.actor != palw_kernel_bond_id_v1(&s.bond)
        || s.id
            != (ProtectedSealKeyV11 {
                job_id: s.job,
                vector_id: s.claim,
                seal_actor: s.actor,
                proof_root: misaka_palw_kernel::hash::object_id(
                    b"misaka-palw/kernel/stage-protected-proof-namespace/v14",
                    &(s.proof_root, h.source_binding, h.admission_root, h.stage_quota_root),
                ),
                horizon_daa: s.horizon_daa,
            })
            .root()
    {
        return Err(refused("stage original paid seal class/job/consent/actor/key lineage differs"));
    }
    Ok(s)
}
impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    pub fn stage_seal_v14(&self, id: &Digest) -> Option<NativeStageSealV14> {
        row(self, id).ok()
    }
    pub fn stage_proof_owner_v14(&self, id: &Digest, now: u64) -> Option<PalwBondKeyV2> {
        let s = row(self, id).ok()?;
        let h = self.stage_job_header_v14(&s.job)?;
        (!s.spent
            && h.terminal == StageTerminalV14::Challengeable
            && h.book_root == s.book
            && now >= s.matures_daa
            && now <= s.horizon_daa)
            .then_some(s.bond)
    }
}
pub(super) fn seal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() != 192 {
        return Err(refused("stage ownseal exact original job/proof/seal frame"));
    }
    let id: Digest = body[..64].try_into().unwrap();
    let proof_root: Digest = body[64..128].try_into().unwrap();
    let seal: Digest = body[128..].try_into().unwrap();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = r.stage_job_header_v14(&id).ok_or_else(|| refused("stage ownseal original typed job absent"))?;
    if h.terminal != StageTerminalV14::Challengeable || h.producer == *signer {
        return Err(refused("stage ownseal lacks independent original committed job"));
    }
    let ch = r.stage_class_header_v14(&h.class).ok_or_else(|| refused("stage ownseal original class absent"))?;
    let sh = r.stage_source_header_v14(&ch.source_binding).ok_or_else(|| refused("stage ownseal original source absent"))?;
    let work = restore_tariff(builder, h.class, 192)?
        .checked_add(h.input_raw_bytes.checked_mul(1024).ok_or_else(|| refused("stage ownseal input tariff overflow"))?)
        .ok_or_else(|| refused("stage ownseal restore tariff overflow"))?;
    let fee = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?.dismissal_fee_v1(work);
    if fee == 0 {
        return Err(refused("stage ownseal requires real paid original native fee"));
    }
    if !prepay_restore(builder, ctx, signer, p, sh.source_bytes_len, ch.wire_bytes_len, 192, work)? {
        return Ok(());
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let restore = derive_stage_restore_metadata_v14(0, 0, h.book_bytes_len, h.input_raw_bytes, 1024).map_err(refused)?;
    let mut metadata = native::quota_shard(builder, &p.execution, &epoch, palw_kernel_bond_id_v1(signer), id)?;
    metadata.debit_ordinary(&p.execution.quota, &epoch, &restore).map_err(refused)?;
    native::write_shard(builder, &metadata, id);
    let class = checked_class(builder, ctx, p, h.class, false)?;
    let original = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_job_row_v14(&id)
        .ok_or_else(|| refused("stage ownseal original bounded book absent"))?;
    let book = StageJobBookV14::from_rooted_bytes(&original.book_bytes, h.book_root).map_err(refused)?;
    let actual = class.check_job(book.job()).map_err(refused)?;
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
    let actor = palw_kernel_bond_id_v1(signer);
    let operator = builder.state.bonds.get(signer).unwrap().operator_id.as_bytes();
    let mature = ctx
        .daa_score
        .checked_add(p.execution.execution.horizons.seal_maturity)
        .ok_or_else(|| refused("stage ownseal maturity overflow"))?;
    let reveal_until = ctx
        .daa_score
        .checked_add(p.execution.execution.horizons.seal_ttl)
        .ok_or_else(|| refused("stage ownseal reveal horizon overflow"))?;
    let horizon = reveal_until
        .checked_add(p.execution.execution.horizons.proof_grace)
        .ok_or_else(|| refused("stage ownseal proof grace overflow"))?;
    let mut state = source::pursuit(builder, id)?;
    let key = ProtectedSealKeyV11 {
        job_id: id,
        vector_id: book.claim_root().ok_or_else(|| refused("stage ownseal original claim absent"))?,
        seal_actor: actor,
        proof_root: misaka_palw_kernel::hash::object_id(
            b"misaka-palw/kernel/stage-protected-proof-namespace/v14",
            &(proof_root, h.source_binding, h.admission_root, h.stage_quota_root),
        ),
        horizon_daa: horizon,
    };
    if state.retained_seals + state.retained_discovery >= 4
        || state.seal_keys.contains(&key.root())
        || builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(STAGE_TABLE_V14, stage_key_v14(19, &key.root())))
    {
        return Err(refused("stage ownseal original retained slots exhausted/duplicate"));
    }
    let mut shard = native::quota_shard_seal(builder, &p.execution, &epoch, actor, id, Some(key.root()))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let shared = shard
        .global_row()
        .protected_seal_count
        .checked_add(r.flat_model_seal_count_v6())
        .and_then(|n| n.checked_add(r.flat_model_seal_count_v7()))
        .and_then(|n| n.checked_add(r.flat_model_seal_count_v9()))
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| refused("stage ownseal shared slots overflow"))?;
    let actor_live = shard.actor_row().retained_protected_seals as usize
        + r.flat_model_seals_of_v6(signer).len()
        + r.flat_model_seals_of_v7(signer).len()
        + r.flat_model_seals_of_v9(signer).len()
        + 1;
    let mut anchor = NativeStageProofAnchorV14 {
        job: id,
        original_book: h.original_book_root,
        current_book: book.root(),
        claim: key.vector_id,
        class: h.class,
        source: class.source().id(),
        source_parameters: class.source().parameters().root(),
        inventory: class.source().inventory_root(),
        admission: actual.admission().root(),
        combined_parameters: actual.combined_parameters().root(),
        maximum_scope: class.maximum_resources().court_scope,
        policy: p.root(),
        quota: plan.root(),
        consent18: h.acceptance,
        proof_root,
        actor,
        actor_operator: operator,
        producer_operator: h.producer_operator,
        posted_daa: ctx.daa_score,
        matures_daa: mature,
        horizon_daa: horizon,
        original_paid_root: [0; 64],
        original_debit: fee,
        original_debit_receipt: misaka_palw_kernel::hash::object_id(
            b"misaka-palw/native/stage-original-paid-ownseal-debit/v14",
            &(ctx.block.as_bytes(), id, actor, proof_root, seal, fee),
        ),
        shared_live: shared,
        actor_live: actor_live as u32,
        job_live: state.retained_seals + state.retained_discovery + 1,
    };
    anchor.original_paid_root = stage_paid_seal_record_root_v14(&anchor);
    let checked = check_stage_paid_seal_v14(&class, &book, &plan, &p.execution.quota, &p.execution.execution, &epoch, &anchor)
        .map_err(refused)?;
    if checked.key() != &key {
        return Err(refused("stage ownseal checked namespace key differs"));
    }
    shard.admit_protected_seal(&p.execution.quota, &epoch, &checked, plan.plan()).map_err(refused)?;
    let trace_plan = derive_stage_trace_plan_v14(&class, &plan, &p.execution.quota).map_err(refused)?;
    let checked_trace =
        check_stage_trace_seal_v14(&class, &book, &plan, &p.execution.quota, &p.execution.execution, &epoch, &trace_plan, &anchor)
            .map_err(refused)?;
    let mut trace = native::trace_shard_from_rows(
        &p.execution,
        &trace_plan,
        &shard.global_row(),
        &shard.actor_row(),
        shard.subject_row().unwrap(),
        None,
    )?;
    trace.admit(&p.execution.quota, &epoch, &trace_plan, &checked_trace).map_err(refused)?;
    let row = NativeStageSealV14 {
        version: 14,
        id: key.root(),
        job: id,
        class: h.class,
        claim: key.vector_id,
        original_book: h.original_book_root,
        book: book.root(),
        consent18: h.acceptance,
        stage_quota: plan.root(),
        trace_plan: trace_plan.root(),
        bond: *signer,
        actor,
        operator,
        producer_operator: h.producer_operator,
        proof_root,
        seal,
        posted_daa: ctx.daa_score,
        matures_daa: mature,
        reveal_until,
        horizon_daa: horizon,
        paid_root: anchor.original_paid_root,
        original_debit: fee,
        debit_receipt: anchor.original_debit_receipt,
        spent: false,
    };
    let slot = state
        .seal_keys
        .iter_mut()
        .find(|n| **n == [0; 64])
        .ok_or_else(|| refused("stage ownseal finite owner catalogue exhausted"))?;
    *slot = key.root();
    state.retained_seals += 1;
    state.required_until = state.required_until.max(horizon);
    native::write_shard(builder, &shard, id);
    native::write_trace(builder, &trace, 1, key.root());
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(19, &key.root()), Some(bytes(&row)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(16, &id), Some(bytes(&state)))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}

pub(super) fn apply_chunk(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    chunk: &crate::palw_kernel_route_v1::PalwKernelChunkV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_kernel_route_v1::{PalwKernelChunkTargetV1, palw_kernel_chunk_inner_matches_target_v1};
    let PalwKernelChunkTargetV1::StageProofV14 { seal, actor } = chunk.target else {
        return Err(refused("stage proof chunk target differs"));
    };
    let p = policies(builder, ctx)?;
    if actor != palw_kernel_bond_id_v1(&chunk.opener)
        || chunk.count == 0
        || chunk.count > 64
        || chunk.index >= chunk.count
        || chunk.bytes.is_empty()
        || chunk.bytes.len() > PROTECTED_FRAGMENT_BYTES_V11 as usize
    {
        return Err(refused("stage own proof fragment fixed owner/count/32KiB extent"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let original = row(r, &seal)?;
    let h = r.stage_job_header_v14(&original.job).ok_or_else(|| refused("stage proof fragment original job absent"))?;
    if original.bond != chunk.opener
        || original.actor != actor
        || original.spent
        || ctx.daa_score < original.matures_daa
        || ctx.daa_score >= original.reveal_until
        || h.terminal != StageTerminalV14::Challengeable
        || h.book_root != original.book
        || builder
            .state
            .bonds
            .get(&chunk.opener)
            .is_none_or(|b| !matches!(b.status, PalwBondStatusV2::Active) || b.operator_id.as_bytes() != original.operator)
    {
        return Err(refused("stage proof fragment original paid operator/claim/window differs"));
    }
    // Cached original plan fields price the attempt BEFORE restoring any model,
    // copying an expanding group or decoding the assembled object.
    let cached = CheckedStageQuotaV14::restore_native_record(
        h.class,
        h.source_binding,
        h.admission_root,
        h.policy_root,
        &h.quota_record,
        h.stage_quota_root,
    )
    .map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = native::quota_shard_seal(builder, &p.execution, &epoch, actor, h.id, Some(seal))?;
    if shard.seal_row().is_none_or(|s| s.original_paid_seal_root != original.paid_root) {
        return Err(refused("stage ownfragment original paid reservation differs"));
    }
    let paid = shard
        .debit_protected_fragment(
            &p.execution.quota,
            &epoch,
            cached.plan(),
            chunk.index as u32,
            chunk.count as u32,
            chunk.bytes.len() as u64,
            512,
        )
        .map_err(refused)?;
    native::write_shard(builder, &shard, h.id);
    let group_key = bytes(&(20u8, seal));
    let part_key = bytes(&(23u8, seal, chunk.index));
    let r = builder.state.kernel_route.as_ref().unwrap();
    let group = if let Some(raw) = r.aux.get(&(STAGE_TABLE_V14, group_key.clone())) {
        let g = NativeExecutionProofGroupV11::from_bytes(
            raw,
            derive_protected_seal_plan_v11(cached.plan()).map_err(refused)?.assembly_cap as usize,
        )
        .map_err(refused)?;
        if g.seal != seal
            || g.group != chunk.group.as_bytes()
            || g.opener != chunk.opener
            || g.count != chunk.count
            || !g.parts.is_empty()
            || ctx.daa_score > g.expires_daa
        {
            return Err(refused("stage ownfragment original fixed group differs"));
        }
        g
    } else {
        NativeExecutionProofGroupV11 {
            group: chunk.group.as_bytes(),
            seal,
            opener: chunk.opener,
            count: chunk.count,
            opened_daa: ctx.daa_score,
            expires_daa: original.reveal_until - 1,
            parts: Default::default(),
        }
    };
    if r.aux.contains_key(&(STAGE_TABLE_V14, part_key.clone())) {
        return Err(refused("stage ownfragment original index already accepted"));
    }
    shard.accept_protected_fragment(&p.execution.quota, &epoch, cached.plan(), &paid).map_err(refused)?;
    let complete = shard.seal_row().is_some_and(|s| s.fragments.complete());
    builder.write_kernel_row(STAGE_TABLE_V14, group_key.clone(), Some(bytes(&group)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, part_key, Some(chunk.bytes.clone()))?;
    native::write_shard(builder, &shard, h.id);
    if !complete {
        return Ok(());
    }
    shard.debit_protected(&p.execution.quota, &epoch, cached.plan(), ProtectedActionKindV11::WholeChunk, &[]).map_err(refused)?;
    native::write_shard(builder, &shard, h.id);
    let cap = derive_protected_seal_plan_v11(cached.plan()).map_err(refused)?.assembly_cap as usize;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut total = 0usize;
    for i in 0..group.count {
        let raw = r
            .aux
            .get(&(STAGE_TABLE_V14, bytes(&(23u8, seal, i))))
            .ok_or_else(|| refused("stage whole proof lost original fragment"))?;
        if raw.is_empty() || raw.len() > PROTECTED_FRAGMENT_BYTES_V11 as usize {
            return Err(refused("stage whole proof original fragment extent"));
        }
        total = total
            .checked_add(raw.len())
            .filter(|n| *n <= cap)
            .ok_or_else(|| refused("stage whole proof exceeds original owned assembly"))?;
    }
    let mut whole = Vec::with_capacity(total);
    for i in 0..group.count {
        whole.extend_from_slice(&r.aux[&(STAGE_TABLE_V14, bytes(&(23u8, seal, i)))]);
    }
    if crate::palw_state_v2::palw_object_chunk_group_id_v1(&whole) != chunk.group {
        return Err(refused("stage prepaid whole proof group hash differs"));
    }
    let inner: PalwConsensusObjectV2 = borsh::from_slice(&whole).map_err(|_| refused("stage prepaid whole proof object framing"))?;
    if !palw_kernel_chunk_inner_matches_target_v1(&inner, &chunk.target)
        || !matches!(&inner,PalwConsensusObjectV2::KernelRouteV1{signer,..}if *signer==chunk.opener)
    {
        return Err(refused("stage own signed proof inner actor/seal/domain differs"));
    }
    builder.write_kernel_row(STAGE_TABLE_V14, group_key, None)?;
    for i in 0..group.count {
        builder.write_kernel_row(STAGE_TABLE_V14, bytes(&(23u8, seal, i)), None)?;
    }
    super::super::apply_object(builder, ctx, &inner)
}
impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    pub fn stage_proof_deadline_v14(&self, seal: &Digest, actor: &Digest, now: u64) -> Option<u64> {
        let s = row(self, seal).ok()?;
        let h = self.stage_job_header_v14(&s.job)?;
        (s.actor == *actor
            && !s.spent
            && h.terminal == StageTerminalV14::Challengeable
            && h.book_root == s.book
            && now >= s.matures_daa
            && now < s.reveal_until)
            .then(|| s.reveal_until - 1)
    }
}

pub(super) fn reveal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    use misaka_palw_kernel::flat_quota11::trace17::trace_plan_from_native_record_v17;
    use misaka_palw_kernel::{
        element::SegClaimContextV1,
        flat_court5::{FlatSegContextV5, flat_trace_profile_id_v5},
        flat_court9::{FlatContextV9, FlatFaultV9},
        job::DecodeRuleV1,
        seg::prompt_root_of_ids_v1,
    };
    if body.len() < 139 {
        return Err(refused("stage reveal fixed seal/salt/version14 filing frame"));
    }
    let seal: Digest = body[..64].try_into().unwrap();
    let salt: Digest = body[64..128].try_into().unwrap();
    let n = u32::from_le_bytes(body[128..132].try_into().unwrap()) as usize;
    if n != body.len() - 132 {
        return Err(refused("stage reveal exact own filing length"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut own = row(r, &seal)?;
    let h = r.stage_job_header_v14(&own.job).ok_or_else(|| refused("stage own reveal original job absent"))?;
    if own.bond != *signer
        || own.spent
        || h.book_root != own.book
        || h.terminal != StageTerminalV14::Challengeable
        || ctx.daa_score < own.matures_daa
        || ctx.daa_score > own.horizon_daa
        || builder.state.bonds.get(signer).is_none_or(|b| b.operator_id.as_bytes() != own.operator)
    {
        return Err(refused("stage own reveal original paid owner/claim/window differs"));
    }
    let cached = CheckedStageQuotaV14::restore_native_record(
        h.class,
        h.source_binding,
        h.admission_root,
        h.policy_root,
        &h.quota_record,
        h.stage_quota_root,
    )
    .map_err(refused)?;
    let trace_plan =
        trace_plan_from_native_record_v17(h.class, &h.quota_record, &h.quota_record.root(), &p.execution.quota).map_err(refused)?;
    if trace_plan.root() != own.trace_plan {
        return Err(refused("stage own reveal original paid restore plan differs"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut trace = native::trace_shard(builder, &p.execution, &trace_plan, own.actor, own.job, 1, seal)?;
    trace.debit_reserved(&p.execution.quota, &epoch, &trace_plan, &seal, 1).map_err(refused)?;
    native::write_trace(builder, &trace, 1, seal);
    let mut paid = native::quota_shard_seal(builder, &p.execution, &epoch, own.actor, own.job, Some(seal))?;
    let verdict =
        paid.debit_stage_protected_verdict_v14(&p.execution.quota, &epoch, &cached, &own.paid_root, &body[132..]).map_err(refused)?;
    native::write_shard(builder, &paid, own.job);
    let filing = &body[132..];
    if stage_proof_root_v14(filing) != own.proof_root
        || stage_proof_seal_v14(own.proof_root, salt, own.actor, own.job, own.claim) != own.seal
        || u16::from_le_bytes(filing[..2].try_into().unwrap()) != 14
        || filing[2] > 2
        || u32::from_le_bytes(filing[3..7].try_into().unwrap()) as usize != filing.len() - 7
    {
        return Err(refused("stage paid reveal original salt/proof/version14 bounded filing differs"));
    }
    let class = checked_class(builder, ctx, p, h.class, false)?;
    let mut original = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_job_row_v14(&own.job)
        .ok_or_else(|| refused("stage own reveal bounded original book absent"))?;
    let mut book = StageJobBookV14::from_rooted_bytes(&original.book_bytes, h.book_root).map_err(refused)?;
    let actual = class.check_job(book.job()).map_err(refused)?;
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
    if plan.root() != cached.root() || Some(own.claim) != book.claim_root() {
        return Err(refused("stage own reveal rederived original admission/claim differs"));
    }
    let kind = match filing[2] {
        0 => {
            let claim = StageTraceV14::from_bytes(&original.trace_bytes, book.job().positions).map_err(refused)?;
            if claim.root() != own.claim {
                return Err(refused("stage own reveal stored original trace differs"));
            }
            let program = actual.program().v1_view();
            let prompt = vec![0; book.job().positions as usize];
            let context = FlatContextV9 {
                scope: actual.scope(),
                base: FlatSegContextV5 {
                    profile: flat_trace_profile_id_v5(),
                    claim: SegClaimContextV1 {
                        program: &program,
                        params: actual.combined_parameters(),
                        segment_roots: &claim.segment_roots,
                        positions: book.job().positions,
                        prompt_len: book.job().positions,
                        prompt_root: prompt_root_of_ids_v1(&prompt),
                        inline_prompt: Some(&prompt),
                        generated: &[0],
                        decode: DecodeRuleV1::Greedy,
                        encoder: None,
                    },
                },
            };
            let fault = FlatFaultV9::from_bytes(&filing[7..]).map_err(|e| refused(format!("{e:?}")))?;
            actual.verify_fault(&context, &fault).map_err(|e| refused(format!("{e:?}")))?;
            StageAdjudicationKindV14::ComputationMismatch
        }
        1 => {
            let source = builder
                .state
                .kernel_route
                .as_ref()
                .unwrap()
                .stage_source_row_v14(&h.source_binding)
                .ok_or_else(|| refused("stage source court original real source absent"))?;
            verify_stage_source_proof_v14(&source, &filing[7..]).map_err(refused)?;
            StageAdjudicationKindV14::SourceMismatch
        }
        2 => {
            let fault: crate::palw_flat_execution_v11::NativeSourceHeaderFaultV16 =
                borsh::from_slice(&filing[7..]).map_err(|_| refused("stage source header original fixed filing"))?;
            if fault.layer_code > 65536 {
                return Err(refused("stage source header original layer extent"));
            }
            let layer = if fault.layer_code == 0 { None } else { Some((fault.layer_code - 1) as u16) };
            let pc = class
                .source()
                .parameters()
                .by_instance
                .get(&(fault.param, layer))
                .ok_or_else(|| refused("stage source header uses non-source PC namespace"))?;
            let decl = class
                .source()
                .program()
                .params
                .get(fault.param as usize)
                .ok_or_else(|| refused("stage source header real declaration absent"))?;
            if !fault.header.authenticates(pc) || fault.header.matches_decl(decl.dtype, &decl.shape) {
                return Err(refused("stage source header is not original bound-PC declaration mismatch"));
            }
            StageAdjudicationKindV14::SourceHeaderMismatch
        }
        _ => unreachable!(),
    };
    let reserve = book.producer_reserved();
    if matches!(kind, StageAdjudicationKindV14::SourceMismatch | StageAdjudicationKindV14::SourceHeaderMismatch) {
        let fact = NativeStageNegativeSourceFactV14 {
            version: 14,
            relation: class.source().relation_root(),
            source: class.source().id(),
            source_program: class.source().program_root(),
            parameters: class.source().parameters().root(),
            inventory: class.source().inventory_root(),
            proof: own.proof_root,
            established_daa: ctx.daa_score,
            reporter: *signer,
        };
        // A theorem denies future aliases. Every existing job still needs its
        // own paid original court/default and actual conserved money below.
        builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(14, &fact.relation), Some(bytes(&fact)))?;
    }
    if builder.state.bonds.get(&h.producer).is_none_or(|b| b.collateral < reserve) {
        return Err(refused("stage owncourt original liability not fully collectible"));
    }
    let outcome = book
        .adjudicate(
            &class,
            &plan,
            &p.execution.execution,
            &NativeStageJudgmentV14 {
                job: own.job,
                original_book: h.original_book_root,
                book: h.book_root,
                class: h.class,
                claim: own.claim,
                source: class.source().id(),
                source_parameters: class.source().parameters().root(),
                inventory: class.source().inventory_root(),
                consent18: h.acceptance,
                proof: own.proof_root,
                finder: own.actor,
                finder_operator: own.operator,
                established_daa: ctx.daa_score,
                current_daa: ctx.daa_score,
                kind,
            },
            reserve,
        )
        .map_err(refused)?;
    let mut queue = misaka_palw_kernel::hash::object_id(
        b"misaka-palw/native/stage-original-principal-refund/v14",
        &(own.job, h.original_book_root, h.requester_payout),
    );
    queue[0] = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_PAYOUT_KEY_PREFIX_V1;
    let queue = Hash64::from_bytes(queue);
    if builder.state.pending_payouts.contains_key(&queue)
        || builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(STAGE_TABLE_V14, stage_key_v14(11, &own.job)))
    {
        return Err(refused("stage owncourt original principal already settled"));
    }
    let counter = builder.state.kernel_route.as_ref().unwrap().execution_nonce_counter_v11(&h.requester_operator).map_err(refused)?;
    let terminal = check_stage_conviction_terminal_v14(
        &class,
        &book,
        &plan,
        &p.execution.quota,
        &NativeStageTerminalAnchorV14 {
            job_id: own.job,
            original_funded_book_root: h.original_book_root,
            terminal_book_root: book.root(),
            acceptance18: h.acceptance,
            nonce: NativeRequesterNonceWitnessV11 {
                requester: book.job().requester,
                requester_operator_id: h.requester_operator,
                next_nonce: counter.next_nonce,
                counter_root: counter.root(),
            },
            established_daa: ctx.daa_score,
            principal_refund: outcome.requester_refund,
            principal_queue: queue.as_bytes(),
        },
        &outcome,
        reserve,
    )
    .map_err(refused)?;
    paid.mark_stage_verdict_closed_v14(&p.execution.quota, &epoch, &terminal, &verdict).map_err(refused)?;
    let payout = builder.state.bonds.get(signer).unwrap().payout_payload;
    if !builder.params.reporter_income_active_at(ctx.daa_score)
        && builder
            .state
            .pending_payouts
            .get(&crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payout))
            .is_some_and(|p| p.amount.checked_add(outcome.finder_credit).is_none())
    {
        return Err(refused("stage owncourt original finder payout overflow"));
    }
    let collected = builder.slash_bond(h.producer, reserve as u128)?;
    if collected != outcome.collected_original_slash {
        return Err(refused("stage owncourt actual collected slash differs"));
    }
    builder.write_payout(queue, Some(PalwPayoutV2 { payload: h.requester_payout, amount: outcome.requester_refund }))?;
    if outcome.finder_credit > 0 {
        if builder.params.reporter_income_active_at(ctx.daa_score) {
            let award = Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
                b"misaka-palw/native/stage-original-court-award/v14",
                &(own.job, outcome.root(), own.actor),
            ));
            builder.stage_reporter_recipient_v1(award, *signer, ctx.daa_score, outcome.finder_credit, Hash64::from_bytes(own.job))?;
        } else {
            builder.add_kernel_payout(payout, outcome.finder_credit)?;
        }
    }
    builder.cancel_native_recipient_v1(Hash64::from_bytes(own.job), h.producer, 14);
    builder.cancel_native_recipient_v1(Hash64::from_bytes(own.job), h.producer, 19);
    builder.bond_budget_close_v1(
        &Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/native/stage-original-fixed-budget/v20", &own.job)),
        ctx.daa_score,
    );
    original.header.book_root = book.root();
    original.header.terminal = book.terminal();
    original.header.producer_reserved = 0;
    original.book_bytes = bytes(&book);
    original.header.book_bytes_len = original.book_bytes.len() as u32;
    own.spent = true;
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(5, &own.job), Some(bytes(&original)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(13, &own.job), Some(bytes(&outcome)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(19, &seal), Some(bytes(&own)))?;
    native::write_shard(builder, &paid, own.job);
    builder.write_kernel_row(
        STAGE_TABLE_V14,
        stage_key_v14(11, &own.job),
        Some(bytes(&NativeStagePrincipalRefundV14 {
            version: 14,
            job: own.job,
            original_book: h.original_book_root,
            terminal_book: book.root(),
            acceptance18: h.acceptance,
            requester: h.requester,
            payout: h.requester_payout,
            queue,
            amount: outcome.requester_refund,
            established_daa: ctx.daa_score,
        })),
    )?;
    Ok(())
}

pub(super) fn retire(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    use misaka_palw_kernel::flat_quota11::trace17::{TRACE_ROW_BYTES_V17, TraceSupplementV17, retire_expired_trace_v17};
    let seal: Digest = body.try_into().map_err(|_| refused("stage expired proof exact original seal identity"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let own = row(r, &seal)?;
    let h = r.stage_job_header_v14(&own.job).ok_or_else(|| refused("stage retired proof original job absent"))?;
    if !matches!(h.terminal, StageTerminalV14::Refunded | StageTerminalV14::Convicted)
        || ctx.daa_score <= h.deadline_daa
        || ctx.daa_score <= own.horizon_daa
    {
        return Err(refused("stage proof retirement needs original closed complete horizon"));
    }
    let ch = r.stage_class_header_v14(&h.class).ok_or_else(|| refused("stage proof retirement original class absent"))?;
    let sh = r.stage_source_header_v14(&ch.source_binding).ok_or_else(|| refused("stage proof retirement original source absent"))?;
    let mut cache_bytes = 8192u64;
    for index in 0u8..64 {
        if let Some(raw) = r.aux.get(&(STAGE_TABLE_V14, bytes(&(23u8, seal, index)))) {
            if raw.len() > PROTECTED_FRAGMENT_BYTES_V11 as usize {
                return Err(refused("stage proof retirement cached part extent differs"));
            }
            cache_bytes =
                cache_bytes.checked_add(raw.len() as u64).ok_or_else(|| refused("stage proof retirement cached journal overflow"))?;
        }
    }
    if !prepay_restore(builder, ctx, signer, p, sh.source_bytes_len, ch.wire_bytes_len, 64, restore_tariff(builder, h.class, 64)?)? {
        return Ok(());
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let restore = derive_stage_restore_metadata_v14(
        0,
        0,
        h.book_bytes_len,
        h.input_raw_bytes.checked_add(cache_bytes).ok_or_else(|| refused("stage proof retirement full journal overflow"))?,
        0,
    )
    .map_err(refused)?;
    let mut caller = native::quota_shard(builder, &p.execution, &epoch, palw_kernel_bond_id_v1(signer), own.job)?;
    caller.debit_ordinary(&p.execution.quota, &epoch, &restore).map_err(refused)?;
    native::write_shard(builder, &caller, own.job);
    let class = checked_class(builder, ctx, p, h.class, false)?;
    let original = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_job_row_v14(&own.job)
        .ok_or_else(|| refused("stage proof retirement original bounded book absent"))?;
    let book = StageJobBookV14::from_rooted_bytes(&original.book_bytes, h.book_root).map_err(refused)?;
    let actual = class.check_job(book.job()).map_err(refused)?;
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let principal: NativeStagePrincipalRefundV14 = r
        .aux_row(STAGE_TABLE_V14, &stage_key_v14(11, &own.job))
        .ok_or_else(|| refused("stage proof retirement original principal credit absent"))?;
    if principal.version != 14
        || principal.job != own.job
        || principal.original_book != h.original_book_root
        || principal.terminal_book != book.root()
        || principal.acceptance18 != h.acceptance
        || principal.requester != h.requester
        || principal.payout != h.requester_payout
    {
        return Err(refused("stage proof retirement original principal beneficiary differs"));
    }
    let counter = r.execution_nonce_counter_v11(&h.requester_operator).map_err(refused)?;
    let anchor = NativeStageTerminalAnchorV14 {
        job_id: own.job,
        original_funded_book_root: h.original_book_root,
        terminal_book_root: book.root(),
        acceptance18: h.acceptance,
        nonce: NativeRequesterNonceWitnessV11 {
            requester: book.job().requester,
            requester_operator_id: h.requester_operator,
            next_nonce: counter.next_nonce,
            counter_root: counter.root(),
        },
        established_daa: principal.established_daa,
        principal_refund: principal.amount,
        principal_queue: principal.queue.as_bytes(),
    };
    let terminal = if book.terminal() == StageTerminalV14::Refunded {
        check_stage_quota_terminal_v14(&class, &book, &plan, &p.execution.quota, &anchor)
    } else {
        let outcome: StageSettlementV14 = r
            .aux_row(STAGE_TABLE_V14, &stage_key_v14(13, &own.job))
            .ok_or_else(|| refused("stage proof retirement original actual settlement absent"))?;
        check_stage_conviction_terminal_v14(
            &class,
            &book,
            &plan,
            &p.execution.quota,
            &anchor,
            &outcome,
            outcome.collected_original_slash,
        )
    }
    .map_err(refused)?;
    let trace_plan = derive_stage_trace_plan_v14(&class, &plan, &p.execution.quota).map_err(refused)?;
    let raw = r
        .aux
        .get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, bytes(&(17u8, 1u8, seal))))
        .ok_or_else(|| refused("stage original proof restore supplement absent"))?;
    if raw.len() != 64 + TRACE_ROW_BYTES_V17 as usize {
        return Err(refused("stage proof restore supplement fixed extent"));
    }
    let (trace_root, trace): (Digest, TraceSupplementV17) =
        borsh::from_slice(raw).map_err(|_| refused("stage proof restore supplement fixed framing"))?;
    if trace.role != 1
        || trace.key != seal
        || trace.actor != own.actor
        || trace.original_paid_root != own.paid_root
        || trace.horizon != own.horizon_daa
        || trace.plan_root != own.trace_plan
    {
        return Err(refused("stage proof restore original paid owner lineage differs"));
    }
    let mut state = source::pursuit(builder, own.job)?;
    let slot = state
        .seal_keys
        .iter()
        .position(|k| *k == seal)
        .ok_or_else(|| refused("stage proof retirement original owner catalogue absent"))?;
    let mut owner_projection = native::quota_shard_seal(builder, &p.execution, &epoch, own.actor, own.job, Some(seal))?;
    let protected =
        owner_projection.seal_row().ok_or_else(|| refused("stage proof retirement original protected record absent"))?.clone();
    if protected.original_paid_seal_root != own.paid_root {
        return Err(refused("stage proof retirement original paid quota root differs"));
    }
    let mut returned_owner = None;
    if own.actor == palw_kernel_bond_id_v1(signer) {
        owner_projection.collect_expired_protected_seal(&p.execution.quota, &epoch, &terminal).map_err(refused)?;
        caller = owner_projection;
    } else {
        let owner = owner_projection.actor_row();
        let (_, remaining) = caller
            .collect_expired_protected_seal_for(
                &p.execution.quota,
                &epoch,
                &terminal,
                &protected,
                &protected.row_root(),
                &owner,
                &owner.root(),
            )
            .map_err(refused)?;
        returned_owner = Some(remaining);
    }
    retire_expired_trace_v17(&mut caller, &p.execution.quota, &epoch, &trace_plan, &trace, &trace_root, &terminal).map_err(refused)?;
    state.seal_keys[slot] = [0; 64];
    state.retained_seals =
        state.retained_seals.checked_sub(1).ok_or_else(|| refused("stage proof retirement owner counter underflow"))?;
    native::write_shard(builder, &caller, own.job);
    if let Some(owner) = returned_owner {
        builder.write_kernel_row(
            crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,
            bytes(&(1u8, owner.actor)),
            Some(bytes(&(owner.root(), owner))),
        )?;
    }
    for key in [bytes(&(3u8, seal)), bytes(&(17u8, 1u8, seal))] {
        builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, key, None)?;
    }
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(19, &seal), None)?;
    builder.write_kernel_row(STAGE_TABLE_V14, bytes(&(20u8, seal)), None)?;
    for index in 0u8..64 {
        builder.write_kernel_row(STAGE_TABLE_V14, bytes(&(23u8, seal, index)), None)?;
    }
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(16, &own.job), Some(bytes(&state)))?;
    Ok(())
}
