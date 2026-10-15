//! Common signed32 paid parts. Namespace-specific new books verify and settle
//! only AFTER this original fixed-row debit/assembly/court sequence.
use super::*;
use crate::{palw_native_paged_proof_v29::*, palw_native_paged_quota_v29::*};
fn status_key(job: Digest) -> Vec<u8> {
    key(&(29u8, job, 0u8))
}
fn plan_key(job: Digest) -> Vec<u8> {
    key(&(29u8, job, 1u8))
}
fn part_key(job: Digest, seal: Digest, index: u16) -> Vec<u8> {
    key(&(29u8, job, seal, index))
}
fn owned_key(job: Digest, seal: Digest) -> Vec<u8> {
    key(&(29u8, job, seal))
}
fn trusted_quota(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    namespace: u16,
) -> Result<QuotaPolicyV11, PalwStateV2Error> {
    let e = builder.extras.kernel_route.as_ref().ok_or_else(|| refused("paged29 native network absent"))?;
    if !e.native_final.as_ref().is_some_and(|f| f.activation.is_active(ctx.daa_score)) {
        return Err(refused("paged29 below own Final27 fence"));
    }
    let q = match namespace {
        29 => e.funded_execution.as_ref().filter(|f| f.activation.is_active(ctx.daa_score)).map(|f| f.policies.quota.clone()),
        28 => e
            .stage_execution_v28
            .as_ref()
            .filter(|f| f.activation.is_active(ctx.daa_score))
            .map(|f| f.policies.execution.quota.clone()),
        _ => None,
    }
    .ok_or_else(|| refused("paged29 original namespace below independent fence"))?;
    q.check().map_err(refused)?;
    Ok(q)
}
pub(super) fn load(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    job: Digest,
    id: Digest,
) -> Result<(QuotaPolicyV11, CheckedNativePagedProofPlanV29, NativePagedQuotaProjectionV29), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let own = r.native_paged_seal_v29(job, id).ok_or_else(|| refused("paged29 original own paid row absent"))?;
    let quota = trusted_quota(builder, ctx, own.namespace)?;
    let cache = r
        .aux
        .get(&(EXECUTION_QUOTA_TABLE_V11, key(&(29u8, job, 3u8))))
        .ok_or_else(|| refused("paged29 original cached policy absent"))?;
    if cache != &key(&quota) {
        return Err(refused("paged29 original/current network policy differs"));
    }
    let plan = CheckedNativePagedProofPlanV29::restore(
        r.aux.get(&(EXECUTION_QUOTA_TABLE_V11, plan_key(job))).ok_or_else(|| refused("paged29 original quote absent"))?,
        own.original_plan,
        &quota,
    )
    .map_err(refused)?;
    let global = read_rooted::<QuotaGlobalRowV11>(r, vec![0], QUOTA_GLOBAL_ROW_BYTES_V11 as usize)?
        .ok_or_else(|| refused("paged29 original global absent"))?;
    let actor = read_rooted::<QuotaActorRowV11>(r, quota_key(1, &own.actor), QUOTA_SELECTED_ACTOR_ROW_BYTES_V11 as usize)?
        .ok_or_else(|| refused("paged29 original paid actor absent"))?;
    let raw =
        r.aux.get(&(EXECUTION_QUOTA_TABLE_V11, status_key(job))).ok_or_else(|| refused("paged29 original paid status absent"))?;
    if raw.len() > 1024 {
        return Err(refused("paged29 original status cap"));
    }
    let (sr, status): (Digest, NativePagedJobStatusV29) =
        borsh::from_slice(raw).map_err(|_| refused("paged29 original status grammar"))?;
    let projection = NativePagedQuotaProjectionV29::from_native_rows(
        &quota,
        &plan,
        &global.1,
        global.0,
        &actor.1,
        actor.0,
        &status,
        sr,
        Some((&own, own.root())),
    )
    .map_err(refused)?;
    Ok((quota, plan, projection))
}
pub(super) fn write(builder: &mut TransitionBuilder<'_>, s: &NativePagedQuotaProjectionV29) {
    let g = s.global();
    let a = s.actor();
    let job = s.status().job;
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, vec![0], Some(key(&(g.root(), g))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(1, &a.actor), Some(key(&(a.root(), a))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, status_key(job), Some(key(&(s.status().root(), s.status()))));
    if let Some(own) = s.seal() {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, owned_key(job, own.id()), Some(key(&(own.root(), own))));
    }
}
pub(super) fn part(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    let Some(meta) = borrowed_native_paged_metadata_v29(raw) else {
        return Ok(());
    };
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.native_paged_proof_owner_v29(meta.job, meta.seal, meta.owner, meta.index, meta.count, ctx.daa_score) != Some(*signer) {
        return Ok(());
    }
    let (quota, plan, mut s) = load(builder, ctx, meta.job, meta.seal)?;
    let epoch = TrustedEpochV11::from_chain(&quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    s.debit(&quota, &epoch, &plan, meta.owner, NativePagedDebitV29::Part(meta.index)).map_err(refused)?;
    write(builder, &s); // Original paid attempt survives EVERY later rejection.
    let Some(body) = borrowed_native_paged_part_v29(raw) else {
        return Ok(());
    };
    if s.accept_part(&plan, body.index, body.bytes.len()).is_err() {
        return Ok(());
    }
    let original = s.seal().unwrap().clone();
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, part_key(body.job, body.seal, body.index), Some(body.bytes.to_vec()))?;
    write(builder, &s);
    if (0..original.count).any(|i| !original.accepted(i)) {
        return Ok(());
    }
    s.debit(&quota, &epoch, &plan, meta.owner, NativePagedDebitV29::Assembly).map_err(refused)?;
    write(builder, &s); // BEFORE the full Vec or repeated fragment reads.
    let mut file = Vec::with_capacity(original.total_bytes as usize);
    for index in 0..original.count {
        let Some(bytes) =
            builder.state.kernel_route.as_ref().unwrap().aux.get(&(EXECUTION_SEAL_TABLE_V11, part_key(body.job, body.seal, index)))
        else {
            return Ok(());
        };
        let expected =
            if index + 1 == original.count { original.total_bytes as usize - index as usize * (32 << 10) } else { 32 << 10 };
        if bytes.len() != expected {
            return Ok(());
        }
        file.extend_from_slice(bytes);
    }
    if file.len() != original.total_bytes as usize {
        return Ok(());
    }
    // The court price also owns its bounded nested decoder, original class
    // restoration and exact source/PC check, including malformed whole files.
    s.debit(&quota, &epoch, &plan, meta.owner, NativePagedDebitV29::Court).map_err(refused)?;
    write(builder, &s);
    if paged_proof_root_v29(&file) != original.proof_root || file.len() < 68 {
        return Ok(());
    }
    let salt: Digest = file[..64].try_into().unwrap();
    let count = u32::from_le_bytes(file[64..68].try_into().unwrap()) as usize;
    if count != file.len() - 68
        || paged_proof_seal_v29(
            &original.namespace,
            &original.job,
            &original.claim,
            &original.actor,
            &original.maximum_contract,
            &original.proof_root,
            &salt,
        ) != original.seal_root
    {
        return Ok(());
    }
    let paid = s.seal().unwrap().clone();
    let result = match paid.namespace {
        28 => super::super::palw_flat_stage_fold_v14::reveal_paged_v28(builder, ctx, signer, &paid, &file),
        29 => super::text29::reveal_paged(builder, ctx, signer, &paid, &file),
        _ => Err(refused("paged29 unknown original native namespace")),
    };
    match result {
        Err(PalwStateV2Error::KernelRouteRefused(_)) => Ok(()),
        other => other,
    }
}
