//! Consumer-owned admission and court folds for explicit flat-v5 candidate traces.
use super::palw_kernel_route_fold_v1::{charge_route_budget_v1, ensure_route_header, route_ledger_policy_v1};
use super::*;
use crate::palw_flat_model_vector_v3::*;
use crate::palw_model_vector_v2::MODEL_VECTORS_PER_BOND_V2;
use crate::palw_onboarding_v1::{
    ArtifactBindingStateV1, PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1, PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
    PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1,
};
use misaka_palw_kernel::hash::Digest;
fn refused(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(s.into())
}
fn enabled(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    if !builder.extras.kernel_route.as_ref().is_some_and(|r| r.opv.is_some()) {
        return Err(refused("flat vectors require palw_panel_free_v1"));
    }
    Ok(())
}
fn charge(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    work: u64,
    court: bool,
) -> Result<Option<u128>, PalwStateV2Error> {
    let fee = route_ledger_policy_v1(builder)?.dismissal_fee_v1(work) as u128;
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < fee {
        return Err(refused("flat vector actor cannot cover its dismissal fee"));
    }
    Ok(charge_route_budget_v1(builder, ctx, work, court)?.then_some(fee))
}
impl PalwChainStateV2 {
    pub fn flat_model_vector_reserved_at_v3(&self, bond: &PalwBondKeyV2, daa: u64) -> u128 {
        self.kernel_route.as_ref().map(|r| r.flat_model_vector_reserved_at_v3(bond, daa)).unwrap_or(0)
    }
}
pub(super) fn post(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    class: &Digest,
    binding: &Digest,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    let (class, binding) = (Hash64::from_bytes(*class), Hash64::from_bytes(*binding));
    let route = builder.state.kernel_route.as_ref().unwrap();
    let mut ids = route.flat_model_vectors_of_v3(signer);
    if ids.len() + route.model_vectors_of_v2(signer).len() >= MODEL_VECTORS_PER_BOND_V2
        || bytes.len() > FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3
    {
        return Err(refused("shared vector catalog/carrier full"));
    }
    let statement = route.model_artifact_binding_header_v2(&binding).ok_or_else(|| refused("flat vector has no scoped model"))?;
    if !matches!(statement.state_at(ctx.daa_score), ArtifactBindingStateV1::Matured | ArtifactBindingStateV1::Final)
        || route.model_artifact_candidate_binding_v2(&class).is_none()
        || route.template().schedule.standing_at(&statement.descriptor, ctx.daa_score)
            != misaka_palw_kernel::descriptor::KernelStandingV1::Active
    {
        return Err(refused("flat vector model/candidate does not stand"));
    }
    let work = flat_model_vector_work_v3(statement.program_bytes_len, statement.pc_instances, bytes.len())
        .ok_or_else(|| refused("flat vector work overflow"))?;
    let Some(fee) = charge(builder, ctx, signer, work, false)? else { return Ok(()) };
    builder.slash_bond(*signer, fee)?;
    let checked = (|| -> Result<_, String> {
        let route = builder.state.kernel_route.as_ref().unwrap();
        let post = decode_flat_model_vector_post_v3(bytes).map_err(|e| e.to_string())?;
        let view = checked_flat_model_vector_v3(route, class, binding, &post)?;
        if view.trace.params.by_instance.len() != statement.pc_instances as usize {
            return Err("flat PC count changed".into());
        }
        let worst = flat_model_vector_work_v3(
            statement.program_bytes_len,
            statement.pc_instances,
            bytes.len().checked_add(view.scope.worst_court_bytes as usize).ok_or("flat carrier overflow")?,
        )
        .and_then(|w| w.checked_add(view.scope.worst_court_work))
        .ok_or("flat worst court overflow")?;
        if worst > route_ledger_policy_v1(builder).map_err(|e| e.to_string())?.guaranteed_proof_work_v1()
            || view.scope.worst_court_bytes > FLAT_MODEL_VECTOR_MAX_PROOF_BYTES_V3 as u64
        {
            return Err("flat court exceeds shared guaranteed budget/carrier".into());
        }
        Ok((post, view.scope))
    })();
    let Ok((post, scope)) = checked else { return Ok(()) };
    let id = post.id(class.as_bytes(), binding.as_bytes());
    if builder.state.kernel_route.as_ref().unwrap().flat_model_vector_header_v3(&id).is_some() {
        return Ok(());
    }
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1 as u128 {
        return Ok(());
    }
    let h = FlatModelVectorHeaderV3 {
        trace_root: post.trace_root(class.as_bytes(), binding.as_bytes()),
        class,
        binding,
        profile: post.profile,
        scope_root: post.scope_root,
        program_bytes_len: statement.program_bytes_len,
        pc_instances: statement.pc_instances,
        post_bytes_len: bytes.len() as u32,
        max_filing_bytes: scope.worst_court_bytes,
        max_court_work: scope.worst_court_work,
        max_court_scratch_bytes: scope.worst_court_scratch_bytes,
        poster: *signer,
        posted_daa: ctx.daa_score,
        liability_until: ctx
            .daa_score
            .checked_add(PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1)
            .ok_or_else(|| refused("flat horizon overflow"))?,
        reserved: PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
        refuted: false,
    };
    ids.push(id);
    builder.write_kernel_row(
        FLAT_MODEL_VECTOR_TABLE_V3,
        borsh::to_vec(&id).unwrap(),
        Some(borsh::to_vec(&FlatModelVectorRowV3 { header: h, post: bytes.to_vec() }).unwrap()),
    );
    builder.write_kernel_row(FLAT_MODEL_VECTOR_BOND_TABLE_V3, borsh::to_vec(signer).unwrap(), Some(borsh::to_vec(&ids).unwrap()));
    Ok(())
}
pub(super) fn refute(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    vector: &Digest,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    let id = Hash64::from_bytes(*vector);
    let h = builder.state.kernel_route.as_ref().unwrap().flat_model_vector_header_v3(&id).ok_or_else(|| refused("no flat vector"))?;
    if h.refuted || ctx.daa_score >= h.liability_until {
        return Err(refused("flat vector no longer refutable"));
    }
    let challenger = builder.state.bonds.get(signer).unwrap();
    if builder.state.bonds.get(&h.poster).is_some_and(|b| b.operator_id == challenger.operator_id) {
        return Err(refused("flat poster operator cannot collect its bounty"));
    }
    let payout = challenger.payout_payload;
    let work = flat_model_vector_work_v3(h.program_bytes_len, h.pc_instances, bytes.len().saturating_add(h.post_bytes_len as usize))
        .and_then(|w| w.checked_add(h.max_court_work))
        .ok_or_else(|| refused("flat court work overflow"))?;
    let Some(fee) = charge(builder, ctx, signer, work, true)? else { return Ok(()) };
    let proven = (|| -> Result<(), String> {
        if bytes.len() as u64 > h.max_filing_bytes {
            return Err("flat filing beyond stored admission".into());
        }
        let fault = misaka_palw_kernel::flat_court5::FlatFaultV5::from_bytes(bytes).map_err(|e| e.to_string())?;
        let v = builder.state.kernel_route.as_ref().unwrap().flat_model_vector_view_v3(&id)?;
        let b = misaka_palw_kernel::flat_court5::flat_fault_court_bounds_v5(&v.context(), &fault).map_err(|e| format!("{e:?}"))?;
        if b.work > h.max_court_work || b.filing_bytes > h.max_filing_bytes || b.scratch_bytes > h.max_court_scratch_bytes {
            return Err("flat exact court outside persisted vector".into());
        }
        misaka_palw_kernel::flat_court5::verify_flat_fault_v5(&v.context(), &fault).map(|_| ()).map_err(|e| format!("{e:?}"))
    })()
    .is_ok();
    if !proven {
        builder.slash_bond(*signer, fee)?;
        return Ok(());
    }
    let mut row = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .flat_model_vector_row_v3(&id)
        .ok_or_else(|| refused("flat vector disappeared"))?;
    let slash = builder.slash_bond(h.poster, h.reserved as u128)?;
    builder.add_kernel_payout(payout, slash.saturating_mul(PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1) / 1000)?;
    row.header.refuted = true;
    row.header.reserved = 0;
    builder.write_kernel_row(FLAT_MODEL_VECTOR_TABLE_V3, borsh::to_vec(&id).unwrap(), Some(borsh::to_vec(&row).unwrap()));
    Ok(())
}
