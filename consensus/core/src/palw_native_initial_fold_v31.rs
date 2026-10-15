//! Paid bounded pre-Fund transport, a native journal child. Full inner
//! registration is dispatched only after original owner/hash and assembly
//! tariff; it revalidates its own public canonical source/class contract.
use super::*;
use crate::palw_native_initial_v31::*;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_quota11::{TrustedEpochV11, initial31::derive_initial_metadata_v31},
    hash::Digest,
};
const TABLE: u8 = crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11;
fn refused(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(s.into())
}
fn bytes<T: BorshSerialize>(v: &T) -> Vec<u8> {
    borsh::to_vec(v).expect("fixed initial31 row")
}
fn key<T: BorshSerialize>(v: &T) -> Vec<u8> {
    initial_key_v31(v)
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct InitialGlobalV31 {
    version: u16,
    next: u64,
    ids: Vec<(Digest, Digest)>,
}
fn global(builder: &TransitionBuilder<'_>) -> Result<InitialGlobalV31, PalwStateV2Error> {
    let Some(raw) = builder.state.kernel_route.as_ref().unwrap().aux.get(&(TABLE, key(&0u8))) else {
        return Ok(InitialGlobalV31 { version: 31, next: 0, ids: Vec::new() });
    };
    if raw.len() > 1024 {
        return Err(refused("initial31 bounded global"));
    }
    let row: InitialGlobalV31 = borsh::from_slice(raw).map_err(|_| refused("initial31 global grammar"))?;
    if row.version != 31
        || row.ids.len() > INITIAL_MAX_LIVE_V31
        || row
            .ids
            .iter()
            .enumerate()
            .any(|(i, (id, op))| *id == [0; 64] || *op == [0; 64] || row.ids[..i].iter().any(|(j, o)| id == j || op == o))
    {
        return Err(refused("initial31 original global owner/count differs"));
    }
    Ok(row)
}
fn write_header(builder: &mut TransitionBuilder<'_>, h: &NativeInitialHeaderV31) {
    builder.write_kernel_row(TABLE, key(&(h.id, 1u8)), Some(bytes(&(h.root(), h))));
}
fn policy(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    namespace: u16,
) -> Result<crate::palw_flat_execution_v11::NativeExecutionPoliciesV11, PalwStateV2Error> {
    super::palw_flat_model_vector_fold_v3::enabled(builder, ctx)?;
    let e = builder.extras.kernel_route.as_ref().ok_or_else(|| refused("initial31 original native extras absent"))?;
    if !e.native_final.as_ref().is_some_and(|f| f.activation.is_active(ctx.daa_score)) {
        return Err(refused("initial31 below own new Final27 fence"));
    }
    let p = match namespace {
        29 => e.funded_execution.as_ref().filter(|f| f.activation.is_active(ctx.daa_score)).map(|f| f.policies.clone()),
        28 => e.stage_execution_v28.as_ref().filter(|f| f.activation.is_active(ctx.daa_score)).map(|f| f.policies.execution.clone()),
        _ => None,
    }
    .ok_or_else(|| refused("initial31 original target namespace below its independent fence"))?;
    p.check(builder.state.kernel_route.as_ref().unwrap().header.policy.network_domain).map_err(refused)?;
    Ok(p)
}
fn prepay(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &crate::palw_flat_execution_v11::NativeExecutionPoliciesV11,
    n: u64,
    parts: u16,
    start: bool,
) -> Result<bool, PalwStateV2Error> {
    let metadata = derive_initial_metadata_v31(n, parts, start).map_err(refused)?;
    let work =
        n.checked_mul(4096).and_then(|n| n.checked_add(65536)).ok_or_else(|| refused("initial31 ordinary paid work overflow"))?;
    let Some(fee) = super::palw_flat_model_vector_fold_v3::charge(builder, ctx, signer, work, false)? else { return Ok(false) };
    builder.slash_bond(*signer, fee)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let actor = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(signer);
    let mut shard = super::palw_flat_execution_fold_v11::quota_shard(builder, p, &epoch, actor, [0; 64])?;
    shard.debit_ordinary(&p.quota, &epoch, &metadata).map_err(refused)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, [0; 64]);
    Ok(true)
}
fn header(builder: &TransitionBuilder<'_>, id: Digest) -> Result<NativeInitialHeaderV31, PalwStateV2Error> {
    builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .native_initial_header_v31(id)
        .ok_or_else(|| refused("initial31 original fixed header absent"))
}
fn mature_operator(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &crate::palw_flat_execution_v11::NativeExecutionPoliciesV11,
) -> Result<Digest, PalwStateV2Error> {
    let b = builder.state.bonds.get(signer).ok_or_else(|| refused("initial31 original bonded owner absent"))?;
    if b.status != PalwBondStatusV2::Active || ctx.daa_score.saturating_sub(b.registered_daa) < p.execution.horizons.seal_maturity {
        return Err(refused("initial31 original owner/operator immature or inactive"));
    }
    Ok(b.operator_id.as_bytes())
}
fn cleanup(builder: &mut TransitionBuilder<'_>, h: &NativeInitialHeaderV31) -> Result<(), PalwStateV2Error> {
    let mut g = global(builder)?;
    let i = g
        .ids
        .iter()
        .position(|(id, op)| *id == h.id && *op == h.operator)
        .ok_or_else(|| refused("initial31 original retained owner absent"))?;
    for index in 0..h.start.count {
        builder.write_kernel_row(TABLE, key(&(h.id, 2u8, index)), None)?;
    }
    builder.write_kernel_row(TABLE, key(&(h.id, 1u8)), None)?;
    g.ids.remove(i);
    builder.write_kernel_row(TABLE, key(&0u8), Some(bytes(&g)))?;
    Ok(())
}
pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    match inner(builder, ctx, signer, action, body) {
        Err(PalwStateV2Error::KernelRouteRefused(_)) => Ok(()),
        r => r,
    }
}
fn inner(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if action == INITIAL_START_ACTION_V31 {
        if body.len() > 128 {
            return Err(refused("initial31 fixed Start frame cap"));
        }
        let start: NativeInitialStartV31 = borsh::from_slice(body).map_err(|_| refused("initial31 exact Start frame"))?;
        let p = policy(builder, ctx, start.namespace)?;
        let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
        start.check(ctx.daa_score, epoch.interval_end(&p.quota).map_err(refused)?).map_err(refused)?;
        let op = mature_operator(builder, ctx, signer, &p)?;
        let mut g = global(builder)?;
        if g.next != start.sequence || g.ids.len() >= INITIAL_MAX_LIVE_V31 || g.ids.iter().any(|(_, o)| *o == op) {
            return Err(refused("initial31 permanent sequence or original global/operator slot exhausted"));
        }
        let next = g.next.checked_add(1).ok_or_else(|| refused("initial31 permanent sequence overflow"))?;
        if !prepay(builder, ctx, signer, &p, start.bytes as u64, start.count, true)? {
            return Ok(());
        }
        let id = start.id(signer);
        let h = NativeInitialHeaderV31 {
            version: 31,
            id,
            owner: *signer,
            operator: op,
            admitted_interval: epoch.network_interval,
            start,
            accepted: [0; 64],
            attempts: [0; 512],
            assembly_attempts: 0,
        };
        g.next = next;
        g.ids.push((id, op));
        write_header(builder, &h);
        builder.write_kernel_row(TABLE, key(&0u8), Some(bytes(&g)))?;
        return Ok(());
    }
    let part = if action == INITIAL_PART_ACTION_V31 {
        Some(borrowed_initial_part_v31(body).ok_or_else(|| refused("initial31 borrowed bounded part metadata"))?)
    } else {
        None
    };
    let id = match part {
        Some(p) => p.id,
        None => body.try_into().map_err(|_| refused("initial31 exact original id"))?,
    };
    let mut h = header(builder, id)?;
    let p = policy(builder, ctx, h.start.namespace)?;
    if action == INITIAL_COLLECT_ACTION_V31 {
        if ctx.daa_score <= h.start.expires_daa {
            return Err(refused("initial31 genuine original expiry has not closed"));
        }
        if !prepay(builder, ctx, signer, &p, h.start.bytes as u64, h.start.count, false)? {
            return Ok(());
        }
        return cleanup(builder, &h);
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    if h.owner != *signer
        || mature_operator(builder, ctx, signer, &p)? != h.operator
        || ctx.daa_score > h.start.expires_daa
        || epoch.network_interval != h.admitted_interval
    {
        return Err(refused("initial31 original owner or prepaid interval/horizon differs"));
    }
    if let Some(part) = part {
        if part.index >= h.start.count || h.has_part(part.index) || h.attempts[part.index as usize] >= 2 {
            return Err(refused("initial31 original part absent/used/exhausted"));
        }
        if !prepay(builder, ctx, signer, &p, body.len() as u64, h.start.count, false)? {
            return Ok(());
        }
        h.attempts[part.index as usize] += 1;
        write_header(builder, &h);
        let extent = if part.index + 1 == h.start.count {
            h.start.bytes as usize - part.index as usize * INITIAL_PART_BYTES_V31
        } else {
            INITIAL_PART_BYTES_V31
        };
        if part.declared as usize != part.bytes.len() || part.bytes.len() != extent {
            return Ok(());
        }
        h.accepted[part.index as usize / 8] |= 1 << (part.index % 8);
        builder.write_kernel_row(TABLE, key(&(id, 2u8, part.index)), Some(part.bytes.to_vec()))?;
        write_header(builder, &h);
        return Ok(());
    }
    if action != INITIAL_ASSEMBLE_ACTION_V31 || h.assembly_attempts >= 2 || !(0..h.start.count).all(|i| h.has_part(i)) {
        return Err(refused("initial31 original complete assembly absent/exhausted"));
    }
    if !prepay(builder, ctx, signer, &p, h.start.bytes as u64, h.start.count, false)? {
        return Ok(());
    }
    h.assembly_attempts += 1;
    write_header(builder, &h);
    let mut object = Vec::with_capacity(h.start.bytes as usize);
    for i in 0..h.start.count {
        let raw = builder
            .state
            .kernel_route
            .as_ref()
            .unwrap()
            .aux
            .get(&(TABLE, key(&(id, 2u8, i))))
            .ok_or_else(|| refused("initial31 original accepted part disappeared"))?;
        if raw.len() > INITIAL_PART_BYTES_V31 || object.len().checked_add(raw.len()).is_none_or(|n| n > h.start.bytes as usize) {
            return Err(refused("initial31 bounded original assembly extent differs"));
        }
        object.extend_from_slice(raw);
    }
    if object.len() != h.start.bytes as usize || initial_object_root_v31(&object) != h.start.original_object {
        return Ok(());
    }
    match h.start.namespace {
        28 => super::palw_flat_stage_fold_v14::apply(builder, ctx, signer, h.start.action, &object)?,
        29 => super::palw_flat_execution_fold_v11::apply(builder, ctx, signer, h.start.action, &object)?,
        _ => return Err(refused("initial31 unexpected original target")),
    }
    cleanup(builder, &h)
}
