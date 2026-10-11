//! Consumer-only fold of inner 22–24; signatures/fences are checked by outer 110.
use super::palw_kernel_route_fold_v1::{charge_route_budget_v1, ensure_route_header, route_ledger_policy_v1};
use super::*;
use crate::palw_model_artifact_v2::*;
use crate::palw_onboarding_v1::{
    ArtifactBindingStateV1, PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1, PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
    PALW_ONBOARDING_BINDING_WINDOW_DAA_V1, PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1, verify_artifact_mismatch_v2,
};
use crate::palw_tir_artifact_v1::PalwTirModelInventoryV2;
use misaka_palw_kernel::descriptor::{KernelDescriptorV1, k2_tir_v4_descriptor, k2_tir_v5_descriptor};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::route::KernelRouteObjectV1;
use misaka_palw_kernel::trace::ParamCommitmentsV1;

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}
fn descriptor(id: &Digest) -> Option<KernelDescriptorV1> {
    [k2_tir_v4_descriptor(), k2_tir_v5_descriptor()].into_iter().find(|d| d.digest() == *id)
}
impl PalwChainStateV2 {
    pub fn model_artifact_reserved_at_v2(&self, bond: &PalwBondKeyV2, daa: u64) -> u128 {
        self.kernel_route.as_ref().map(|r| r.model_artifact_reserved_at_v2(bond, daa)).unwrap_or(0)
    }
}

/// Charged junk is a dismissal (Ok), keeping its work and attacker fee in the rehearsal.
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
        return Err(refused("model artifact actor has insufficient free collateral for a dismissal"));
    }
    Ok(charge_route_budget_v1(builder, ctx, work, court)?.then_some(fee))
}

pub(super) fn bind(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    d: &Digest,
    program_bytes: &[u8],
    model_root: &Digest,
    pc: &ParamCommitmentsV1,
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    if !builder.extras.kernel_route.as_ref().is_some_and(|e| e.opv.is_some()) {
        return Err(refused("model artifact statements require palw_panel_free_v1"));
    }
    let route = builder.state.kernel_route.as_ref().unwrap();
    let mut ids = route.model_artifact_bindings_of_v2(signer);
    if ids.len() >= PALW_MODEL_ARTIFACT_BINDINGS_PER_BOND_V2 {
        return Err(refused("model artifact bond catalog is full"));
    }
    let Some(d) = descriptor(d) else {
        return Err(refused("unknown model artifact role descriptor"));
    };
    if route.template().schedule.standing_at(&d.digest(), ctx.daa_score) != misaka_palw_kernel::descriptor::KernelStandingV1::Active {
        return Err(refused("model artifact descriptor is not active under the route schedule"));
    }
    // Exact Borsh map bytes (count, u16/Option<u16> key, 64-byte digest), without
    // allocating another attacker-controlled map encoding merely to price it.
    let filing_bytes = pc
        .by_instance
        .keys()
        .try_fold(4usize, |n, (_, layer)| n.checked_add(67 + usize::from(layer.is_some()) * 2))
        .ok_or_else(|| refused("PC map byte count overflows"))?;
    let max_proof =
        filing_bytes.checked_add(PALW_MODEL_ARTIFACT_PROOF_OVERHEAD_V2).ok_or_else(|| refused("binding proof size overflows"))?;
    if program_bytes.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES
        || max_proof + (16 << 10) > crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1
    {
        return Err(refused("model statement or its worst public identity proof cannot be carried"));
    }
    let work = model_artifact_work_v2(program_bytes.len(), filing_bytes).ok_or_else(|| refused("binding work overflows"))?;
    let Some(fee) = charge(builder, ctx, signer, work, false)? else { return Ok(()) };
    let checked = misaka_palw_tir::TirProgramV1::decode_canonical(program_bytes)
        .map_err(|e| e.to_string())
        .and_then(|p| PalwTirModelInventoryV2::new(&d, &p).map(|_| ()));
    if checked.is_err() {
        builder.slash_bond(*signer, fee)?;
        return Ok(());
    }
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1 as u128 {
        builder.slash_bond(*signer, fee)?;
        return Ok(());
    }
    let row = ModelArtifactBindingRowV2 {
        descriptor: d.digest(),
        program_root: misaka_palw_kernel::public::program_root_v1(program_bytes),
        program_bytes_len: program_bytes.len() as u32,
        program_bytes: program_bytes.to_vec(),
        model_inventory_root: Hash64::from_bytes(*model_root),
        kernel_param_root: Hash64::from_bytes(pc.root()),
        pc_instances: pc.by_instance.len() as u32,
        max_proof_bytes: max_proof as u32,
        binder: *signer,
        bound_daa: ctx.daa_score,
        matures_daa: ctx
            .daa_score
            .checked_add(PALW_ONBOARDING_BINDING_WINDOW_DAA_V1)
            .ok_or_else(|| refused("binding maturity overflows"))?,
        final_daa: ctx
            .daa_score
            .checked_add(PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1)
            .ok_or_else(|| refused("binding horizon overflows"))?,
        reserved: PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
        refuted: false,
    };
    let id = row.id();
    if builder.state.kernel_route.as_ref().unwrap().model_artifact_binding_v2(&id).is_some() {
        // Identical content shares a statement, not an owner's execution privilege.
        return Ok(());
    }
    ids.push(id);
    builder.write_kernel_row(PALW_MODEL_ARTIFACT_BINDINGS_TABLE_V2, borsh::to_vec(&id).unwrap(), Some(borsh::to_vec(&row).unwrap()));
    builder.write_kernel_row(
        PALW_MODEL_ARTIFACT_BOND_INDEX_TABLE_V2,
        borsh::to_vec(signer).unwrap(),
        Some(borsh::to_vec(&ids).unwrap()),
    );
    Ok(())
}

pub(super) fn refute(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    binding: &Digest,
    proof_bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    if !builder.extras.kernel_route.as_ref().is_some_and(|e| e.opv.is_some()) {
        return Err(refused("model artifact statements require palw_panel_free_v1"));
    }
    let id = Hash64::from_bytes(*binding);
    let row = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .model_artifact_binding_header_v2(&id)
        .ok_or_else(|| refused("no model artifact statement"))?;
    if row.refuted || ctx.daa_score >= row.final_daa {
        return Err(refused("model artifact statement is no longer refutable"));
    }
    let challenger = builder.state.bonds.get(signer).unwrap();
    if builder.state.bonds.get(&row.binder).is_some_and(|b| b.operator_id == challenger.operator_id) {
        return Err(refused("a model statement cannot pay its own operator a refutation bounty"));
    }
    let payout = challenger.payout_payload;
    let work = model_artifact_work_v2(row.program_bytes_len as usize, proof_bytes.len())
        .ok_or_else(|| refused("model court work overflows"))?;
    let Some(fee) = charge(builder, ctx, signer, work, true)? else { return Ok(()) };
    let row = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .model_artifact_binding_v2(&id)
        .ok_or_else(|| refused("stored model row disappeared"))?;
    let judgement = (|| {
        let proof = decode_model_artifact_proof_v2(proof_bytes, row.pc_instances, row.max_proof_bytes)
            .map_err(|_| "invalid bounded model proof")?;
        let program =
            misaka_palw_tir::TirProgramV1::decode_canonical(&row.program_bytes).map_err(|_| "invalid stored model program")?;
        let d = descriptor(&row.descriptor).ok_or("invalid stored descriptor")?;
        let inventory = PalwTirModelInventoryV2::new(&d, &program).map_err(|_| "invalid stored model scope")?;
        verify_artifact_mismatch_v2(&inventory, row.model_inventory_root, row.kernel_param_root, &proof)
    })();
    if judgement.is_err() {
        builder.slash_bond(*signer, fee)?;
        return Ok(());
    }
    let slashed = builder.slash_bond(row.binder, row.reserved as u128)?;
    builder.add_kernel_payout(payout, slashed.saturating_mul(PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1) / 1000)?;
    let refuted = ModelArtifactBindingRowV2 { reserved: 0, refuted: true, ..row };
    builder.write_kernel_row(
        PALW_MODEL_ARTIFACT_BINDINGS_TABLE_V2,
        borsh::to_vec(&id).unwrap(),
        Some(borsh::to_vec(&refuted).unwrap()),
    );
    Ok(())
}

/// Return the ordinary candidate object and only its scoped ephemeral attestation. The PC
/// root is never added to the global mature-root list; signatures still cover the original wire.
pub(super) fn candidate(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    object: &KernelRouteObjectV1,
) -> Result<Option<(KernelRouteObjectV1, Hash64)>, PalwStateV2Error> {
    let KernelRouteObjectV1::RegisterModelConformanceClassV2 { binding, descriptor, program_bytes, plan, param_commitments } = object
    else {
        return Ok(None);
    };
    let id = Hash64::from_bytes(*binding);
    let row = builder
        .state
        .kernel_route
        .as_ref()
        .and_then(|r| r.model_artifact_binding_header_v2(&id))
        .ok_or_else(|| refused("candidate has no model statement"))?;
    if !matches!(row.state_at(ctx.daa_score), ArtifactBindingStateV1::Matured | ArtifactBindingStateV1::Final)
        || row.descriptor != *descriptor
        || row.program_root != misaka_palw_kernel::public::program_root_v1(program_bytes)
        || row.kernel_param_root.as_bytes() != param_commitments.root()
    {
        return Err(refused("candidate does not match a matured model artifact scope"));
    }
    Ok(Some((
        KernelRouteObjectV1::RegisterConformanceClass {
            descriptor: *descriptor,
            program_bytes: program_bytes.clone(),
            plan: plan.clone(),
            param_commitments: param_commitments.clone(),
        },
        id,
    )))
}
