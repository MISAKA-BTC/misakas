//! Candidate trace assertions and their public courts; no execution registration or reward.
use super::palw_kernel_route_fold_v1::{charge_route_budget_v1, ensure_route_header, route_ledger_policy_v1};
use super::*;
use crate::palw_model_artifact_v2::model_artifact_work_v2;
use crate::palw_model_vector_v2::*;
use crate::palw_onboarding_v1::{
    ArtifactBindingStateV1, PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1, PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
    PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1,
};
use misaka_palw_kernel::hash::Digest;

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}
fn enabled(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    if !builder.extras.kernel_route.as_ref().is_some_and(|e| e.opv.is_some()) {
        return Err(refused("model vector statements require palw_panel_free_v1"));
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
        return Err(refused("vector actor's free collateral cannot cover its dismissal fee"));
    }
    Ok(charge_route_budget_v1(builder, ctx, work, court)?.then_some(fee))
}
impl PalwChainStateV2 {
    pub fn model_vector_reserved_at_v2(&self, bond: &PalwBondKeyV2, daa: u64) -> u128 {
        self.kernel_route.as_ref().map(|r| r.model_vector_reserved_at_v2(bond, daa)).unwrap_or(0)
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
    let mut ids = route.model_vectors_of_v2(signer);
    if ids.len() >= MODEL_VECTORS_PER_BOND_V2 || bytes.len() > MODEL_VECTOR_MAX_POST_BYTES_V2 {
        return Err(refused("vector catalog or carrier is full"));
    }
    let statement = route.model_artifact_binding_header_v2(&binding).ok_or_else(|| refused("no scoped vector model"))?;
    if !matches!(statement.state_at(ctx.daa_score), ArtifactBindingStateV1::Matured | ArtifactBindingStateV1::Final)
        || route.model_artifact_candidate_binding_v2(&class).is_none()
        || route.template().schedule.standing_at(&statement.descriptor, ctx.daa_score)
            != misaka_palw_kernel::descriptor::KernelStandingV1::Active
    {
        return Err(refused("vector's model/candidate does not stand"));
    }
    let work =
        model_artifact_work_v2(statement.program_bytes_len as usize, bytes.len()).ok_or_else(|| refused("vector work overflow"))?;
    let Some(fee) = charge(builder, ctx, signer, work, false)? else { return Ok(()) };
    // Every judged assertion pays once, including a truthful duplicate. Otherwise a
    // repeated cheap-to-copy statement could consume admission CPU without new liability.
    builder.slash_bond(*signer, fee)?;
    // All variable stored-program, vector and semantic decoding follows budget reservation.
    let checked = (|| -> Result<_, String> {
        let route = builder.state.kernel_route.as_ref().unwrap();
        let post = decode_model_vector_post_v2(bytes).map_err(|e| e.to_string())?;
        let view = checked_vector_view(route, class, binding, &post)?;
        let record = route.kernel_class_record_v1(&class).ok_or("no vector class")?;
        let d = [misaka_palw_kernel::descriptor::k2_tir_v4_descriptor(), misaka_palw_kernel::descriptor::k2_tir_v5_descriptor()]
            .into_iter()
            .find(|d| d.digest() == record.descriptor)
            .ok_or("unknown vector descriptor")?;
        let policy = route_ledger_policy_v1(builder).map_err(|e| e.to_string())?;
        let bounds = misaka_palw_kernel::gate::class_prosecution_bounds_v1(
            &d,
            &record.plan,
            &view.program,
            &misaka_palw_kernel::public::ProfileMaterialV1::kernel_route(true),
            &policy.prosecution,
        )
        .map_err(|e| format!("vector plan is not publicly prosecutable: {e:?}"))?;
        let worst_work = model_artifact_work_v2(statement.program_bytes_len as usize, bounds.max_filing_bytes as usize)
            .and_then(|w| w.checked_add(bounds.max_court_work))
            .ok_or("vector worst court work overflows")?;
        if worst_work > policy.guaranteed_proof_work_v1() || bounds.max_filing_bytes > MODEL_VECTOR_MAX_PROOF_BYTES_V2 as u64 {
            return Err("vector court cannot fit the shared reserved work/carrier".into());
        }
        Ok((post, bounds))
    })();
    let Ok((post, bounds)) = checked else { return Ok(()) };
    let id = post.id(class.as_bytes(), binding.as_bytes());
    if builder.state.kernel_route.as_ref().unwrap().model_vector_header_v2(&id).is_some() {
        return Ok(());
    }
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1 as u128 {
        return Ok(());
    }
    let row = ModelVectorRowV2 {
        header: ModelVectorHeaderV2 {
            trace_root: post.trace_root(class.as_bytes(), binding.as_bytes()),
            class,
            binding,
            program_bytes_len: statement.program_bytes_len,
            max_filing_bytes: bounds.max_filing_bytes,
            max_court_work: bounds.max_court_work,
            poster: *signer,
            posted_daa: ctx.daa_score,
            liability_until: ctx
                .daa_score
                .checked_add(PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1)
                .ok_or_else(|| refused("vector liability horizon overflows"))?,
            reserved: PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
            refuted: false,
        },
        post: bytes.to_vec(),
    };
    ids.push(id);
    builder.write_kernel_row(MODEL_VECTOR_TABLE_V2, borsh::to_vec(&id).unwrap(), Some(borsh::to_vec(&row).unwrap()));
    builder.write_kernel_row(MODEL_VECTOR_BOND_INDEX_TABLE_V2, borsh::to_vec(signer).unwrap(), Some(borsh::to_vec(&ids).unwrap()));
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
    let h = builder.state.kernel_route.as_ref().unwrap().model_vector_header_v2(&id).ok_or_else(|| refused("no vector statement"))?;
    if h.refuted || ctx.daa_score >= h.liability_until {
        return Err(refused("vector is no longer refutable"));
    }
    let challenger = builder.state.bonds.get(signer).unwrap();
    if builder.state.bonds.get(&h.poster).is_some_and(|b| b.operator_id == challenger.operator_id) {
        return Err(refused("vector poster cannot collect its own operator's bounty"));
    }
    let payout = challenger.payout_payload;
    let work = model_artifact_work_v2(h.program_bytes_len as usize, bytes.len())
        .and_then(|w| w.checked_add(h.max_court_work))
        .ok_or_else(|| refused("vector court envelope overflows"))?;
    let Some(fee) = charge(builder, ctx, signer, work, true)? else { return Ok(()) };
    let proven = (|| -> Result<(), String> {
        if bytes.len() as u64 > h.max_filing_bytes {
            return Err("vector filing exceeds admitted court bound".into());
        }
        let fault = decode_model_vector_fault_v2(bytes).map_err(|e| e.to_string())?;
        let view = builder.state.kernel_route.as_ref().unwrap().model_vector_view_v2(&id)?;
        misaka_palw_kernel::element::verify_seg_fault_v1(&view.context(), &fault).map(|_| ()).map_err(|e| format!("{e:?}"))
    })()
    .is_ok();
    if !proven {
        builder.slash_bond(*signer, fee)?;
        return Ok(());
    }
    let mut row = builder.state.kernel_route.as_ref().unwrap().model_vector_v2(&id).ok_or_else(|| refused("vector disappeared"))?;
    let slashed = builder.slash_bond(h.poster, h.reserved as u128)?;
    builder.add_kernel_payout(payout, slashed.saturating_mul(PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1) / 1000)?;
    row.header.refuted = true;
    row.header.reserved = 0;
    builder.write_kernel_row(MODEL_VECTOR_TABLE_V2, borsh::to_vec(&id).unwrap(), Some(borsh::to_vec(&row).unwrap()));
    Ok(())
}
