//! Authenticated complete finite-domain conformance; never a registrant flag or test seam.
use super::palw_kernel_route_fold_v1::{charge_route_budget_v1, ensure_route_header, route_ledger_policy_v1};
use super::*;
use crate::palw_model_artifact_v2::{ModelArtifactBindingHeaderV2, PALW_MODEL_ARTIFACT_CANDIDATE_TABLE_V2};
use crate::palw_model_conformance_v2::*;
use crate::palw_onboarding_v1::ArtifactBindingStateV1;
use misaka_palw_kernel::hash::Digest;

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}
fn standing(row: &ModelArtifactBindingHeaderV2, daa: u64) -> bool {
    matches!(row.state_at(daa), ArtifactBindingStateV1::Matured | ArtifactBindingStateV1::Final)
}

pub(super) fn complete(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    class: &Digest,
    binding: &Digest,
    proof: &[u8],
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    if !builder.extras.kernel_route.as_ref().is_some_and(|r| r.opv.is_some()) {
        return Err(refused("model conformance requires palw_panel_free_v1"));
    }
    let (class, id) = (Hash64::from_bytes(*class), Hash64::from_bytes(*binding));
    let route = builder.state.kernel_route.as_ref().unwrap();
    if route.model_conformance_v2(&class).is_some() {
        return Err(refused("model conformance is already checked"));
    }
    let row = route.model_artifact_binding_header_v2(&id).ok_or_else(|| refused("model conformance has no statement"))?;
    if !standing(&row, ctx.daa_score) {
        return Err(refused("model conformance statement does not stand"));
    }
    if route.template().schedule.standing_at(&row.descriptor, ctx.daa_score)
        != misaka_palw_kernel::descriptor::KernelStandingV1::Active
    {
        return Err(refused("model conformance descriptor is not active"));
    }
    if route.model_artifact_candidate_binding_v2(&class).is_none() {
        return Err(refused("model conformance has no scoped candidate"));
    }
    let runs = route
        .aux_row::<(u64, u32)>(MODEL_CONFORMANCE_BLOCK_TABLE_V2, &[])
        .filter(|(score, _)| *score == ctx.blue_score)
        .map(|(_, n)| n)
        .unwrap_or(0);
    if runs >= MODEL_CONFORMANCE_CHECKS_PER_BLOCK_V2 {
        return Ok(());
    }
    if proof.len() > MODEL_CONFORMANCE_MAX_PROOF_BYTES_V2 {
        return Err(refused("model conformance proof cannot be carried"));
    }
    let work = model_conformance_charge_v2(row.program_bytes_len as usize, proof.len())
        .ok_or_else(|| refused("model conformance work overflows"))?;
    let fee = route_ledger_policy_v1(builder)?.dismissal_fee_v1(work) as u128;
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < fee {
        return Err(refused("model conformance actor has insufficient free collateral"));
    }
    if !charge_route_budget_v1(builder, ctx, work, false)? {
        return Ok(());
    }
    builder.write_kernel_row(MODEL_CONFORMANCE_BLOCK_TABLE_V2, Vec::new(), Some(borsh::to_vec(&(ctx.blue_score, runs + 1)).unwrap()));
    // A judged run pays on pass or failure. Returning Ok on junk preserves budget/fee.
    builder.slash_bond(*signer, fee)?;
    let checked = (|| -> Result<ModelConformanceRowV2, String> {
        let route = builder.state.kernel_route.as_ref().unwrap();
        let record = route.kernel_class_record_v1(&class).ok_or("model conformance candidate disappeared")?;
        if record.descriptor != row.descriptor
            || misaka_palw_kernel::public::program_root_v1(&record.program_bytes) != row.program_root
            || record.param_commitments.root() != row.kernel_param_root.as_bytes()
        {
            return Err("model conformance candidate has another scope".into());
        }
        let descriptor =
            [misaka_palw_kernel::descriptor::k2_tir_v4_descriptor(), misaka_palw_kernel::descriptor::k2_tir_v5_descriptor()]
                .into_iter()
                .find(|d| d.digest() == row.descriptor)
                .ok_or("unknown model conformance descriptor")?;
        let program = misaka_palw_tir::TirProgramV1::decode_canonical(&record.program_bytes).map_err(|e| e.to_string())?;
        let domain = ModelConformanceDomainV2::new(&descriptor, &program, &record.plan)?;
        let post = decode_model_conformance_post_v2(proof).map_err(|e| e.to_string())?;
        let reference =
            domain.verify(&post.operands, row.model_inventory_root, row.kernel_param_root.as_bytes(), post.implementation_roots)?;
        Ok(ModelConformanceRowV2 {
            binding: id,
            descriptor: row.descriptor,
            program_root: row.program_root,
            plan_root: record.plan.root(),
            model_root: reference.model_root,
            pc_root: reference.pc_root,
            trace_root: reference.trace_root,
            cases: reference.cases,
            work: domain.work(),
            ram_envelope: domain.ram_envelope(),
            checked_daa: ctx.daa_score,
        })
    })();
    let Ok(checked) = checked else {
        return Ok(());
    };
    // A malicious earlier candidate's declared R cannot poison the class forever.
    // Only a complete, checked R↔PC relation can choose this authoritative binding.
    builder.write_kernel_row(
        PALW_MODEL_ARTIFACT_CANDIDATE_TABLE_V2,
        borsh::to_vec(&class).unwrap(),
        Some(borsh::to_vec(&id).unwrap()),
    );
    builder.write_kernel_row(MODEL_CONFORMANCE_TABLE_V2, borsh::to_vec(&class).unwrap(), Some(borsh::to_vec(&checked).unwrap()));
    Ok(())
}
