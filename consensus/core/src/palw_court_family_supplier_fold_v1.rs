//! RFC-0042 independent supplier capital and finite fee tranche. Every money
//! mutation uses the existing journal, once ledger, real bond debit and R book.
use super::*;
use crate::palw_court_family_material_v1::{FAMILY_MATERIAL_PART_BYTES_V1, FamilyMaterialTargetV1, NativeFamilyInventoryV1};
use crate::palw_court_family_supplier_v1::*;
use crate::palw_court_service_v1::{ServiceAssessment, ServiceCost, ServicePacket};
use crate::palw_settlement_once_v1::*;
use crate::palw_staged_court_state_v1::PalwStagedCourtRowV1 as Row;
fn refused(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.into())
}
fn digest<T: borsh::BorshSerialize>(domain: &[u8], x: &T) -> Result<Hash64, PalwStateV2Error> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(domain, x).map_err(refused)
}
fn add(a: u64, b: u64) -> Result<u64, PalwStateV2Error> {
    a.checked_add(b).ok_or_else(|| refused("family supplier checked sum overflow"))
}
fn mul(a: u64, b: u64) -> Result<u64, PalwStateV2Error> {
    a.checked_mul(b).ok_or_else(|| refused("family supplier checked product overflow"))
}
pub(super) fn write_family_supplier_v1(
    b: &mut TransitionBuilder<'_>,
    id: Hash64,
    row: &FamilySupplierRowV1,
) -> Result<(), PalwStateV2Error> {
    if row.consent.id_v1().map_err(refused)? != id {
        return Err(refused("family immutable contract identity"));
    }
    let key = family_status_key_v2(id);
    let exists = b
        .state
        .kernel_route()
        .is_some_and(|r| r.aux.keys().any(|(t, k)| *t == FAMILY_SUPPLIER_TABLE_V1 && k.as_slice() == id.as_byte_slice()));
    if !exists {
        reserve_copy(b, row)?;
        let mut immutable = row.clone();
        immutable.version = 2; // physical normalized contract header, logical API remains V1
        immutable.completed.clear();
        immutable.defaulted_daa = None;
        immutable.fee_state = FamilySupplierFeeStateV1::Held;
        b.write_kernel_row_encoded_v1(FAMILY_SUPPLIER_TABLE_V1, id.as_byte_slice(), Some(&immutable))?;
    } else {
        let old = family_contract_for_material_v1(&b.state, id)?.ok_or_else(|| refused("family immutable contract missing"))?;
        if old.consent != row.consent
            || old.allocation != row.allocation
            || old.accepted_daa != row.accepted_daa
            || old.targets != row.targets
            || old.completed != row.completed
        {
            return Err(refused("family immutable catalogue/completion mutation"));
        }
    }
    let status =
        FamilySupplierStatusV2 { version: 2, contract: id, defaulted_daa: row.defaulted_daa, fee_state: row.fee_state.clone() };
    b.write_kernel_row_encoded_v1(FAMILY_SUPPLIER_TABLE_V1, &key, Some(&status))
}
pub(super) fn family_contract_for_material_v1(
    state: &PalwChainStateV2,
    id: Hash64,
) -> Result<Option<crate::palw_native_resource_v1::NativeRetainedV1<FamilySupplierRowV1>>, PalwStateV2Error> {
    let rows = state.family_supplier_rows_v1()?;
    let (rows, lease) = rows.into_parts_v1();
    Ok(rows
        .into_iter()
        .find(|(key, _)| *key == id)
        .map(|(_, row)| crate::palw_native_resource_v1::NativeRetainedV1::from_owner_reserved_v1(row, lease)))
}
fn delete_family_supplier_v2(b: &mut TransitionBuilder<'_>, id: Hash64, row: &FamilySupplierRowV1) -> Result<(), PalwStateV2Error> {
    for target in row.completed.keys() {
        b.write_kernel_row_encoded_v1::<FamilySupplierCompletionV2>(
            FAMILY_SUPPLIER_TABLE_V1,
            &family_completion_key_v2(id, *target),
            None,
        )?;
    }
    b.write_kernel_row_encoded_v1::<FamilySupplierStatusV2>(FAMILY_SUPPLIER_TABLE_V1, &family_status_key_v2(id), None)?;
    b.write_kernel_row_encoded_v1::<FamilySupplierRowV1>(FAMILY_SUPPLIER_TABLE_V1, id.as_byte_slice(), None)
}
fn reserve_copy(b: &mut TransitionBuilder<'_>, row: &FamilySupplierRowV1) -> Result<(), PalwStateV2Error> {
    let mut q = super::native_writers::NativeWriteQuoteV1::new("family supplier mutation clone");
    q.copy(row)?;
    let packets = row
        .targets
        .iter()
        .try_fold(0u64, |n, t| n.checked_add(t.packets.len() as u64))
        .ok_or_else(|| refused("family mutation packet count overflow"))?;
    q.exact::<u64>(packets)?;
    q.member::<Hash64, Hash64>(1)?;
    b.admit_native_write_v1(q)
}

pub(super) struct FamilySupplierPlanV1 {
    pub row: FamilySupplierRowV1,
    pub packets: Vec<ServicePacket>,
    _lease: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
}

/// Profile IDs include the exact unchanged prices and this derivation version.
pub fn family_supplier_profiles_v1() -> Result<[Hash64; 4], PalwStateV2Error> {
    use crate::palw_g14_values_v1 as g;
    let values = (
        (3u16, 2u16), // native clock derivation, normalized finance storage
        crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_CLOCK_FAMILY_VERSION_V3,
        g::PALW_G14_CARRIAGE_SOMPI_PER_BYTE_V1,
        g::PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1.sompi_per_byte,
        g::PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1,
        g::PALW_G14_SOURCE_PACKET_COST_V1,
        g::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1,
        FAMILY_MATERIAL_PART_BYTES_V1 as u64,
        (g::PALW_G14_HEAP_BYTES_PER_BRANCH_V1, g::PALW_G14_RETENTION_BYTES_PER_BRANCH_V1),
        (FAMILY_SUPPLIER_SIGNATURE_CONTEXT_V1, crate::palw_court_family_material_v1::FAMILY_MATERIAL_SIGNATURE_CONTEXT_V1),
    );
    Ok([
        digest(b"misaka-palw/family-supplier/fee-profile/v1", &values)?,
        digest(b"misaka-palw/family-supplier/capital-profile/v1", &values)?,
        digest(b"misaka-palw/family-supplier/retention-profile/v1", &values)?,
        digest(b"misaka-palw/family-supplier/settlement-profile/v1", &(values, true, true))?,
    ])
}

/// Exact upper widths of the existing material controls including lifecycle tag,
/// fixed outer fields, target encoding, action encoding and ML-DSA signature.
fn control_bytes(target: FamilyMaterialTargetV1, kind: u8, payload: u64) -> Result<u64, PalwStateV2Error> {
    let target_bytes = borsh::object_length(&target).map_err(|e| refused(e.to_string()))? as u64;
    let fixed = 2 + 1 + 2 + 64 + target_bytes + 68 + 4 + crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64;
    add(
        fixed,
        match kind {
            0 => 1 + 8 + 64,
            1 => add(1 + 64 + 8 + 4, payload)?,
            2 => 1 + 64,
            _ => return Err(refused("family material control kind")),
        },
    )
}
/// Simultaneous complete body plus unfinished parts/draft, including literal
/// aux row/key framing. Parser and writer peak allocations are also admitted
/// by their native owners; these are the persistent all-branch holding nodes.
fn retained_family_bytes_v1(inventory: &NativeFamilyInventoryV1) -> Result<u64, PalwStateV2Error> {
    let mut total = 0;
    for (target, n) in inventory.targets_v1() {
        let t = borsh::object_length(target).map_err(|e| refused(e.to_string()))? as u64;
        let parts = n.div_ceil(FAMILY_MATERIAL_PART_BYTES_V1 as u64);
        let body = 64 + t + 64 + 4 + 64 + 68 + 8 + 8 + 74;
        let draft = 64 + t + 68 + 8 + 64 + 8 + 8 + 8 + 74;
        let part = 64 + 8 + 4 + 8 + 82;
        total = add(total, add(mul(*n, 2)?, add(add(body, draft)?, mul(parts, part)?)?)?)?;
    }
    // The admission witness is a distinct rooted canonical table copy. It
    // coexists with the later supplier body and is retained under this role.
    if inventory.role_v1() == FamilySupplierRoleV1::SemanticTable {
        // Court fields, booleans and prompt form have fixed Borsh widths;
        // encode a shape exemplar instead of guessing the new metadata bytes.
        let shape = Some(crate::palw_court_family_material_v1::FamilyOriginalRuleContextV2 {
            court: crate::palw_mode_v2::PalwCourtParamsV2::new(1u64 << 40, 45, 2).map_err(|e| refused(e.to_string()))?,
            dissection: false,
            prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        });
        let rule_bytes = borsh::object_length(&shape).map_err(|e| refused(e.to_string()))? as u64;
        total = add(total, add(inventory.max_material_bytes_v1(), add(64 + 68 + 64 + 4 + 8 + 74, rule_bytes)?)?)?;
    }
    // Finance is not free metadata. Every simultaneously retained literal,
    // its aux-tree node, the maximum logical query reconstruction and the
    // complete create/update/delete undo journal are included. Normalized
    // completions make this linear in native target/packet cardinality.
    let finance = family_finance_geometry_v2(inventory.targets_v1().len() as u64, inventory.packet_count_v1().map_err(refused)?)?;
    add(total, add(finance.retained, add(finance.journal, finance.reconstruction)?)?)
}
#[derive(Clone, Copy, Debug)]
pub(super) struct FamilyFinanceGeometryV2 {
    pub retained: u64,
    pub journal: u64,
    pub reconstruction: u64,
}
pub(super) fn family_finance_geometry_v2(targets: u64, packets: u64) -> Result<FamilyFinanceGeometryV2, PalwStateV2Error> {
    // All terms fields are fixed-width. Both signatures have the existing
    // exact ML-DSA length enforced by decoding and native consent validation.
    let terms = 2 + 11 * 64 + 1 + 2 * 68 + 5 * 8 + 2 * 8;
    let consent = add(terms, mul(2, 4 + crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)?)?;
    let catalogue = add(mul(targets, 64 + 8 + 4)?, mul(packets, 64)?)?;
    let header = add(add(2 + 64 + 8 + 4 + 4 + 1 + 1 + 73, consent)?, catalogue)?;
    let status = 2 + 64 + 9 + (1 + 68 + 64 + 64) + 74;
    let completion = 2 + 3 * 64 + 138;
    let nodes = add(targets, 2)?;
    let mapnode = std::mem::size_of::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<(u8, Vec<u8>), Vec<u8>>>() as u64;
    let retained = add(add(header, status)?, add(mul(targets, completion)?, mul(nodes, mapnode)?)?)?;
    // Header create/delete; each completion create/delete; status at most
    // initial + one objective default + once queued + once paid + deletion.
    // The create/delete delta option framing exceeds one aux literal by
    // three bytes. Include native journal membership as well as encoded bytes;
    // replay entries retain old/new byte owners, not just a checksum.
    let journal_bytes = add(add(mul(2, add(header, 3)?)?, mul(8, add(status, 3)?)?)?, mul(targets, mul(2, add(completion, 3)?)?)?)?;
    let journal_entries = add(7, mul(2, targets)?)?;
    let journal = add(journal_bytes, mul(journal_entries, std::mem::size_of::<PalwDeltaEntryV2>() as u64)?)?;
    let reconstruction = add(
        (std::mem::size_of::<(Hash64, FamilySupplierRowV1)>() + std::mem::size_of::<Vec<(Hash64, FamilySupplierRowV1)>>()) as u64,
        add(
            mul(2, crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)?,
            add(
                mul(targets, std::mem::size_of::<FamilySupplierTargetV1>() as u64)?,
                add(
                    mul(packets, 64)?,
                    mul(targets, std::mem::size_of::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, Hash64>>() as u64)?,
                )?,
            )?,
        )?,
    )?;
    Ok(FamilyFinanceGeometryV2 { retained, journal, reconstruction })
}
fn retention_service_bytes_v1(inventory: &NativeFamilyInventoryV1) -> Result<u64, PalwStateV2Error> {
    use crate::palw_g14_values_v1 as g;
    let retained = retained_family_bytes_v1(inventory)?;
    let quanta = retained.div_ceil(g::PALW_G14_RETENTION_BYTES_PER_BRANCH_V1);
    add(retained, mul(quanta, g::PALW_G14_HEAP_BYTES_PER_BRANCH_V1)?)
}
fn retention_nodes_v1(inventory: &NativeFamilyInventoryV1) -> Result<u64, PalwStateV2Error> {
    let per_packet = crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes;
    if per_packet == 0 {
        return Err(refused("family retention service byte budget zero"));
    }
    Ok(retention_service_bytes_v1(inventory)?.div_ceil(per_packet))
}
/// Read-only SDK quote from an opaque native inventory. Clocks and actor IDs
/// remain explicit offered terms; real admission re-derives them at birth.
pub fn palw_family_supplier_terms_quote_v1(
    inventory: &NativeFamilyInventoryV1,
    mut expected: FamilySupplierTermsV1,
) -> Result<FamilySupplierTermsV1, PalwStateV2Error> {
    use crate::palw_g14_values_v1 as g;
    expected.version = 1;
    expected.claim = inventory.claim_v1();
    expected.source = inventory.source_v1();
    expected.relative_graph = inventory.relative_graph_v1();
    expected.role = inventory.role_v1();
    expected.inventory = inventory.id_v1();
    expected.max_material_bytes = inventory.max_material_bytes_v1();
    let [fee, capital, retention, settlement] = family_supplier_profiles_v1()?;
    expected.fee_profile = fee;
    expected.capital_profile = capital;
    expected.retention_profile = retention;
    expected.settlement_policy = settlement;
    let mut carriage = 0u64;
    let mut packet_count = 0u64;
    for (target, n) in inventory.targets_v1() {
        let parts = n.div_ceil(FAMILY_MATERIAL_PART_BYTES_V1 as u64);
        packet_count = add(packet_count, add(parts, 2)?)?;
        carriage = add(carriage, add(control_bytes(*target, 0, 0)?, control_bytes(*target, 2, 0)?)?)?;
        carriage = add(carriage, add(mul(parts, control_bytes(*target, 1, 0)?)?, *n)?)?;
    }
    expected.fee_sompi = add(
        mul(carriage, g::PALW_G14_CARRIAGE_SOMPI_PER_BYTE_V1)?,
        mul(inventory.max_material_bytes_v1(), g::PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1.sompi_per_byte)?,
    )?;
    expected.collateral_sompi = mul(add(packet_count, retention_nodes_v1(inventory)?)?, g::PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1)?;
    expected.validate_shape_v1().map_err(refused)?;
    Ok(expected)
}
pub(super) fn derive_family_supplier_plan_v1(
    state: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    inventory: &NativeFamilyInventoryV1,
    allocation: Hash64,
    material_until: u64,
    consent: &SignedFamilySupplierV1,
) -> Result<FamilySupplierPlanV1, PalwStateV2Error> {
    derive_family_supplier_plan_inner_v1(state, ctx, inventory, allocation, material_until, consent, true)
}
fn derive_family_supplier_plan_inner_v1(
    state: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    inventory: &NativeFamilyInventoryV1,
    allocation: Hash64,
    material_until: u64,
    consent: &SignedFamilySupplierV1,
    admission: bool,
) -> Result<FamilySupplierPlanV1, PalwStateV2Error> {
    use crate::palw_g14_values_v1 as g;
    let hard =
        ctx.daa_score.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA).ok_or_else(|| refused("family hard overflow"))?;
    let mut expected = consent.terms.clone();
    expected.version = 1;
    expected.claim = inventory.claim_v1();
    expected.source = inventory.source_v1();
    expected.relative_graph = inventory.relative_graph_v1();
    expected.role = inventory.role_v1();
    expected.inventory = inventory.id_v1();
    expected.max_material_bytes = inventory.max_material_bytes_v1();
    expected.respond_until_daa = hard;
    expected.material_until_daa = material_until;
    let route = state.kernel_route().ok_or_else(|| refused("family supplier native route absent"))?;
    expected.network = Hash64::from_bytes(route.header.policy.network_domain);
    expected.ruleset = Hash64::from_bytes(route.header.policy.ruleset_digest);
    expected = palw_family_supplier_terms_quote_v1(inventory, expected)?;
    let packet_count = add(inventory.packet_count_v1().map_err(refused)?, retention_nodes_v1(inventory)?)?;
    let supplier = state.bond(&expected.supplier).ok_or(PalwStateV2Error::MissingBond(expected.supplier))?;
    let sponsor = state.bond(&expected.sponsor).ok_or(PalwStateV2Error::MissingBond(expected.sponsor))?;
    if admission
        && (!matches!(supplier.status, PalwBondStatusV2::Active)
            || !matches!(sponsor.status, PalwBondStatusV2::Active)
            || supplier.operator_id != palw_operator_id_v2(&supplier.pubkey)
            || sponsor.operator_id != palw_operator_id_v2(&sponsor.pubkey))
    {
        return Err(refused("family supplier/sponsor current authority"));
    }
    consent.verify_v1(&expected, ctx.daa_score, &supplier.pubkey, &sponsor.pubkey).map_err(refused)?;
    let id = consent.id_v1().map_err(refused)?;
    let first = add(ctx.daa_score, 1)?;
    let mut packets = Vec::new();
    let mut targets = Vec::new();
    // Counts and every nested packet/target allocation are admitted before fill.
    let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.exact_v1::<FamilySupplierPlanV1>(1).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<ServicePacket>(packet_count).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<FamilySupplierTargetV1>(inventory.targets_v1().len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<[u8; 64]>(packet_count).map_err(PalwStateV2Error::NativeResource)?;
    q.add_v1(crate::palw_native_resource_v1::native_value_requests_v1(consent).map_err(PalwStateV2Error::NativeResource)?)
        .map_err(PalwStateV2Error::NativeResource)?;
    let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    packets
        .try_reserve_exact(usize::try_from(packet_count).map_err(|_| refused("family packet host count"))?)
        .map_err(|_| refused("family packet allocation"))?;
    targets.try_reserve_exact(inventory.targets_v1().len()).map_err(|_| refused("family target allocation"))?;
    for (target, n) in inventory.targets_v1() {
        let target_id = inventory.target_id_v1(*target).map_err(refused)?;
        let parts = n.div_ceil(FAMILY_MATERIAL_PART_BYTES_V1 as u64);
        let mut ids = Vec::with_capacity(usize::try_from(add(parts, 2)?).map_err(|_| refused("family packet host count"))?);
        for ordinal in 0..add(parts, 2)? {
            let packet = digest(b"misaka-palw/family-supplier/service-packet/v1", &(id, allocation, target_id, ordinal))?;
            let (kind, bytes) = if ordinal == 0 {
                (0, 0)
            } else if ordinal == parts + 1 {
                (2, 0)
            } else {
                (1, (*n - (ordinal - 1) * FAMILY_MATERIAL_PART_BYTES_V1 as u64).min(FAMILY_MATERIAL_PART_BYTES_V1 as u64))
            };
            let cost = ServiceCost {
                bytes: control_bytes(*target, kind, bytes)?.max(g::PALW_G14_SOURCE_PACKET_COST_V1.bytes),
                ..g::PALW_G14_SOURCE_PACKET_COST_V1
            };
            packets.push(ServicePacket { id: packet.as_bytes(), release_daa: first, service_before: hard, cost });
            ids.push(packet.as_bytes());
        }
        targets.push(FamilySupplierTargetV1 { id: target_id, max_bytes: *n, packets: ids });
    }
    let retained = retention_service_bytes_v1(inventory)?;
    for ordinal in 0..retention_nodes_v1(inventory)? {
        let packet = digest(b"misaka-palw/family-supplier/retention-packet/v1", &(id, allocation, ordinal))?;
        let quantum = g::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes;
        let bytes = (retained - ordinal * quantum).min(quantum);
        packets.push(ServicePacket {
            id: packet.as_bytes(),
            release_daa: first,
            service_before: hard,
            cost: ServiceCost { bytes, work: 1, io: 0, controls: 1 },
        });
    }
    targets.sort_unstable_by_key(|t| t.id);
    Ok(FamilySupplierPlanV1 {
        row: FamilySupplierRowV1 {
            version: 1,
            consent: consent.clone(),
            allocation,
            accepted_daa: ctx.daa_score,
            targets,
            completed: BTreeMap::new(),
            defaulted_daa: None,
            fee_state: FamilySupplierFeeStateV1::Held,
        },
        packets,
        _lease: lease,
    })
}

/// Extend the same original's all-branch candidate book; no second state commit.
pub(super) fn reserve_family_plans_v1(
    book: &mut crate::palw_court_service_state_v2::CourtServiceStateV2,
    claim: Hash64,
    plans: &[FamilySupplierPlanV1],
) -> Result<(), PalwStateV2Error> {
    for plan in plans {
        let hard = plan.row.consent.terms.respond_until_daa;
        book.book.reserve(&plan.packets, hard).map_err(|e| refused(format!("family all-branch reservation: {e:?}")))?;
        let binding = book.claims.get_mut(&claim).ok_or_else(|| refused("family original binding absent"))?;
        binding.packet_ids.extend(plan.packets.iter().map(|p| p.id));
        for target in &plan.row.targets {
            let latest = target
                .packets
                .iter()
                .filter_map(|id| book.book.assigned_daa(id))
                .max()
                .ok_or_else(|| refused("family packet assignment absent"))?;
            if add(latest, crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.service_span_daa)? > hard {
                return Err(refused("family response window cannot fit original hard horizon"));
            }
        }
    }
    book.check_live_v1().map_err(refused)
}

/// Full sponsor totals and supplier totals are checked together, including one
/// bond acting in several roles; a fee debit cannot eat another role's capital.
pub(super) fn check_family_funding_v1(
    b: &TransitionBuilder<'_>,
    now: u64,
    plans: &[FamilySupplierPlanV1],
) -> Result<(), PalwStateV2Error> {
    let mut needed = BTreeMap::<PalwBondKeyV2, u128>::new();
    for p in plans {
        let t = &p.row.consent.terms;
        if family_contract_for_material_v1(&b.state, p.row.consent.id_v1().map_err(refused)?)?.is_some() {
            return Err(refused("family supplier consent already installed"));
        }
        for (who, amount) in [(t.supplier, t.collateral_sompi), (t.sponsor, t.fee_sompi)] {
            let sum = needed.entry(who).or_default();
            *sum = sum.checked_add(u128::from(amount)).ok_or_else(|| refused("family capital overflow"))?;
        }
    }
    for (who, needed) in needed {
        let bond = b.state.bond(&who).ok_or(PalwStateV2Error::MissingBond(who))?;
        if u128::from(bond.collateral).saturating_sub(b.committed_at(&who, now)?) < needed {
            return Err(refused("family sponsor/supplier has insufficient uncommitted real principal"));
        }
    }
    Ok(())
}
pub(super) fn install_family_plans_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    plans: Vec<FamilySupplierPlanV1>,
) -> Result<(), PalwStateV2Error> {
    check_family_funding_v1(b, ctx.daa_score, &plans)?;
    for plan in plans {
        let row = plan.row;
        let t = &row.consent.terms;
        let id = row.consent.id_v1().map_err(refused)?;
        crate::palw_settlement_liability_v1::palw_settlement_liability_sourced_v1(&b.signed_fee_tranche_v1(
            t.fee_sompi,
            t.fee_sompi,
            t.respond_until_daa,
            t.material_until_daa,
            ctx.daa_score,
        ))
        .map_err(super::palw_settlement_liability_fold_v1::liability_refused_v1)?;
        let funding = digest(b"misaka-palw/family-supplier/funding/v1", &id)?;
        let once = b.settle_once_liable_v1(
            ctx.daa_score,
            PalwSettlementObligationV1::of(PalwSettlementLegV1::MaterialFunding, funding.as_bytes()),
            PalwSettlementConsumerV1::MonetaryMaterial,
            t.fee_sompi,
            PalwSettlementDebitV1::Prepaid {
                sponsor: crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&t.sponsor),
                amount: t.fee_sompi,
            },
            super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::NoLiableClaim(
                "family supplier sponsor principal funding debit",
            ),
        )?;
        if !once.pays() {
            return Err(refused("family supplier funding already consumed"));
        }
        b.reserve_source_income_v1(ctx, id, t.claim, t.supplier, t.respond_until_daa, t.fee_sompi)?;
        let income = super::palw_bond_budget_fold_v1::source_income_id_v1(id, t.claim, &t.supplier);
        b.bond_budget_op(super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1::point(), |book, _, journal| {
            book.bind_material_income_v1(t.claim, income, journal)
        })?
        .ok_or_else(|| refused("family supplier funding material engine absent"))?
        .map_err(|e| refused(e.to_string()))?;
        if b.slash_bond(t.sponsor, u128::from(t.fee_sompi))? != t.fee_sompi {
            return Err(refused("family sponsor full fee debit"));
        }
        write_family_supplier_v1(b, id, &row)?;
    }
    Ok(())
}

pub(super) fn complete_family_target_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    contract: Hash64,
    target: Hash64,
    body: Hash64,
) -> Result<(), PalwStateV2Error> {
    let owned = family_contract_for_material_v1(&b.state, contract)?.ok_or_else(|| refused("family supplier contract absent"))?;
    if ctx.daa_score < owned.accepted_daa
        || ctx.daa_score > owned.consent.terms.material_until_daa
        || body == Hash64::default()
        || owned.targets.binary_search_by_key(&target, |t| t.id).is_err()
    {
        return Err(refused("family authenticated completion scope/window"));
    }
    if let Some(old) = owned.completed.get(&target) {
        return if *old == body { Ok(()) } else { Err(refused("family completed body substitution")) };
    }
    let completed = FamilySupplierCompletionV2 { version: 2, contract, target, digest: body };
    b.write_kernel_row_encoded_v1(FAMILY_SUPPLIER_TABLE_V1, &family_completion_key_v2(contract, target), Some(&completed))
}

pub(super) fn tick_family_suppliers_v1(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if !b.params.family_supplier_active_at(ctx.daa_score) {
        return Ok(());
    }
    let rows = b.state.family_supplier_rows_v1()?;
    for (id, original) in rows.iter() {
        reserve_copy(b, original)?;
        let mut row = original.clone();
        let t = row.consent.terms.clone();
        // Supplied reserved units are opportunities only under the adopted
        // inclusion premise, inside the original's native bounded duty clock.
        if row.defaulted_daa.is_none() && ctx.daa_score <= t.material_until_daa {
            if let (Some(service), Some(premise)) = (b.state.court_service_state_v1(), b.extras.court_service_inclusion_premise_v1) {
                let span = crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.service_span_daa;
                let mut default = false;
                for target in &row.targets {
                    if row.completed.contains_key(&target.id) {
                        continue;
                    }
                    let units = target.packets.iter().map(|id| service.book.assigned_daa(id)).collect::<Option<Vec<_>>>();
                    let Some(units) = units else { return Err(refused("family service packet lost before material horizon")) };
                    let due = add(*units.iter().max().ok_or_else(|| refused("family empty packet set"))?, span)?;
                    if premise.version == 1
                        && due <= t.respond_until_daa
                        && ctx.daa_score > due
                        && target.packets.iter().all(|id| service.book.assess(id) == ServiceAssessment::OpportunityProvided)
                        && units.iter().all(|u| u.checked_add(premise.inclusion_delay_daa).is_some_and(|included| included <= due))
                    {
                        default = true;
                        break;
                    }
                }
                if default {
                    let charge = digest(b"misaka-palw/family-supplier/service-default/v1", id)?;
                    let once = b.settle_once_liable_v1(
                        ctx.daa_score,
                        PalwSettlementObligationV1::of(PalwSettlementLegV1::MaterialProviderDefault, charge.as_bytes()),
                        PalwSettlementConsumerV1::MonetaryMaterial,
                        t.collateral_sompi,
                        PalwSettlementDebitV1::Collateral {
                            bond: crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&t.supplier),
                            amount: t.collateral_sompi,
                        },
                        super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::NoLiableClaim(
                            "family supplier objective service-bond debit, no computation credit",
                        ),
                    )?;
                    if !once.pays() || b.slash_bond(t.supplier, u128::from(t.collateral_sompi))? != t.collateral_sompi {
                        return Err(refused("family full service collateral charge"));
                    }
                    row.defaulted_daa = Some(ctx.daa_score);
                }
            }
        }
        if matches!(row.fee_state, FamilySupplierFeeStateV1::Held) {
            let refund = row.defaulted_daa.is_some_and(|d| d <= t.respond_until_daa);
            if refund || ctx.daa_score > t.respond_until_daa {
                queue_fee(b, ctx, *id, &mut row, refund)?;
            }
        }
        if row != *original {
            write_family_supplier_v1(b, *id, &row)?;
        }
        if ctx.daa_score > t.material_until_daa
            && matches!(row.fee_state, FamilySupplierFeeStateV1::Paid { .. })
            && crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state, t.claim).is_none()
        {
            // Only birth/fee metadata remains while bounded staged GC or an
            // original source lease still retains its version3 frame. Actual
            // fee and collateral horizons remain the signed finite horizons.
            delete_family_supplier_v2(b, *id, &row)?;
        }
    }
    Ok(())
}

/// Only the actual consumed coinbase prefix can acknowledge an exact fee. A
/// status query or an empty cash queue cannot invent a physical payment.
pub(super) fn acknowledge_family_fee_payments_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let rows = b.state.family_supplier_rows_v1()?;
    for (id, original) in rows.iter() {
        let FamilySupplierFeeStateV1::Queued { payee, queue, receipt } = original.fee_state else { continue };
        let paid=b.entries.iter().any(|e|matches!(e,PalwDeltaEntryV2::Payout{key,old:Some(p),new:None} if *key==queue&&p.amount>=original.consent.terms.fee_sompi));
        if !paid {
            continue;
        }
        let cash = b
            .state
            .bond_budget
            .as_ref()
            .and_then(|book| book.material_income.get(&receipt))
            .ok_or_else(|| refused("family exact physical receipt missing"))?;
        if cash.root != Some(original.consent.terms.claim)
            || cash.amount != original.consent.terms.fee_sompi
            || cash.accepted_daa != original.accepted_daa
        {
            return Err(refused("family physical receipt substitution"));
        }
        reserve_copy(b, original)?;
        let mut row = original.clone();
        row.fee_state = FamilySupplierFeeStateV1::Paid { payee, daa: ctx.daa_score };
        write_family_supplier_v1(b, *id, &row)?;
    }
    Ok(())
}

pub(crate) fn validate_family_suppliers_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
    let rows = state.family_supplier_rows_v1()?;
    let now = state.last_point().map_or(0, |p| p.daa_score);
    // The new clock is never an interpretation of a historical TFO1 frame or
    // a standalone owner row. It requires its own admitted, signed cohort.
    for (_, stored) in &state.staged_courts {
        match stored {
            Row::TirClaim(frame)
                if frame.plan_v1().acquisition.clock.version
                    == crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_CLOCK_FAMILY_VERSION_V3 =>
            {
                let p = frame.plan_v1();
                if !params.family_supplier_active_at(p.terms.accepted_daa)
                    || !rows.iter().any(|(_, r)| r.consent.terms.claim == p.original_claim)
                {
                    return Err(refused("family clock lacks owning fork and signed supplier cohort"));
                }
            }
            Row::TirAcquisition(row)
                if row.owned.contract_v1().plan.clock.version
                    == crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_CLOCK_FAMILY_VERSION_V3 =>
            {
                let claim = row.owned.contract_v1().claim;
                let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
                    .ok_or_else(|| refused("family owner clock lost its original frame"))?;
                if frame.plan_v1().acquisition != row.owned.contract_v1().plan
                    || row.owned.work_v1().is_none_or(|work| work.allocation != frame.plan_v1().owner_work)
                    || !params.family_supplier_active_at(frame.plan_v1().terms.accepted_daa)
                {
                    return Err(refused("family owner clock differs from original native allocation"));
                }
            }
            _ => {}
        }
    }

    let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, ()>>(rows.len() as u64)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _claims_lease =
        crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut live_claims = std::collections::BTreeSet::new();
    for (id, row) in rows.iter() {
        let t = &row.consent.terms;
        if !params.family_supplier_active_at(row.accepted_daa)
            || !params.settlement_once_active_at(row.accepted_daa)
            || row.targets.iter().try_fold(0u64, |sum, target| sum.checked_add(target.max_bytes)) != Some(t.max_material_bytes)
            || row.defaulted_daa.is_some_and(|d| d > now)
        {
            return Err(refused("family supplier restored cohort/capacity/default clock"));
        }
        let supplier = state.bond(&t.supplier).ok_or(PalwStateV2Error::MissingBond(t.supplier))?;
        let sponsor = state.bond(&t.sponsor).ok_or(PalwStateV2Error::MissingBond(t.sponsor))?;
        row.consent.verify_v1(t, row.accepted_daa, &supplier.pubkey, &sponsor.pubkey).map_err(refused)?;
        if now <= t.material_until_daa {
            live_claims.insert(t.claim);
            let inv = crate::palw_court_family_material_v1::derive_current_family_inventory_classified_v1(state, t.claim, t.role)
                .map_err(|e| match e {
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c) => PalwStateV2Error::NativeResource(c),
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e),
                })?;
            if inv.id_v1() != t.inventory
                || inv.max_material_bytes_v1() != t.max_material_bytes
                || inv.source_v1() != t.source
                || inv.relative_graph_v1() != t.relative_graph
            {
                return Err(refused("family restored native inventory differs from signed contract"));
            }
            // Completion history cannot replace retained material while its
            // signed duty is live. Check the reverse direction as well as the
            // material validator's body->finance link; an orphan digest must
            // never suppress an objective service default after restore.
            for (target, _) in inv.targets_v1() {
                let target_id = inv.target_id_v1(*target).map_err(refused)?;
                let Some(expected_digest) = row.completed.get(&target_id) else { continue };
                let actual_digest = if *target == FamilyMaterialTargetV1::SemanticTable {
                    let literal = crate::palw_court_family_material_v1::read_authenticated_family_table_inventory_v1(state, t.claim)
                        .map_err(|e| match e {
                            crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c) => PalwStateV2Error::NativeResource(c),
                            crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e),
                        })?
                        .ok_or_else(|| refused("family completed table lost actual original literal"))?;
                    crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&literal.bytes)
                } else {
                    crate::palw_court_family_material_v1::read_authenticated_family_material_classified_v1(state, &inv, *target)
                        .map_err(|e| match e {
                            crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c) => PalwStateV2Error::NativeResource(c),
                            crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e),
                        })?
                        .ok_or_else(|| refused("family completed target lost actual retained body"))?
                        .digest
                };
                if actual_digest != *expected_digest {
                    return Err(refused("family completion changes retained literal digest"));
                }
            }
            let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, t.claim)
                .ok_or_else(|| refused("family original was pruned inside signed horizon"))?;
            if frame.plan_v1().allocation_id != row.allocation || frame.plan_v1().terms.accepted_daa != row.accepted_daa {
                return Err(refused("family restored original birth/allocation differs"));
            }
            let ctx = PalwBlockContextV2 { block: Hash64::default(), daa_score: row.accepted_daa, blue_score: 0, subsidy: 0 };
            let expected =
                derive_family_supplier_plan_inner_v1(state, &ctx, &inv, row.allocation, t.material_until_daa, &row.consent, false)?;
            if expected.row.targets != row.targets {
                return Err(refused("family restored packet inventory differs"));
            }
            let service = state.court_service_state_v1().ok_or_else(|| refused("family restored service book absent"))?;
            let binding = service.claims.get(&t.claim).ok_or_else(|| refused("family restored original binding absent"))?;
            if expected.packets.iter().any(|p| !binding.packet_ids.contains(&p.id) || service.book.packet(&p.id) != Some(p)) {
                return Err(refused("family reservation is not in original atomic binding"));
            }
        }
        let context = digest(b"misaka-palw/family-supplier/held-fee/v1", id)?;
        let obligation = PalwSettlementObligationV1::of(PalwSettlementLegV1::MaterialProviderFee, context.as_bytes());
        let key = obligation.id_v1();
        let settled = state
            .kernel_route()
            .and_then(|r| {
                r.aux.iter().find(|((table, k), _)| *table == PALW_SETTLEMENT_ONCE_TABLE_V1 && k.as_slice() == key).map(|(_, v)| v)
            })
            .map(|bytes| {
                let row: PalwSettlementOnceRowV1 = borsh::from_slice(bytes).map_err(|_| refused("family fee settlement malformed"))?;
                row.validate_v1(&key).map_err(refused)?;
                if row.amount != t.fee_sompi {
                    return Err(refused("family settled amount substitution"));
                }
                Ok(row)
            })
            .transpose()?;
        match row.fee_state {
            FamilySupplierFeeStateV1::Held if settled.is_some() => return Err(refused("held family fee already consumed")),
            FamilySupplierFeeStateV1::Held => {}
            FamilySupplierFeeStateV1::Queued { payee, queue, receipt } => {
                let refund = row.defaulted_daa.is_some_and(|d| d <= t.respond_until_daa);
                if payee != if refund { t.sponsor } else { t.supplier } || settled.is_none() {
                    return Err(refused("family queued fee payee/once mismatch"));
                }
                let cash = state
                    .bond_budget
                    .as_ref()
                    .and_then(|b| b.material_income.get(&receipt))
                    .ok_or_else(|| refused("family queued material receipt absent"))?;
                if cash.root != Some(t.claim)
                    || cash.amount != t.fee_sompi
                    || !match cash.payment {
                        crate::palw_bond_budget_v1::PalwMaterialPaymentV1::Queued {
                            income: None,
                            queue: Some(q),
                            paid_daa: None,
                            ..
                        } => q == queue,
                        crate::palw_bond_budget_v1::PalwMaterialPaymentV1::Queued {
                            income: Some(i),
                            queue: None,
                            paid_daa: None,
                            ..
                        } => i == super::palw_bond_budget_fold_v1::native_queued_income_id_v1(queue, &t.supplier),
                        _ => false,
                    }
                {
                    return Err(refused("family queued cash receipt scope"));
                }
            }
            FamilySupplierFeeStateV1::Paid { payee, daa } => {
                let refund = row.defaulted_daa.is_some_and(|d| d <= t.respond_until_daa);
                if payee != if refund { t.sponsor } else { t.supplier } || daa < row.accepted_daa || daa > now || settled.is_none() {
                    return Err(refused("family paid fee exact history"));
                }
            }
        }
    }
    for claim in live_claims {
        validate_original_family_universe_v2(state, claim, rows.as_slice())?;
    }
    Ok(())
}
/// Historical restore checks the immutable birth universe, not current actor
/// eligibility or whether its already-consumed funding is free a second time.
fn validate_original_family_universe_v2(
    state: &PalwChainStateV2,
    claim: Hash64,
    rows: &[(Hash64, FamilySupplierRowV1)],
) -> Result<(), PalwStateV2Error> {
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
        .ok_or_else(|| refused("family original disappeared inside material horizon"))?;
    let plan = frame.plan_v1();
    if plan.acquisition.clock.version != crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_CLOCK_FAMILY_VERSION_V3 {
        return Err(refused("new family original substitutes historical owner timing"));
    }
    let class = frame.original_v1().job.class_id;
    let record = state.tir_class_v1(&class).ok_or_else(|| refused("family restored registered program absent"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, class)
        .ok_or_else(|| refused("family restored registered layout absent"))?;
    let map_native = |e: crate::palw_native_resource_v1::NativeDecodeErrorV1<String>| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e),
    };
    let source = frame.source_identity_native_v2(record, layout, frame.descriptor_v1().maximum_leaves).map_err(map_native)?;
    if state.claim_semantics_v1(&claim).is_some_and(|sem| sem.token_table_root.is_some()) {
        let witness = crate::palw_court_family_material_v1::read_authenticated_family_table_inventory_v1(state, claim)
            .map_err(map_native)?
            .ok_or_else(|| refused("family restored allocation table witness absent"))?;
        let context =
            witness.allocation_rules_v2.as_ref().ok_or_else(|| refused("family restored native allocation rule context absent"))?;
        let rules = context.native_rules_v2(frame.descriptor_v1());
        let sem = state.claim_semantics_v1(&claim).unwrap();
        let checked =
            crate::palw_court_family_material_v1::prepare_family_table_inventory_v1(&witness.bytes, sem, record.logits_vocab)
                .map_err(map_native)?;
        let geometry = crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
            class_id: class,
            producer: frame.custody_v1().pledge_v1().producer,
            execution_root: frame.original_v1().execution_root,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(frame.original_v1()),
            accepted_daa: plan.terms.accepted_daa,
        };
        crate::palw_tir_public_allocation_v1::check_tir_public_allocation_family_retained_v2(
            plan,
            &geometry,
            frame.original_v1(),
            frame.header_v1(),
            record,
            layout,
            Some(sem),
            state.staged_tokenizer_inventory_v1(sem.tokenizer_id),
            rules,
            Some(&checked),
        )
        .map_err(map_native)?;
    }

    let service = state.court_service_state_v1().ok_or_else(|| refused("family original restored book missing"))?;
    let profile = crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(service.profile_id);
    let schedule = crate::palw_g14_values_v1::palw_g14_funded_original_schedule_estimate_v1();
    let graph = crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3::derive_native_v3(
        frame,
        &source,
        profile,
        schedule.source_budget(),
    )
    .map_err(map_native)?;
    // An original custody always starts with the producer alone. Later native
    // grants do not retroactively change its funded birth universe.
    let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<PalwBondKeyV2, ()>>(1)
        .map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<FamilySupplierPlanV1>(4).map_err(PalwStateV2Error::NativeResource)?;
    let _actors_lease =
        crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let signers = [frame.custody_v1().pledge_v1().producer].into_iter().collect();
    let (original, _original_lease) = crate::palw_funded_original_admission_v1::derive_funded_admission_family_v2(
        claim,
        plan.allocation_id,
        plan.source_id,
        plan.terms.accepted_daa,
        &signers,
        &graph,
        &schedule.admission_v1().ok_or_else(|| refused("family restored original schedule absent"))?,
    )
    .map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => {
            refused(format!("family restored original derivation: {e:?}"))
        }
    })?;
    let sem = state.claim_semantics_v1(&claim);
    let required = [
        true,
        sem.is_some_and(|s| s.token_table_root.is_some()),
        sem.is_some_and(|s| s.raw_tokenizer_bytes.is_some()),
        matches!(frame.original_v1().job.tail.as_ref(), Some(crate::palw_freeprompt_v3::PalwFpJobTailV1::Prefix(_))),
    ];
    let mut seen = [false; 4];
    let mut family = Vec::with_capacity(4);
    for (_, row) in rows.iter().filter(|(_, r)| r.consent.terms.claim == claim) {
        let role = row.consent.terms.role as usize;
        if seen[role] || !required[role] {
            return Err(refused("family restored original duplicate or extraneous supplier role"));
        }
        seen[role] = true;
        let inventory =
            crate::palw_court_family_material_v1::derive_current_family_inventory_classified_v1(state, claim, row.consent.terms.role)
                .map_err(map_native)?;
        let ctx = PalwBlockContextV2 { block: Hash64::default(), daa_score: row.accepted_daa, blue_score: 0, subsidy: 0 };
        family.push(derive_family_supplier_plan_inner_v1(
            state,
            &ctx,
            &inventory,
            row.allocation,
            row.consent.terms.material_until_daa,
            &row.consent,
            false,
        )?);
    }
    if seen != required {
        return Err(refused("family restored original missing native supplier role"));
    }
    let count = family
        .iter()
        .try_fold(original.branch_count(), |n, p| n.checked_add(p.packets.len() as u64))
        .ok_or_else(|| refused("family restored packet count overflow"))?;
    let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<[u8; 64], ()>>(count)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _universe_lease =
        crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let binding = service.claims.get(&claim).ok_or_else(|| refused("family restored original binding missing"))?;
    let mut universe = std::collections::BTreeSet::new();
    for packet in original
        .signer_packets
        .iter()
        .chain(&original.source_packets)
        .chain(std::iter::once(&original.retention_packet))
        .chain(&original.retention_tail_v2)
        .chain(family.iter().flat_map(|p| p.packets.iter()))
    {
        if !universe.insert(packet.id) || service.book.packet(&packet.id) != Some(packet) {
            return Err(refused("family restored original packet missing, duplicated or cost changed"));
        }
    }
    if universe != binding.packet_ids {
        return Err(refused("family restored original reservation universe differs"));
    }
    Ok(())
}
fn queue_fee(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    row: &mut FamilySupplierRowV1,
    refund: bool,
) -> Result<(), PalwStateV2Error> {
    let t = &row.consent.terms;
    let payee = if refund { t.sponsor } else { t.supplier };
    let payload = b.state.bond(&payee).ok_or(PalwStateV2Error::MissingBond(payee))?.payout_payload;
    let queue = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload);
    if b.state.pending_payouts.get(&queue).is_none() && b.state.pending_payouts.len() >= PALW_V2_MAX_PAYOUTS_PER_BLOCK {
        return Ok(());
    }
    if b.state.pending_payouts.get(&queue).map_or(0, |p| p.amount).checked_add(t.fee_sompi).is_none() {
        return Ok(());
    }
    let fee = digest(b"misaka-palw/family-supplier/held-fee/v1", &id)?;
    crate::palw_settlement_liability_v1::palw_settlement_liability_sourced_v1(&b.signed_fee_tranche_v1(
        t.fee_sompi,
        t.fee_sompi,
        t.respond_until_daa,
        t.material_until_daa,
        ctx.daa_score,
    ))
    .map_err(super::palw_settlement_liability_fold_v1::liability_refused_v1)?;
    let once = b.settle_once_v1(
        ctx.daa_score,
        PalwSettlementObligationV1::of(PalwSettlementLegV1::MaterialProviderFee, fee.as_bytes()),
        PalwSettlementConsumerV1::MonetaryMaterial,
        t.fee_sompi,
        PalwSettlementDebitV1::Held { amount: t.fee_sompi },
    )?;
    if !once.pays() {
        return Err(refused("held family fee obligation already consumed"));
    }
    if refund {
        b.cancel_source_income_v1(ctx, id, t.claim, t.supplier, row.accepted_daa)?;
    } else {
        b.settle_source_income_v1(ctx, id, t.claim, t.supplier, row.accepted_daa, t.respond_until_daa, t.fee_sompi)?;
    }
    b.add_kernel_payout(payload, t.fee_sompi)?;
    let source_income = super::palw_bond_budget_fold_v1::source_income_id_v1(id, t.claim, &t.supplier);
    let receipt = crate::palw_bond_budget_v1::material_queue_receipt_id_v1(
        source_income,
        if refund { queue } else { super::palw_bond_budget_fold_v1::native_queued_income_id_v1(queue, &t.supplier) },
        ctx.daa_score,
    );
    // A receipt ties this exact fee to the coalesced cash queue until it drains.
    if refund {
        b.bond_budget_op(super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1::point(), |book, _, j| {
            if book.material_income.contains_key(&receipt) {
                return Err(crate::palw_bond_budget_v1::PalwBudgetRefusalV1::Duplicate);
            }
            book.put_material_income_v1(
                receipt,
                Some(crate::palw_bond_budget_v1::PalwMaterialIncomeV1 {
                    root: Some(t.claim),
                    amount: t.fee_sompi,
                    accepted_daa: row.accepted_daa,
                    payment: crate::palw_bond_budget_v1::PalwMaterialPaymentV1::Queued {
                        income: None,
                        queue: Some(queue),
                        queued_daa: ctx.daa_score,
                        paid_daa: None,
                        release_daa: None,
                    },
                }),
                j,
            )
        })?
        .ok_or_else(|| refused("family physical fee material engine missing"))?
        .map_err(|e| refused(e.to_string()))?;
    }
    row.fee_state = FamilySupplierFeeStateV1::Queued { payee, queue, receipt };
    Ok(())
}

#[cfg(test)]
mod encoded_width_tests {
    use super::*;
    #[test]
    fn family_control_quote_matches_literal_single_object_lifecycle_framing() {
        use crate::palw_court_family_material_v1::{FamilyMaterialActionV1 as A, SignedFamilyMaterialV1};
        let target = FamilyMaterialTargetV1::SemanticTable;
        let h = Hash64::from_u64_word;
        let submitter = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(2), 0));
        for (kind, payload, action) in [
            (0, 0, A::Begin { total_bytes: 16_384, body_hash: h(3) }),
            (1, 16_384, A::Part { draft: h(4), offset: 0, bytes: vec![7; 16_384] }),
            (2, 0, A::Finish { draft: h(4) }),
        ] {
            let control = SignedFamilyMaterialV1 {
                version: 1,
                contract: h(1),
                target,
                submitter,
                action,
                signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            };
            let wire = crate::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2 {
                version: crate::palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2,
                object: PalwConsensusObjectV2::CourtFamilyMaterialV1 { control: Box::new(control) },
            };
            assert_eq!(control_bytes(target, kind, payload).unwrap(), borsh::object_length(&wire).unwrap() as u64);
        }
    }
}
