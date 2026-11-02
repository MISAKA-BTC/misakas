//! Nonexclusive authenticated family material: rooted multipart transport and exact-target
//! completion. Expiry releases storage, never invents a service default or slash.
use super::palw_court_family_supplier_fold_v1 as supplier;
use super::*;
#[path = "palw_court_family_draft_reservation_v2.rs"]
pub(crate) mod draft_reservations;
use crate::palw_court_family_material_v1 as m;
use crate::palw_native_resource_v1::{NativeRetainedV1 as Held, NativeWireScanV1};
use borsh::{BorshDeserialize, BorshSerialize};
pub use draft_reservations::palw_family_material_begin_quote_v2;
fn refused(s: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.to_string())
}
fn native(e: crate::palw_native_resource_v1::NativeDecodeErrorV1<String>) -> PalwStateV2Error {
    match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e),
    }
}
fn read<T: NativeWireScanV1 + BorshDeserialize>(state: &PalwChainStateV2, key: &[u8]) -> Result<Option<Held<T>>, PalwStateV2Error> {
    let Some(route) = state.kernel_route_state_v1() else { return Ok(None) };
    let Some(bytes) = route.aux.get(&(m::FAMILY_MATERIAL_TABLE_V1, key.to_vec())) else { return Ok(None) };
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(bytes, || borsh::from_slice::<T>(bytes))
        .map(Some)
        .map_err(|e| match e {
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e),
        })
}
fn remove(b: &mut TransitionBuilder<'_>, key: Vec<u8>) -> Result<(), PalwStateV2Error> {
    b.write_kernel_row(m::FAMILY_MATERIAL_TABLE_V1, key, None)
}
fn clear_draft(b: &mut TransitionBuilder<'_>, id: Hash64, draft: &m::FamilyMaterialDraftV1) -> Result<(), PalwStateV2Error> {
    for offset in (0..draft.offset).step_by(m::FAMILY_MATERIAL_PART_BYTES_V1) {
        remove(b, m::material_aux_key_v1(2, id, Some(offset)))?;
    }
    remove(b, m::material_aux_key_v1(1, id, None))
}
fn put<T: BorshSerialize>(b: &mut TransitionBuilder<'_>, key: &[u8], row: &T) -> Result<(), PalwStateV2Error> {
    b.write_kernel_row_encoded_v1(m::FAMILY_MATERIAL_TABLE_V1, key, Some(row))
}
/// Install the canonical preclaim table witness in the same original journal.
/// This records an EOF inventory only; it does not mark supplier service complete.
pub(super) fn install_family_table_inventory_v1(
    b: &mut TransitionBuilder<'_>,
    claim: Hash64,
    bytes: &[u8],
    carrier: crate::tx::TransactionOutpoint,
) -> Result<(), PalwStateV2Error> {
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state, claim)
        .ok_or_else(|| refused("family table inventory original missing"))?;
    let sem = b.state.claim_semantics_v1(&claim).ok_or_else(|| refused("family table immutable snapshot missing"))?;
    let class = frame.original_v1().job.class_id;
    let record = b.state.tir_class_v1(&class).ok_or_else(|| refused("family table native record missing"))?;
    m::prepare_family_table_inventory_v1(bytes, sem, record.logits_vocab).map_err(native)?;
    let hard = frame
        .plan_v1()
        .terms
        .accepted_daa
        .checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
        .ok_or_else(|| refused("family table original hard overflow"))?;
    let until = crate::palw_court_service_state_v2::tir_service_required_material_until_v1(frame, hard).map_err(refused)?;
    let key = m::material_aux_key_v1(4, claim, None);
    let _scratch =
        crate::palw_native_resource_v1::reserve_native_requests_v1(bytes.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let rules = b.extras.tir_public_allocation_rules_v1.ok_or_else(|| refused("family table original native rules absent"))?;
    let allocation_rules_v2 = Some(m::FamilyOriginalRuleContextV2::from_native_v2(rules));
    let row = m::FamilyTableInventoryRowV1 {
        claim,
        carrier,
        allocation_id: frame.plan_v1().allocation_id,
        allocation_rules_v2,
        bytes: bytes.to_vec(),
        material_until_daa: until,
    };
    if let Some(old) = read::<m::FamilyTableInventoryRowV1>(&b.state, &key)? {
        if *old != row { return Err(refused("family table immutable inventory collision")) } else { return Ok(()) }
    }
    put(b, &key, &row)
}
/// The original TFO2 carrier already supplied this authenticated literal.
/// Nonexclusive availability discharges the exact target, without inventing an
/// uploader, a second transport, a service opportunity, or an accelerated fee.
pub(super) fn complete_family_table_inventory_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_court_family_supplier_v1::FamilySupplierRoleV1 as Role;
    let contracts = b.state.family_supplier_rows_v1()?;
    for (id, contract) in
        contracts.iter().filter(|(_, row)| row.consent.terms.claim == claim && row.consent.terms.role == Role::SemanticTable)
    {
        let inventory = m::derive_contract_family_inventory_classified_v3(&b.state, contract).map_err(native)?;
        if inventory.id_v1() != contract.consent.terms.inventory {
            return Err(refused("admission table changes signed native inventory"));
        }
        let witness = read::<m::FamilyTableInventoryRowV1>(&b.state, &m::material_aux_key_v1(4, claim, None))?
            .ok_or_else(|| refused("admission table witness absent"))?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state, claim)
            .ok_or_else(|| refused("admission table original absent"))?;
        if witness.claim != claim
            || witness.allocation_id != frame.plan_v1().allocation_id
            || witness.material_until_daa != contract.consent.terms.material_until_daa
            || ctx.daa_score > contract.consent.terms.respond_until_daa
        {
            return Err(refused("admission table origin/horizon differs"));
        }
        let target = m::FamilyMaterialTargetV1::SemanticTable;
        m::authenticate_family_material_classified_v1(&b.state, &inventory, target, &witness.bytes).map_err(native)?;
        let target_id = inventory.target_id_v1(target).map_err(refused)?;
        let digest = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&witness.bytes);
        supplier::complete_family_target_v1(b, ctx, *id, target_id, digest)?;
    }
    Ok(())
}
pub(super) fn apply_family_material_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    s: &m::SignedFamilyMaterialV1,
) -> Result<(), PalwStateV2Error> {
    let checkpoint = b.try_checkpoint()?;
    let first = b.entries.len();
    let result = apply_inner(b, ctx, s).and_then(|()| {
        if !b.params.claim_local_settlement_active_at(ctx.daa_score) {
            return Ok(());
        }
        let contract = supplier::family_contract_for_material_v1(&b.state, s.contract)?
            .ok_or_else(|| refused("executed family material lost its contract"))?;
        let network = b.extras.tir_public_network_domain_v1.ok_or_else(|| refused("executed family material lost network"))?;
        let message = s.signing_message_v1(network).map_err(refused)?;
        super::palw_claim_local_material_projection_v2::mark(
            b,
            contract.consent.terms.claim,
            super::palw_claim_local_material_projection_v2::FamilyEventV2::Delivery(message),
            first,
        )
    });
    if result.is_err() {
        b.restore(checkpoint);
    }
    if let Some((message, spent, reserved)) = b.court_family_charged_v2.take()
        && result.is_ok()
        && reserved
        && b.extras.tir_public_network_domain_v1.is_some_and(|n| s.signing_message_v1(n).ok() == Some(message))
    {
        let remaining = &mut b.court_family_unreserved_service_spent_v2;
        remaining.bytes -= spent.bytes;
        remaining.work -= spent.work;
        remaining.io -= spent.io;
        remaining.controls -= spent.controls;
    }
    result
}
fn apply_inner(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    s: &m::SignedFamilyMaterialV1,
) -> Result<(), PalwStateV2Error> {
    if !b.extras.staged_tir_route_active || !b.params.family_supplier_active_at(ctx.daa_score) || !b.extras.family_supplier_active {
        return Err(refused("family material intake or dedicated supplier fence is dormant"));
    }
    let network = b.extras.tir_public_network_domain_v1.ok_or_else(|| refused("family material network absent"))?;
    let bond = b.state.bond(&s.submitter).ok_or(PalwStateV2Error::MissingBond(s.submitter))?;
    if !matches!(bond.status, PalwBondStatusV2::Active) || bond.collateral < b.params.min_collateral_sompi() {
        return Err(refused("family uploader must be an Active bond at the existing floor"));
    }
    s.verify_v1(network, &bond.pubkey).map_err(refused)?;
    let contract =
        supplier::family_contract_for_material_v1(&b.state, s.contract)?.ok_or_else(|| refused("family material contract absent"))?;
    let terms = &contract.consent.terms;
    if ctx.daa_score < contract.accepted_daa || ctx.daa_score > terms.respond_until_daa || ctx.daa_score > terms.material_until_daa {
        return Err(refused("family material outside accepted response/material interval"));
    }
    let inventory = m::derive_contract_family_inventory_classified_v3(&b.state, &contract).map_err(native)?;
    if inventory.id_v1() != terms.inventory
        || inventory.source_v1() != terms.source
        || inventory.relative_graph_v1() != terms.relative_graph
        || inventory.max_material_bytes_v1() != terms.max_material_bytes
    {
        return Err(refused("family material contract differs from native inventory"));
    }
    let bound = inventory.target_bound_v1(s.target).map_err(refused)?;
    let target_id = inventory.target_id_v1(s.target).map_err(refused)?;
    let body_key = m::material_aux_key_v1(3, target_id, None);
    if let Some(body) = read::<m::FamilyMaterialBodyV1>(&b.state, &body_key)? {
        check_body(&b.state, &inventory, s.target, &body, ctx.daa_score)?;
        supplier::complete_family_target_v1(b, ctx, s.contract, target_id, body.digest)?;
        // A completed authenticated target may discharge another independently signed
        // contract. It is not retransferred, repriced or accepted on a bare boolean.
        if let m::FamilyMaterialActionV1::Finish { draft } = s.action
            && let Some(old) = read::<m::FamilyMaterialDraftV1>(&b.state, &m::material_aux_key_v1(1, draft, None))?
            && old.submitter == s.submitter
            && old.contract == s.contract
            && old.target == s.target
        {
            clear_draft(b, draft, &old)?;
        }
        return Ok(());
    }
    match &s.action {
        m::FamilyMaterialActionV1::Begin { total_bytes, body_hash }
        | m::FamilyMaterialActionV1::BeginReserved { total_bytes, body_hash, .. } => {
            if *total_bytes == 0 || *total_bytes > bound || *body_hash == Hash64::default() {
                return Err(refused("family material declared width/hash differs from native target bound"));
            }
            let id = match &s.action {
                m::FamilyMaterialActionV1::BeginReserved { quote, .. } => {
                    m::family_material_draft_id_v2(s.contract, s.target, s.submitter, *total_bytes, *body_hash, quote)
                }
                _ => m::family_material_draft_id_v1(s.contract, s.target, s.submitter, *total_bytes, *body_hash),
            }
            .map_err(refused)?;
            let key = m::material_aux_key_v1(1, id, None);
            if read::<m::FamilyMaterialDraftV1>(&b.state, &key)?.is_some() {
                return Ok(());
            }
            let schedule = crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1()
                .map_err(|_| refused("family material storage schedule absent"))?;
            let fee = match draft_reservations::begin_fee(b, ctx, s, &contract, *total_bytes, *body_hash)? {
                Some(fee) => fee,
                None => total_bytes.checked_mul(schedule.sompi_per_byte).ok_or_else(|| refused("family storage fee overflow"))?,
            };
            let collateral = b.state.bond(&s.submitter).ok_or(PalwStateV2Error::MissingBond(s.submitter))?.collateral;
            if supplier::family_free_principal_v1(b, &s.submitter, ctx.daa_score)? < u128::from(fee) {
                return Err(refused("family material storage debit exceeds uncommitted principal"));
            }
            if collateral < fee || b.slash_bond(s.submitter, u128::from(fee))? != fee {
                return Err(refused("family material storage debit not paid in full"));
            }
            let expires = ctx
                .daa_score
                .checked_add(schedule.draft_ttl_daa)
                .ok_or_else(|| refused("family material expiry overflow"))?
                .min(terms.respond_until_daa);
            put(
                b,
                &key,
                &m::FamilyMaterialDraftV1 {
                    contract: s.contract,
                    target: s.target,
                    submitter: s.submitter,
                    total_bytes: *total_bytes,
                    body_hash: *body_hash,
                    offset: 0,
                    expires_daa: expires,
                    material_until_daa: terms.material_until_daa,
                },
            )?;
        }
        m::FamilyMaterialActionV1::Part { draft, offset, bytes } => {
            let key = m::material_aux_key_v1(1, *draft, None);
            let old = read::<m::FamilyMaterialDraftV1>(&b.state, &key)?.ok_or_else(|| refused("family material draft absent"))?;
            check_draft(&b.state, &old, *draft, s, ctx.daa_score, bound)?;
            if bytes.is_empty()
                || bytes.len() > m::FAMILY_MATERIAL_PART_BYTES_V1
                || offset % m::FAMILY_MATERIAL_PART_BYTES_V1 as u64 != 0
            {
                return Err(refused("family material noncanonical part width/offset"));
            }
            let width = (old.total_bytes.checked_sub(*offset).ok_or_else(|| refused("family part outside declared native width"))?)
                .min(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64);
            if bytes.len() as u64 != width {
                return Err(refused("family material part does not fill exact canonical chunk"));
            }
            let part_key = m::material_aux_key_v1(2, *draft, Some(*offset));
            if *offset < old.offset {
                let part = read::<m::FamilyMaterialPartV1>(&b.state, &part_key)?
                    .ok_or_else(|| refused("family material accepted part missing"))?;
                if part.bytes != *bytes {
                    return Err(refused("family material retry changes accepted bytes"));
                }
                return Ok(());
            }
            if *offset != old.offset {
                return Err(refused("family material part skips accepted offset"));
            }
            b.native_reservation.grow_v1(bytes.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
            put(
                b,
                &part_key,
                &m::FamilyMaterialPartV1 { draft: *draft, offset: *offset, bytes: bytes.clone(), expires_daa: old.expires_daa },
            )?;
            let mut next = (*old).clone();
            next.offset = next.offset.checked_add(bytes.len() as u64).ok_or_else(|| refused("family material offset overflow"))?;
            put(b, &key, &next)?;
        }
        m::FamilyMaterialActionV1::Finish { draft } => {
            let key = m::material_aux_key_v1(1, *draft, None);
            let old = read::<m::FamilyMaterialDraftV1>(&b.state, &key)?.ok_or_else(|| refused("family material draft absent"))?;
            check_draft(&b.state, &old, *draft, s, ctx.daa_score, bound)?;
            if old.offset != old.total_bytes {
                return Err(refused("family material Finish before actual EOF"));
            }
            let _lease = crate::palw_native_resource_v1::reserve_native_requests_v1(old.total_bytes)
                .map_err(PalwStateV2Error::NativeResource)?;
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(usize::try_from(old.total_bytes).map_err(refused)?).map_err(|e| {
                PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                    m::FamilyMaterialBodyV1,
                >("family material receive buffer", &e.to_string()))
            })?;
            for offset in (0..old.total_bytes).step_by(m::FAMILY_MATERIAL_PART_BYTES_V1) {
                let part = read::<m::FamilyMaterialPartV1>(&b.state, &m::material_aux_key_v1(2, *draft, Some(offset)))?
                    .ok_or_else(|| refused("family material rooted part absent"))?;
                if part.draft != *draft
                    || part.offset != offset
                    || part.expires_daa != old.expires_daa
                    || part.bytes.len() as u64 != (old.total_bytes - offset).min(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64)
                {
                    return Err(refused("family material rooted part changes draft/EOF"));
                }
                bytes.extend_from_slice(&part.bytes);
            }
            let digest = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&bytes);
            if digest != old.body_hash {
                return Err(refused("family material body does not reproduce accepted digest"));
            }
            let authenticated =
                m::authenticate_family_material_classified_v1(&b.state, &inventory, s.target, &bytes).map_err(native)?;
            // Publish source inputs into the existing shared-data consumers only inside
            // their existing live-clock window. Semantic/raw bytes remain rooted below.
            let c = crate::palw_court_shared_tir_state_v1::context(&b.state, terms.claim).map_err(refused)?.clone();
            if ctx.daa_score < c.hard_daa {
                match authenticated {
                    m::AuthenticatedFamilyMaterialV1::Ids { owner } => {
                        super::palw_court_shared_tir_fold_v1::share_ids(b, ctx, terms.claim, owner.rows_root_v1(), owner.ids_v1())?;
                    }
                    m::AuthenticatedFamilyMaterialV1::Body => {
                        if let m::FamilyMaterialTargetV1::Input(target) = s.target {
                            use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
                            let id = target.id_v1(terms.claim).map_err(refused)?;
                            let key = Key::CourtMaterial(terms.claim, id);
                            if b.state.staged_court_row_v1(&key).is_none() {
                                b.native_reservation.grow_v1(bytes.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
                                b.write_staged_row_v1(
                                    key,
                                    Some(Row::CourtMaterial(
                                        Box::new(crate::palw_court_shared_material_intake_v1::MaterialRowV1 {
                                            version: 1,
                                            claim: terms.claim,
                                            scope_id: c.scope_id,
                                            target,
                                            target_id: id,
                                            bytes: bytes.clone(),
                                            digest,
                                            block: ctx.block,
                                            accepted_daa: ctx.daa_score,
                                            completed_by: s.submitter,
                                            material_until_daa: terms.material_until_daa,
                                        })
                                        .into(),
                                    )),
                                )?;
                            }
                        }
                    }
                }
            }
            put(
                b,
                &body_key,
                &m::FamilyMaterialBodyV1 {
                    contract: s.contract,
                    target: s.target,
                    inventory: inventory.id_v1(),
                    bytes,
                    digest,
                    completed_by: s.submitter,
                    accepted_daa: ctx.daa_score,
                    material_until_daa: terms.material_until_daa,
                },
            )?;
            supplier::complete_family_target_v1(b, ctx, s.contract, target_id, digest)?;
            clear_draft(b, *draft, &old)?;
        }
    }
    Ok(())
}
fn check_draft(
    state: &PalwChainStateV2,
    d: &m::FamilyMaterialDraftV1,
    id: Hash64,
    s: &m::SignedFamilyMaterialV1,
    now: u64,
    bound: u64,
) -> Result<(), PalwStateV2Error> {
    if d.contract != s.contract
        || d.target != s.target
        || d.submitter != s.submitter
        || d.total_bytes == 0
        || d.total_bytes > bound
        || d.offset > d.total_bytes
        || now > d.expires_daa
        || (m::family_material_draft_id_v1(d.contract, d.target, d.submitter, d.total_bytes, d.body_hash).map_err(refused)? != id
            && !draft_reservations::matches_independent_id(state, d, id)?)
    {
        return Err(refused("family material draft scope/width/expiry"));
    }
    Ok(())
}
fn check_body(
    state: &PalwChainStateV2,
    inventory: &m::NativeFamilyInventoryV1,
    target: m::FamilyMaterialTargetV1,
    body: &m::FamilyMaterialBodyV1,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    if body.target != target
        || body.inventory != inventory.id_v1()
        || body.accepted_daa > now
        || now > body.material_until_daa
        || crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body.bytes) != body.digest
    {
        return Err(refused("family material canonical body changes scope/digest/retention"));
    }
    m::authenticate_family_material_classified_v1(state, inventory, target, &body.bytes).map_err(native)?;
    Ok(())
}
pub(super) fn sweep_family_material_v1(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let count =
        b.state.kernel_route_state_v1().map_or(0, |r| r.aux.keys().filter(|(table, _)| *table == m::FAMILY_MATERIAL_TABLE_V1).count());
    b.native_reservation
        .grow_v1((count as u64).checked_mul(128).ok_or_else(|| refused("family sweep key count overflow"))?)
        .map_err(PalwStateV2Error::NativeResource)?;
    let scoped = b.params.claim_local_settlement_active_at(ctx.daa_score);
    b.native_reservation
        .grow_v1(
            (count as u64)
                .checked_mul(std::mem::size_of::<(Vec<u8>, Hash64)>() as u64)
                .ok_or_else(|| refused("family expiry owner inventory"))?,
        )
        .map_err(PalwStateV2Error::NativeResource)?;
    let mut expired = Vec::with_capacity(count);
    if let Some(route) = b.state.kernel_route_state_v1() {
        for ((table, key), raw) in &route.aux {
            if *table != m::FAMILY_MATERIAL_TABLE_V1 {
                continue;
            }
            let deadline = match (key.len(), key.first()) {
                (65, Some(1)) => {
                    read::<m::FamilyMaterialDraftV1>(&b.state, key)?.ok_or_else(|| refused("family draft disappeared"))?.expires_daa
                }
                (73, Some(2)) => {
                    read::<m::FamilyMaterialPartV1>(&b.state, key)?.ok_or_else(|| refused("family part disappeared"))?.expires_daa
                }
                (65, Some(4)) => {
                    read::<m::FamilyTableInventoryRowV1>(&b.state, key)?
                        .ok_or_else(|| refused("family table inventory disappeared"))?
                        .material_until_daa
                }
                (65, Some(3)) => {
                    read::<m::FamilyMaterialBodyV1>(&b.state, key)?
                        .ok_or_else(|| refused("family body disappeared"))?
                        .material_until_daa
                }
                (65, Some(5 | 6)) => {
                    let (claim, until) = if key[0] == 5 {
                        let row =
                            read::<m::FamilyDraftReservationV2>(&b.state, key)?.ok_or_else(|| refused("draft receipt absent"))?;
                        (row.claim, row.material_until_daa)
                    } else {
                        let row = read::<m::FamilyPrimaryDraftV2>(&b.state, key)?.ok_or_else(|| refused("primary receipt absent"))?;
                        (row.claim, row.material_until_daa)
                    };
                    if crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state, claim).is_some() {
                        continue;
                    }
                    until
                }
                _ => return Err(refused("unallocated family material row key")),
            };
            if ctx.daa_score > deadline {
                let claim = if scoped {
                    super::palw_claim_local_material_projection_v2::expiry_owner(b, key, raw)?
                } else {
                    Hash64::default()
                };
                expired.push((key.clone(), claim));
            }
        }
    }
    for (key, claim) in expired {
        let first = b.entries.len();
        remove(b, key)?;
        super::palw_claim_local_material_projection_v2::mark(
            b,
            claim,
            super::palw_claim_local_material_projection_v2::FamilyEventV2::Expiry,
            first,
        )?;
    }
    Ok(())
}

/// Snapshot/restart validation re-authenticates every retained completed body; a completion
/// map or byte checksum by itself cannot restore material availability.
pub(crate) fn validate_family_material_rows_v1(state: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    let Some(route) = state.kernel_route_state_v1() else { return Ok(()) };
    let now = state.last_point().map_or(0, |p| p.daa_score);
    for (table, key) in route.aux.keys() {
        if *table != m::FAMILY_MATERIAL_TABLE_V1 {
            continue;
        }
        match (key.len(), key.first()) {
            (65, Some(1)) => {
                let d = read::<m::FamilyMaterialDraftV1>(state, key)?.ok_or_else(|| refused("family draft disappeared"))?;
                let row = supplier::family_contract_for_material_v1(state, d.contract)?
                    .ok_or_else(|| refused("family draft lost signed contract"))?;
                let terms = &row.consent.terms;
                let inventory = m::derive_contract_family_inventory_classified_v3(state, &row).map_err(native)?;
                let bound = inventory.target_bound_v1(d.target).map_err(refused)?;
                let id = Hash64::from_bytes(key[1..].try_into().map_err(refused)?);
                let primary =
                    m::family_material_draft_id_v1(d.contract, d.target, d.submitter, d.total_bytes, d.body_hash).map_err(refused)?;
                if (id != primary && !draft_reservations::matches_independent_id(state, &d, id)?)
                    || *key != m::material_aux_key_v1(1, id, None)
                    || inventory.id_v1() != terms.inventory
                    || d.total_bytes == 0
                    || d.total_bytes > bound
                    || d.offset > d.total_bytes
                    || d.expires_daa > terms.respond_until_daa
                    || d.material_until_daa != terms.material_until_daa
                    || now > d.expires_daa
                {
                    return Err(refused("family restored draft changes native scope/clock"));
                }
                if draft_reservations::requires_reserved_begin_v2(&row)? {
                    let target_id = inventory.target_id_v1(d.target).map_err(refused)?;
                    let primary = read::<m::FamilyPrimaryDraftV2>(state, &m::material_aux_key_v1(6, target_id, None))?;
                    let is_primary = primary.is_some_and(|p| p.draft == id && d.submitter == terms.supplier);
                    let paid = read::<m::FamilyDraftReservationV2>(state, &m::material_aux_key_v1(5, id, None))?;
                    if !is_primary
                        && !paid.is_some_and(|p| p.draft == id && p.contract == d.contract && p.begin.submitter == d.submitter)
                    {
                        return Err(refused("restored draft lacks original prepaid slot or signed independent reservation"));
                    }
                }
                for offset in (0..d.offset).step_by(m::FAMILY_MATERIAL_PART_BYTES_V1) {
                    let p = read::<m::FamilyMaterialPartV1>(state, &m::material_aux_key_v1(2, id, Some(offset)))?
                        .ok_or_else(|| refused("family restored draft missing immutable chunk"))?;
                    if p.draft != id
                        || p.offset != offset
                        || p.expires_daa != d.expires_daa
                        || p.bytes.len() as u64 != (d.total_bytes - offset).min(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64)
                    {
                        return Err(refused("family restored chunk changes canonical EOF"));
                    }
                }
            }
            (73, Some(2)) => {
                let p = read::<m::FamilyMaterialPartV1>(state, key)?.ok_or_else(|| refused("family part disappeared"))?;
                let d = read::<m::FamilyMaterialDraftV1>(state, &m::material_aux_key_v1(1, p.draft, None))?
                    .ok_or_else(|| refused("family part has no live draft"))?;
                if *key != m::material_aux_key_v1(2, p.draft, Some(p.offset))
                    || p.offset % m::FAMILY_MATERIAL_PART_BYTES_V1 as u64 != 0
                    || p.bytes.is_empty()
                    || p.bytes.len() > m::FAMILY_MATERIAL_PART_BYTES_V1
                    || p.offset.checked_add(p.bytes.len() as u64).is_none_or(|end| end > d.offset)
                    || p.expires_daa != d.expires_daa
                    || now > p.expires_daa
                {
                    return Err(refused("family orphan or substituted material chunk"));
                }
            }
            (65, Some(4)) => {
                let row =
                    read::<m::FamilyTableInventoryRowV1>(state, key)?.ok_or_else(|| refused("family table inventory disappeared"))?;
                if *key != m::material_aux_key_v1(4, row.claim, None) || now > row.material_until_daa {
                    return Err(refused("family table inventory key/expiry"));
                }
                let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, row.claim)
                    .ok_or_else(|| refused("family table inventory original missing"))?;
                if row.allocation_id != frame.plan_v1().allocation_id {
                    return Err(refused("family table inventory changed original allocation"));
                }
                let sem = state.claim_semantics_v1(&row.claim).ok_or_else(|| refused("family table inventory snapshot missing"))?;
                let record = state
                    .tir_class_v1(&frame.original_v1().job.class_id)
                    .ok_or_else(|| refused("family table inventory registered class missing"))?;
                let hard = frame
                    .plan_v1()
                    .terms
                    .accepted_daa
                    .checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
                    .ok_or_else(|| refused("family table restored hard overflow"))?;
                let until =
                    crate::palw_court_service_state_v2::tir_service_required_material_until_v1(frame, hard).map_err(refused)?;
                if row.material_until_daa != until {
                    return Err(refused("family table inventory changes original retention"));
                }
                m::prepare_family_table_inventory_v1(&row.bytes, sem, record.logits_vocab).map_err(native)?;
                if !state.family_supplier_rows_v1()?.iter().any(|(_, supplier)| {
                    supplier.consent.terms.claim == row.claim
                        && supplier.consent.terms.role == crate::palw_court_family_supplier_v1::FamilySupplierRoleV1::SemanticTable
                }) {
                    return Err(refused("family table inventory lost its admitted semantic supplier contract"));
                }
            }
            (65, Some(3)) => {
                let body = read::<m::FamilyMaterialBodyV1>(state, key)?.ok_or_else(|| refused("family body disappeared"))?;
                let row = supplier::family_contract_for_material_v1(state, body.contract)?
                    .ok_or_else(|| refused("family body lost signed contract"))?;
                let terms = &row.consent.terms;
                let inventory = m::derive_contract_family_inventory_classified_v3(state, &row).map_err(native)?;
                let target = inventory.target_id_v1(body.target).map_err(refused)?;
                if *key != m::material_aux_key_v1(3, target, None)
                    || body.material_until_daa != terms.material_until_daa
                    || row.completed.get(&target) != Some(&body.digest)
                {
                    return Err(refused("family restored body not the authenticated completed target"));
                }
                check_body(state, &inventory, body.target, &body, now)?;
            }
            (65, Some(5)) => {}
            (65, Some(6)) => {
                draft_reservations::validate_primary(state, key)?;
            }
            _ => return Err(refused("unallocated family material row namespace")),
        }
    }
    draft_reservations::validate(state)?;
    Ok(())
}
