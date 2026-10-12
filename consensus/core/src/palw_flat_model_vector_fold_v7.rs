//! Protected flat candidate seal/reveal: consumer-owned signatures, capital,
//! persisted quotas, shared processing budget and once-only atomic settlement.
use super::palw_flat_model_vector_fold_v3::{charge, enabled};
use super::*;
use crate::palw_flat_model_vector_v3::*;
use crate::palw_flat_model_vector_v7::*;
use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
use crate::palw_onboarding_v1::PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1;
use misaka_palw_kernel::{
    flat_seal7::{FlatSealRowV7, check_flat_seal_v7},
    hash::Digest,
};

fn refused(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(s.into())
}
pub(super) fn metadata_work(bytes: usize) -> Option<u64> {
    crate::palw_model_artifact_v2::model_artifact_work_v2(0, bytes.checked_add(1024)?)
}
impl PalwChainStateV2 {
    pub fn flat_model_seal_reserved_v7(&self, bond: &PalwBondKeyV2) -> u128 {
        self.kernel_route.as_ref().map(|r| r.flat_model_seal_reserved_v7(bond)).unwrap_or(0)
    }
}
fn protected_header(builder: &TransitionBuilder<'_>, id: &Hash64, now: u64) -> Result<FlatModelVectorHeaderV3, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = r.flat_model_vector_header_v3(id).ok_or_else(|| refused("no protected flat vector"))?;
    if !r.flat_model_vector_protected_v7(id) || h.refuted || r.flat_vector_unavailable_v7(id) || now >= h.liability_until {
        return Err(refused("protected flat vector is not open"));
    }
    Ok(h)
}
fn independent(builder: &TransitionBuilder<'_>, signer: &PalwBondKeyV2, h: &FlatModelVectorHeaderV3) -> Result<(), PalwStateV2Error> {
    if builder.state.bonds.get(&h.poster).map(|b| b.operator_id) == builder.state.bonds.get(signer).map(|b| b.operator_id) {
        return Err(refused("flat poster operator cannot collect its bounty"));
    }
    Ok(())
}
pub(super) fn post_seal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    vector: &Digest,
    seal: &Digest,
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    let id = Hash64::from_bytes(*vector);
    let h = protected_header(builder, &id, ctx.daa_score)?;
    independent(builder, signer, &h)?;
    let actor = palw_kernel_bond_id_v1(signer);
    let route = builder.state.kernel_route.as_ref().unwrap();
    let mut ids = route.flat_model_seals_of_v7(signer);
    let count = route.flat_model_seal_count_v7();
    if ids.len() + route.flat_model_seals_of_v6(signer).len() + route.flat_model_seals_of_v9(signer).len()
        >= FLAT_NATIVE_SEALS_PER_BOND_V7
        || count.saturating_add(route.flat_model_seal_count_v6()).saturating_add(route.flat_model_seal_count_v9())
            >= FLAT_NATIVE_SEALS_MAX_V7
        || route.flat_model_seal_v7(&id, &actor).is_some()
    {
        return Err(refused("protected flat seal slot already occupied or full"));
    }
    let mature = ctx.daa_score.checked_add(FLAT_NATIVE_SEAL_MATURITY_DAA_V7).ok_or_else(|| refused("flat seal clock overflow"))?;
    let expires = ctx
        .daa_score
        .checked_add(FLAT_NATIVE_SEAL_TTL_DAA_V7)
        .ok_or_else(|| refused("flat seal clock overflow"))?
        .min(h.liability_until);
    if mature >= expires {
        return Err(refused("flat seal has no reveal window"));
    }
    let deposit = super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?.seal_deposit;
    let Some(fee) = charge(builder, ctx, signer, metadata_work(130).unwrap(), false)? else { return Ok(()) };
    builder.slash_bond(*signer, fee)?;
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < deposit as u128 {
        return Ok(());
    }
    let row = FlatNativeSealRowV7 {
        seal: FlatSealRowV7 {
            version: 7,
            vector: *vector,
            actor,
            seal: *seal,
            posted_daa: ctx.daa_score,
            mature_daa: mature,
            expires_daa: expires,
            spent: false,
        },
        bond: *signer,
        reserved: deposit,
    };
    ids.push(id);
    builder.write_kernel_row(FLAT_SEAL_TABLE_V7, borsh::to_vec(&(id, actor)).unwrap(), Some(borsh::to_vec(&row).unwrap()));
    builder.write_kernel_row(FLAT_SEAL_ACTOR_TABLE_V7, borsh::to_vec(signer).unwrap(), Some(borsh::to_vec(&ids).unwrap()));
    builder.write_kernel_row(FLAT_SEAL_COUNT_TABLE_V7, Vec::new(), Some(borsh::to_vec(&(count + 1)).unwrap()));
    Ok(())
}
pub(super) fn reveal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    vector: &Digest,
    salt: &Digest,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    let id = Hash64::from_bytes(*vector);
    let h = protected_header(builder, &id, ctx.daa_score)?;
    independent(builder, signer, &h)?;
    if bytes.len() as u64 > h.max_filing_bytes || bytes.len() > FLAT_MODEL_VECTOR_MAX_PROOF_BYTES_V3 {
        return Err(refused("protected flat filing exceeds admitted carrier"));
    }
    let actor = palw_kernel_bond_id_v1(signer);
    let prior_seal = builder.state.kernel_route.as_ref().unwrap().flat_model_seal_v7(&id, &actor);
    let owns_mature = prior_seal.as_ref().is_some_and(|r| {
        r.bond == *signer && !r.seal.spent && ctx.daa_score >= r.seal.mature_daa && ctx.daa_score < r.seal.expires_daa
    });
    // One proof-reserve reservation covers metadata/hash and the deferred court.
    // An exhausted admission share must not block a mature honest proof. Refused
    // seals pay the same derived dismissal tariff without decoding the program.
    let work = flat_model_vector_work_v3(
        h.program_bytes_len,
        h.pc_instances,
        bytes.len().checked_add(h.post_bytes_len as usize).ok_or_else(|| refused("protected flat carrier overflow"))?,
    )
    .and_then(|w| w.checked_add(h.max_court_work))
    .and_then(|w| metadata_work(bytes.len()).and_then(|m| w.checked_add(m)))
    .ok_or_else(|| refused("protected flat court work overflow"))?;
    let work =
        if owns_mature { work } else { metadata_work(bytes.len()).ok_or_else(|| refused("mixed seal metadata work overflow"))? };
    let Some(fee) = charge(builder, ctx, signer, work, owns_mature)? else { return Ok(()) };
    let row = prior_seal;
    let authenticated = row
        .as_ref()
        .is_some_and(|r| r.bond == *signer && check_flat_seal_v7(&r.seal, vector, &actor, salt, bytes, ctx.daa_score).is_ok());
    if !authenticated {
        builder.slash_bond(*signer, fee)?;
        return Ok(());
    }
    let mut seal_row = row.unwrap();
    let proven = (|| -> Result<(), String> {
        let fault = misaka_palw_kernel::flat_court7::FlatFaultV7::from_bytes(bytes).map_err(|e| e.to_string())?;
        let v = builder.state.kernel_route.as_ref().unwrap().flat_model_vector_view_v7(&id)?;
        let b = misaka_palw_kernel::flat_court7::flat_fault_court_bounds_v7(&v.context(), &fault).map_err(|e| format!("{e:?}"))?;
        if b.work > h.max_court_work || b.filing_bytes > h.max_filing_bytes || b.scratch_bytes > h.max_court_scratch_bytes {
            return Err("protected flat exact court outside persisted scope".into());
        }
        misaka_palw_kernel::flat_court7::verify_flat_fault_v7(&v.context(), &fault).map(|_| ()).map_err(|e| format!("{e:?}"))
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
        .ok_or_else(|| refused("protected vector disappeared"))?;
    let payout = builder.state.bonds.get(signer).unwrap().payout_payload;
    let slash = builder.slash_bond(h.poster, h.reserved as u128)?;
    builder.add_kernel_payout(payout, slash.saturating_mul(PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1) / 1000)?;
    row.header.refuted = true;
    row.header.reserved = 0;
    seal_row.seal.spent = true;
    seal_row.reserved = 0;
    builder.write_kernel_row(FLAT_MODEL_VECTOR_TABLE_V3, borsh::to_vec(&id).unwrap(), Some(borsh::to_vec(&row).unwrap()));
    builder.write_kernel_row(FLAT_SEAL_TABLE_V7, borsh::to_vec(&(id, actor)).unwrap(), Some(borsh::to_vec(&seal_row).unwrap()));
    Ok(())
}
pub(super) fn expire(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    vector: &Digest,
    actor: &Digest,
) -> Result<(), PalwStateV2Error> {
    enabled(builder, ctx)?;
    let id = Hash64::from_bytes(*vector);
    let route = builder.state.kernel_route.as_ref().unwrap();
    if !route.flat_model_vector_protected_v7(&id) {
        return Err(refused("not an explicit v7 sealed vector"));
    }
    let row = route.flat_model_seal_v7(&id, actor).ok_or_else(|| refused("no flat seal to expire"))?;
    let h = route.flat_model_vector_header_v3(&id).ok_or_else(|| refused("no sealed flat vector"))?;
    let closed = h.refuted || route.flat_vector_unavailable_v7(&id);
    if !closed && ctx.daa_score < row.seal.expires_daa {
        return Err(refused("flat seal expiry is not due"));
    }
    let mut ids = route.flat_model_seals_of_v7(&row.bond);
    let count = route.flat_model_seal_count_v7().checked_sub(1).ok_or_else(|| refused("flat seal counter underflow"))?;
    if !ids.contains(&id) {
        return Err(refused("flat seal actor catalog mismatch"));
    }
    ids.retain(|v| *v != id);
    let Some(fee) = charge(builder, ctx, signer, metadata_work(130).unwrap(), false)? else { return Ok(()) };
    builder.slash_bond(*signer, fee)?;
    if !closed && !row.seal.spent {
        builder.slash_bond(row.bond, row.reserved as u128)?;
    }
    builder.write_kernel_row(FLAT_SEAL_TABLE_V7, borsh::to_vec(&(id, actor)).unwrap(), None);
    builder.write_kernel_row(
        FLAT_SEAL_ACTOR_TABLE_V7,
        borsh::to_vec(&row.bond).unwrap(),
        if ids.is_empty() { None } else { Some(borsh::to_vec(&ids).unwrap()) },
    );
    builder.write_kernel_row(
        FLAT_SEAL_COUNT_TABLE_V7,
        Vec::new(),
        if count == 0 { None } else { Some(borsh::to_vec(&count).unwrap()) },
    );
    Ok(())
}

/// Fee and structural processing reservation precede body/program/PC decoding.
/// The same original-capital catalog holds protected v3/v6/v7 statements.
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
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut ids = r.flat_model_vectors_of_v3(signer);
    if ids.len() + r.model_vectors_of_v2(signer).len() >= crate::palw_model_vector_v2::MODEL_VECTORS_PER_BOND_V2
        || bytes.len() > FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3
    {
        return Err(refused("shared vector catalog/carrier full"));
    }
    let statement = r.model_artifact_binding_header_v2(&binding).ok_or_else(|| refused("mixed vector has no scoped model"))?;
    if !matches!(
        statement.state_at(ctx.daa_score),
        crate::palw_onboarding_v1::ArtifactBindingStateV1::Matured | crate::palw_onboarding_v1::ArtifactBindingStateV1::Final
    ) || r.model_artifact_candidate_binding_v2(&class) != Some(binding)
        || r.template().schedule.standing_at(&statement.descriptor, ctx.daa_score)
            != misaka_palw_kernel::descriptor::KernelStandingV1::Active
    {
        return Err(refused("mixed model/candidate does not stand"));
    }
    let work = flat_model_vector_work_v3(statement.program_bytes_len, statement.pc_instances, bytes.len())
        .ok_or_else(|| refused("mixed post work overflow"))?;
    let Some(fee) = charge(builder, ctx, signer, work, false)? else { return Ok(()) };
    builder.slash_bond(*signer, fee)?;
    let checked = (|| -> Result<_, String> {
        let p = decode_flat_model_vector_post_v7(bytes).map_err(|e| e.to_string())?;
        let v = checked_flat_model_vector_v7(builder.state.kernel_route.as_ref().unwrap(), class, binding, &p)?;
        if v.trace.params.by_instance.len() != statement.pc_instances as usize {
            return Err("mixed PC count changed".into());
        }
        let worst = flat_model_vector_work_v3(
            statement.program_bytes_len,
            statement.pc_instances,
            bytes.len().checked_add(v.scope.worst_court_bytes as usize).ok_or("mixed carrier overflow")?,
        )
        .and_then(|w| w.checked_add(v.scope.worst_court_work))
        .and_then(|w| metadata_work(v.scope.worst_court_bytes as usize).and_then(|m| w.checked_add(m)))
        .and_then(|w| {
            metadata_work(
                crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES
                    + 3 * crate::palw_kernel_route_v1::PALW_FLAT_PROOF_CHUNK_MAX_BYTES_V7,
            )
            .and_then(|m| w.checked_add(m))
        })
        .ok_or("mixed guaranteed work overflow")?;
        let policy = super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder).map_err(|e| e.to_string())?;
        let runs = if policy.prosecution_reserved_runs() == 0 {
            policy.max_adjudications_per_block
        } else {
            policy.prosecution_reserved_runs()
        };
        // A protected carrier's last own chunk and its court consume two runs.
        // Promise that route only under a policy that can retain both of them.
        if runs < 2
            || v.scope.worst_court_bytes > FLAT_MODEL_VECTOR_MAX_PROOF_BYTES_V3 as u64
            || worst > policy.guaranteed_proof_work_v1()
        {
            return Err("mixed terminal court cannot fit guaranteed reserve".into());
        }
        Ok((p, v.scope))
    })();
    let Ok((p, scope)) = checked else { return Ok(()) };
    let root = p.trace_root(class.as_bytes(), binding.as_bytes());
    let id = p.id(class.as_bytes(), binding.as_bytes());
    if builder.state.kernel_route.as_ref().unwrap().flat_model_vector_header_v3(&id).is_some() {
        return Ok(());
    }
    let reserve = crate::palw_onboarding_v1::PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1;
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    if collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)) < reserve as u128 {
        return Ok(());
    }
    let h = FlatModelVectorHeaderV3 {
        trace_root: root,
        class,
        binding,
        profile: p.profile,
        scope_root: p.scope_root,
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
            .checked_add(crate::palw_onboarding_v1::PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1)
            .ok_or_else(|| refused("mixed horizon overflow"))?,
        reserved: reserve,
        refuted: false,
    };
    ids.push(id);
    builder.write_kernel_row(
        FLAT_MODEL_VECTOR_TABLE_V3,
        borsh::to_vec(&id).unwrap(),
        Some(borsh::to_vec(&FlatModelVectorRowV3 { header: h, post: bytes.to_vec() }).unwrap()),
    );
    builder.write_kernel_row(FLAT_MODEL_VECTOR_BOND_TABLE_V3, borsh::to_vec(signer).unwrap(), Some(borsh::to_vec(&ids).unwrap()));
    builder.write_kernel_row(FLAT_PROTECTED_TABLE_V7, borsh::to_vec(&id).unwrap(), Some(borsh::to_vec(&true).unwrap()));
    Ok(())
}
