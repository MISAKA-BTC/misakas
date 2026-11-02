//! RFC-0042 independent supplier capital and finite fee tranche. Every money
//! mutation uses the existing journal, once ledger, real bond debit and R book.
use super::*;
#[path = "palw_court_family_calendar_search_v2.rs"]
pub(super) mod calendar_search;
use crate::palw_court_family_material_v1::{FAMILY_MATERIAL_PART_BYTES_V1, FamilyMaterialTargetV1, NativeFamilyInventoryV1};
use crate::palw_court_family_supplier_v1::*;
use crate::palw_court_service_v1::{ServiceAssessment, ServiceCost, ServicePacket};
use crate::palw_settlement_once_v1::*;
use crate::palw_staged_court_state_v1::PalwStagedCourtRowV1 as Row;
fn refused(s: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.to_string())
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
pub(crate) fn family_contract_for_material_v1(
    state: &PalwChainStateV2,
    id: Hash64,
) -> Result<Option<crate::palw_native_resource_v1::NativeRetainedV1<FamilySupplierRowV1>>, PalwStateV2Error> {
    use crate::palw_native_resource_v1::{BTreeNodeEnvelopeV1, NativeRetainedV1, TirNativeRequestsV1};
    use borsh::BorshDeserialize;
    let Some(route) = state.kernel_route_state_v1() else { return Ok(None) };
    let _key_lease = crate::palw_native_resource_v1::reserve_native_requests_v1(129).map_err(PalwStateV2Error::NativeResource)?;
    let Some(raw) = route.aux.get(&(FAMILY_SUPPLIER_TABLE_V1, id.as_byte_slice().to_vec())) else { return Ok(None) };
    let shape = crate::palw_native_resource_v1::preflight_native_value_v1::<FamilySupplierRowV1>(raw.as_slice(), raw.len() as u64)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::OutOfMemory => PalwStateV2Error::NativeResource(
                crate::palw_native_resource_v1::NativeCoverageV1::overflow::<FamilySupplierRowV1>("family direct catalogue scan"),
            ),
            std::io::ErrorKind::Unsupported | std::io::ErrorKind::TimedOut => {
                PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<FamilySupplierRowV1>(
                    "family direct catalogue scan",
                    &e.to_string(),
                ))
            }
            _ => refused(e.to_string()),
        })?;
    let mut reader = raw.as_slice();
    let version = u16::deserialize_reader(&mut reader).map_err(|e| refused(e.to_string()))?;
    let terms = FamilySupplierTermsV1::deserialize_reader(&mut reader).map_err(|e| refused(e.to_string()))?;
    for _ in 0..2 {
        let n = u32::deserialize_reader(&mut reader).map_err(|e| refused(e.to_string()))? as usize;
        if n != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
            return Err(refused("family direct consent signature width"));
        }
        reader = reader.get(n..).ok_or_else(|| refused("family direct header short signature"))?;
    }
    let _allocation = Hash64::deserialize_reader(&mut reader).map_err(|e| refused(e.to_string()))?;
    let accepted = u64::deserialize_reader(&mut reader).map_err(|e| refused(e.to_string()))?;
    let targets = u32::deserialize_reader(&mut reader).map_err(|e| refused(e.to_string()))? as u64;
    let mut q = TirNativeRequestsV1::new();
    q.add_v1(shape.logical_requests_v1()).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, Hash64>>(targets).map_err(PalwStateV2Error::NativeResource)?;
    let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut row: FamilySupplierRowV1 = borsh::from_slice(raw).map_err(|e| refused(e.to_string()))?;
    if version != 2
        || row.consent.id_v1().map_err(|e| refused(e.to_string()))? != id
        || accepted < terms.accept_from_daa
        || accepted > terms.accept_until_daa
        || row.targets.is_empty()
        || row.targets.windows(2).any(|p| p[0].id >= p[1].id)
        || row.targets.iter().any(|t| t.max_bytes == 0 || t.packets.is_empty())
        || !row.completed.is_empty()
        || row.defaulted_daa.is_some()
        || row.fee_state != FamilySupplierFeeStateV1::Held
    {
        return Err(refused("family direct immutable catalogue"));
    }
    let status_key = family_status_key_v2(id);
    let raw = route.aux.get(&(FAMILY_SUPPLIER_TABLE_V1, status_key.to_vec())).ok_or_else(|| refused("family direct status absent"))?;
    let status: FamilySupplierStatusV2 = borsh::from_slice(raw).map_err(|e| refused(e.to_string()))?;
    if status.version != 2
        || status.contract != id
        || status.defaulted_daa.is_some_and(|d| d < accepted || d > terms.material_until_daa)
    {
        return Err(refused("family direct status scope"));
    }
    row.version = 1;
    row.defaulted_daa = status.defaulted_daa;
    row.fee_state = status.fee_state;
    for target in &row.targets {
        let key = family_completion_key_v2(id, target.id);
        if let Some(raw) = route.aux.get(&(FAMILY_SUPPLIER_TABLE_V1, key.to_vec())) {
            let done: FamilySupplierCompletionV2 = borsh::from_slice(raw).map_err(|e| refused(e.to_string()))?;
            if done.version != 2 || done.contract != id || done.target != target.id || done.digest == Hash64::default() {
                return Err(refused("family direct completion scope"));
            }
            row.completed.insert(target.id, done.digest);
        }
    }
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(row, lease)))
}
fn delete_family_supplier_v2(b: &mut TransitionBuilder<'_>, id: Hash64, row: &FamilySupplierRowV1) -> Result<(), PalwStateV2Error> {
    if crate::palw_court_family_supplier_v1::signed_profile_generation_v3(&row.consent.terms).map_err(refused)? == 3 {
        b.write_kernel_row_encoded_v1::<FamilyCarrierProjectionRowV3>(
            FAMILY_SUPPLIER_TABLE_V1,
            &family_carrier_projection_key_v3(id),
            None,
        )?;
    }
    for target in &row.targets {
        b.write_kernel_row_encoded_v1::<FamilyPrimaryWindowV2>(
            FAMILY_SUPPLIER_TABLE_V1,
            &family_primary_window_key_v2(id, target.id),
            None,
        )?;
    }
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
    pub windows: Vec<FamilyPrimaryWindowV2>,
    carrier_projection_v3: Option<crate::palw_court_family_material_v1::FamilyCarrierProjectionFactsV3>,
    primary_order: Vec<Hash64>,
    primary_span_daa: u64,
    _lease: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
}

/// Profile IDs include the exact unchanged prices and owner3 derivation.
/// Original-material rule4 preserves that pricing/calendar grammar. Its exact
/// rule/graph ID and longer declared H are signed independently in each consent;
/// old signatures on a shorter H or a rule3 inventory cannot authorize rule4.
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

/// Explicit independently funded draft contract. Old V1 profile IDs and old
/// restore derivation remain literal; this profile is signed by new cohorts.
pub fn family_supplier_profiles_v2() -> Result<[Hash64; 4], PalwStateV2Error> {
    let native_geometry = (
        std::mem::size_of::<PalwDeltaEntryV2>() as u64,
        std::mem::size_of::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<(u8, Vec<u8>), Vec<u8>>>() as u64,
        std::mem::size_of::<
            crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<
                crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
                crate::palw_staged_court_state_v1::PalwStagedCourtRowV1,
            >,
        >() as u64,
        std::mem::size_of::<ServicePacket>() as u64,
        std::mem::size_of::<FamilyPrimaryWindowV2>() as u64,
    );
    let old = (
        family_supplier_profiles_v1()?,
        crate::palw_g14_values_v1::PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1,
        crate::palw_court_family_material_v1::FAMILY_MATERIAL_SIGNATURE_CONTEXT_V2,
        b"native-primary-point-calendar-stride-and-ttl/v2".as_slice(),
        native_geometry,
    );
    Ok([
        digest(b"misaka-palw/family-supplier/fee-profile/v2", &(old, 2u16))?,
        digest(b"misaka-palw/family-supplier/capital-profile/v2", &(old, 2u16))?,
        digest(b"misaka-palw/family-supplier/retention-profile/v2", &(old, 2u16))?,
        digest(b"misaka-palw/family-supplier/settlement-profile/v2", &(old, 2u16))?,
    ])
}

/// New generated semantic custody purpose. Prior profile1/2 hashes and
/// inventory/calendar/fee derivations remain literal for accepted cohorts.
pub fn family_supplier_profiles_v3() -> Result<[Hash64; 4], PalwStateV2Error> {
    let previous = family_supplier_profiles_v2()?;
    let convention = (
        3u16,
        previous,
        b"generated-semantic-native-rows-tiles-fullids/actual-carrier-born-facts/m-backed-primary-metadata/v3".as_slice(),
        std::mem::size_of::<FamilyCarrierProjectionRowV3>() as u64,
    );
    Ok([
        digest(b"misaka-palw/family-supplier/fee-profile/v3", &convention)?,
        digest(b"misaka-palw/family-supplier/capital-profile/v3", &convention)?,
        digest(b"misaka-palw/family-supplier/retention-profile/v3", &convention)?,
        digest(b"misaka-palw/family-supplier/settlement-profile/v3", &convention)?,
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
fn primary_projection_bytes_v2(inventory: &NativeFamilyInventoryV1) -> Result<u64, PalwStateV2Error> {
    let targets = inventory.targets_v1().len() as u64;
    let parts =
        inventory.targets_v1().iter().try_fold(0u64, |n, (_, bytes)| add(n, bytes.div_ceil(FAMILY_MATERIAL_PART_BYTES_V1 as u64)))?;
    let mut mutable = 272u64;
    let mut material_journal = 0u64;
    for (target, n) in inventory.targets_v1() {
        let t = borsh::object_length(target).map_err(|e| refused(e.to_string()))? as u64;
        let draft = 64 + t + 68 + 8 + 64 + 8 + 8 + 8;
        mutable = add(mutable, draft)?;
        let p = n.div_ceil(FAMILY_MATERIAL_PART_BYTES_V1 as u64);
        let literals =
            add(mul(add(*n, 354 + t + 3)?, 2)?, add(mul(add(*n, mul(p, 166 + 3)?)?, 2)?, mul(add(p, 1)?, mul(draft + 74 + 3, 2)?)?)?)?;
        let entries = add(mul(p, 3)?, 4)?;
        material_journal = add(material_journal, add(literals, mul(entries, std::mem::size_of::<PalwDeltaEntryV2>() as u64)?)?)?;
    }
    let shared = if matches!(
        inventory.role_v1(),
        FamilySupplierRoleV1::ParameterAndPrompt | FamilySupplierRoleV1::PrefixGeneratedSelectionAndStop
    ) {
        targets
    } else {
        0
    };
    let table = u64::from(inventory.role_v1() == FamilySupplierRoleV1::SemanticTable);
    let rows = add(add(add(add(mul(targets, 5)?, parts)?, 2)?, shared)?, table)?;
    let keys = add(add(add(add(129, mul(targets, 453)?)?, mul(parts, 73)?)?, mul(shared, 129)?)?, mul(table, 65)?)?;
    let p = super::palw_claim_local_material_projection_v2::projection_geometry_v2(rows, keys, add(targets, 1)?, mutable, 1)?;
    // Each first target has at most Begin+all Part+Finish; each finite status
    // has create/default/queue/paid/delete. Current index copies and old/new
    // mutable journal rows must be funded even with no historical full F copy.
    let events = add(add(parts, mul(targets, 4)?)?, 5)?;
    let index = add(add(74, mul(rows, 77)?)?, keys)?;
    let marker = 64 + 2 + 64 + 64 + 64 + 8 + 74 + (2 + 64 + 64 + 8 + 8 + 138);
    let mut mirror = 0u64;
    if shared != 0 {
        for (target, n) in inventory.targets_v1() {
            let t = borsh::object_length(target).map_err(|e| refused(e.to_string()))? as u64;
            // MaterialRow's exact fixed envelope (also bounds SharedIds), the
            // native staged node, and both resident/writer raw byte owners.
            let node = std::mem::size_of::<
                crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<
                    crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
                    crate::palw_staged_court_state_v1::PalwStagedCourtRowV1,
                >,
            >() as u64;
            mirror = add(mirror, add(mul(*n, 2)?, add(410 + t + 129 + 1, node)?)?)?;
        }
    }
    let raw_nodes = add(add(mul(targets, 4)?, parts)?, table)?;
    let raw_node_bytes =
        mul(raw_nodes, std::mem::size_of::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<(u8, Vec<u8>), Vec<u8>>>() as u64)?;
    // Mirror create/delete and each immutable marker/calendar create/delete.
    let journal =
        add(add(material_journal, mul(mirror, 2)?)?, mul(mul(targets, 4)?, std::mem::size_of::<PalwDeltaEntryV2>() as u64)?)?;
    let cost_projection = mul(add(parts, mul(targets, 2)?)?, std::mem::size_of::<ServiceCost>() as u64)?;
    add(
        cost_projection,
        add(
            add(add(add(p.retained_heap, raw_node_bytes)?, mirror)?, mul(targets, marker)?)?,
            add(journal, add(mul(events, mul(index, 2)?)?, mul(targets, mul(marker + 3, 2)?)?)?)?,
        )?,
    )
}
pub fn palw_family_supplier_terms_quote_v2(
    inventory: &NativeFamilyInventoryV1,
    expected: FamilySupplierTermsV1,
) -> Result<FamilySupplierTermsV1, PalwStateV2Error> {
    use crate::palw_g14_values_v1 as g;
    let ttl = crate::palw_g14_values_v1::PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1.draft_ttl_daa;
    let span =
        crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_INCLUSION_DELAY_DAA_ESTIMATE_V1.max(inventory.native_inclusion_margin_v2());
    for (_, bytes) in inventory.targets_v1() {
        if mul(add(bytes.div_ceil(FAMILY_MATERIAL_PART_BYTES_V1 as u64), 2)?, span)? > ttl {
            return Err(refused("native primary body cannot fit existing multipart TTL/inclusion chain"));
        }
    }
    let mut out = palw_family_supplier_terms_quote_v1(inventory, expected)?;
    let [fee, capital, retention, settlement] = family_supplier_profiles_v2()?;
    out.fee_profile = fee;
    out.capital_profile = capital;
    out.retention_profile = retention;
    out.settlement_policy = settlement;
    let extra = primary_projection_bytes_v2(inventory)?;
    out.collateral_sompi = add(
        out.collateral_sompi,
        mul(
            add(retention_service_bytes_v1(inventory)?, extra)?
                .div_ceil(g::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes)
                .checked_sub(retention_nodes_v1(inventory)?)
                .ok_or_else(|| refused("primary quote holding node count"))?,
            g::PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1,
        )?,
    )?;
    out.fee_sompi = add(out.fee_sompi, mul(extra, g::PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1.sompi_per_byte)?)?;
    Ok(out)
}
/// Only this new quote selects generated semantic scope; old V2 callers
/// still obtain exactly their historical complete-inventory amounts/profile.
pub fn palw_family_supplier_terms_quote_v3(
    inventory: &NativeFamilyInventoryV1,
    expected: FamilySupplierTermsV1,
) -> Result<FamilySupplierTermsV1, PalwStateV2Error> {
    if inventory.role_v1() == FamilySupplierRoleV1::PrefixGeneratedSelectionAndStop && !inventory.generated_semantic_only_v3() {
        return Err(refused("new profile3 requires exact generated semantic native inventory"));
    }
    let facts = inventory.carrier_projection_v3().ok_or_else(|| refused("profile3 quote lacks actual carrier input projection"))?;
    let mut out = palw_family_supplier_terms_quote_v2(inventory, expected)?;
    let extra = add(carrier_projection_storage_v3(inventory, facts)?, primary_m_backed_metadata_storage_v3(inventory)?)?;
    let old_retained = add(retention_service_bytes_v1(inventory)?, primary_projection_bytes_v2(inventory)?)?;
    let quantum = crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes;
    out.collateral_sompi = add(
        out.collateral_sompi,
        mul(
            add(old_retained, extra)?
                .div_ceil(quantum)
                .checked_sub(old_retained.div_ceil(quantum))
                .ok_or_else(|| refused("profile3 carrier holding count"))?,
            crate::palw_g14_values_v1::PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1,
        )?,
    )?;
    out.fee_sompi = add(out.fee_sompi, mul(extra, crate::palw_g14_values_v1::PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1.sompi_per_byte)?)?;
    let [fee, capital, retention, settlement] = family_supplier_profiles_v3()?;
    out.fee_profile = fee;
    out.capital_profile = capital;
    out.retention_profile = retention;
    out.settlement_policy = settlement;
    Ok(out)
}
/// Actual current descriptor codec keeps these immutable birth facts as M
/// bodies. Correct only the new3 projection; old1/2 pricing/classifier is literal.
fn primary_m_backed_metadata_storage_v3(inventory: &NativeFamilyInventoryV1) -> Result<u64, PalwStateV2Error> {
    let count = inventory.targets_v1().len() as u64;
    let marker = crate::palw_court_family_material_v1::FamilyPrimaryDraftV2 {
        contract: Hash64::default(),
        version: 2,
        claim: Hash64::default(),
        target: Hash64::default(),
        draft: Hash64::default(),
        material_until_daa: 0,
    };
    let calendar =
        FamilyPrimaryWindowV2 { version: 2, contract: Hash64::default(), target: Hash64::default(), first_daa: 0, span_daa: 0 };
    let m = borsh::object_length(&marker).map_err(refused)? as u64;
    let c = borsh::object_length(&calendar).map_err(refused)? as u64;
    let rows = mul(count, 2)?;
    let keys = mul(count, 65 + 129)?;
    let bytes = mul(count, add(m, c)?)?;
    let old = super::palw_claim_local_material_projection_v2::projection_geometry_v2(rows, keys, 0, 0, 0)?;
    let actual = super::palw_claim_local_material_projection_v2::projection_geometry_v2(rows, keys, rows, bytes, 0)?;
    let current = actual.retained_heap.checked_sub(old.retained_heap).ok_or_else(|| refused("new3 M metadata ownership underflow"))?;
    // Each immutable marker/calendar is born once and deleted once. These
    // are body/Q-reference journals, separate from the existing index M and
    // already priced raw AUX create/delete journal.
    let lifecycle = mul(count, add(add(470, mul(m, 2)?)?, add(470, mul(c, 2)?)?)?)?;
    add(current, lifecycle)
}
fn carrier_projection_storage_v3(
    inventory: &NativeFamilyInventoryV1,
    facts: crate::palw_court_family_material_v1::FamilyCarrierProjectionFactsV3,
) -> Result<u64, PalwStateV2Error> {
    facts.check_v3().map_err(refused)?;
    let row = FamilyCarrierProjectionRowV3 {
        version: 3,
        contract: Hash64::default(),
        claim: Hash64::default(),
        accepted_daa: 0,
        accepted_block: Hash64::default(),
        accepted_blue_score: 0,
        source: Hash64::default(),
        allocation: Hash64::default(),
        funding_contract: Hash64::default(),
        facts,
    };
    let wire = borsh::object_length(&row).map_err(refused)? as u64;
    let literal = add(65 + 9, wire)?;
    let node = std::mem::size_of::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<(u8, Vec<u8>), Vec<u8>>>() as u64;
    let q = super::palw_claim_local_material_projection_v2::projection_geometry_v2(1, 65, 1, wire, 0)?;
    let parts =
        inventory.targets_v1().iter().try_fold(0u64, |n, (_, bytes)| add(n, bytes.div_ceil(FAMILY_MATERIAL_PART_BYTES_V1 as u64)))?;
    let events = add(add(parts, mul(inventory.targets_v1().len() as u64, 4)?)?, 7)?; // includes birth sidecar create/delete
    let own_projection_journal = mul(events, 2 * (77 + 65) + 2 * 64)?;
    let own_material_lifecycle = add(470, mul(wire, 2)?)?; // M-backed Q+body create/delete, index M excluded
    // Raw birth row/node, current owner reference, and complete create+delete
    // undo plus its native journal membership. Cross-cohort index/R pending.
    add(
        add(own_projection_journal, own_material_lifecycle)?,
        add(
            add(add(literal, node)?, q.retained_heap)?,
            add(mul(add(literal, 3)?, 2)?, mul(2, std::mem::size_of::<PalwDeltaEntryV2>() as u64)?)?,
        )?,
    )
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
    let generation = if admission {
        3
    } else {
        crate::palw_court_family_supplier_v1::signed_profile_generation_v3(&consent.terms).map_err(refused)?
    };
    let independently_reserved = generation >= 2;
    expected = match generation {
        1 => palw_family_supplier_terms_quote_v1(inventory, expected)?,
        2 => palw_family_supplier_terms_quote_v2(inventory, expected)?,
        3 => palw_family_supplier_terms_quote_v3(inventory, expected)?,
        _ => return Err(refused("unknown family native profile generation")),
    };
    let carrier_projection_v3 = if generation == 3 {
        Some(inventory.carrier_projection_v3().ok_or_else(|| refused("profile3 plan lacks carrier facts"))?)
    } else {
        None
    };
    let mut extra_retention = if independently_reserved { primary_projection_bytes_v2(inventory)? } else { 0 };
    if let Some(facts) = carrier_projection_v3 {
        extra_retention = add(
            extra_retention,
            add(carrier_projection_storage_v3(inventory, facts)?, primary_m_backed_metadata_storage_v3(inventory)?)?,
        )?;
    }
    let extra_nodes = add(retention_service_bytes_v1(inventory)?, extra_retention)?
        .div_ceil(g::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes)
        .checked_sub(retention_nodes_v1(inventory)?)
        .ok_or_else(|| refused("new primary holding node count"))?;
    let packet_count = add(add(inventory.packet_count_v1().map_err(refused)?, retention_nodes_v1(inventory)?)?, extra_nodes)?;
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
    q.exact_v1::<FamilyPrimaryWindowV2>(inventory.targets_v1().len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<Hash64>(inventory.targets_v1().len() as u64).map_err(PalwStateV2Error::NativeResource)?;
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
    let retained = add(retention_service_bytes_v1(inventory)?, extra_retention)?;
    for ordinal in 0..add(retention_nodes_v1(inventory)?, extra_nodes)? {
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
    let mut primary_order = Vec::with_capacity(inventory.targets_v1().len());
    if independently_reserved {
        let is_ids = |t: &&(FamilyMaterialTargetV1, u64)| {
            matches!(t.0, FamilyMaterialTargetV1::Input(crate::palw_court_shared_material_intake_v1::MaterialTargetV1::FullIds))
        };
        for (target, _) in inventory.targets_v1().iter().filter(is_ids).chain(inventory.targets_v1().iter().filter(|t| !is_ids(t))) {
            primary_order.push(inventory.target_id_v1(*target).map_err(refused)?);
        }
    }
    let mut prepared = FamilySupplierPlanV1 {
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
        windows: Vec::with_capacity(inventory.targets_v1().len()),
        carrier_projection_v3,
        primary_order,
        primary_span_daa: crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_INCLUSION_DELAY_DAA_ESTIMATE_V1
            .max(inventory.native_inclusion_margin_v2()),
        _lease: lease,
    };
    if independently_reserved && !admission {
        load_primary_windows_v2(state, &mut prepared)?;
    }
    Ok(prepared)
}

/// Extend the same original's all-branch candidate book; no second state commit.
pub(super) fn reserve_family_plans_v1(
    book: &mut crate::palw_court_service_state_v2::CourtServiceStateV2,
    claim: Hash64,
    plans: &mut [FamilySupplierPlanV1],
    graph: &crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3,
) -> Result<(), PalwStateV2Error> {
    let extra = plans.iter().try_fold(0u64, |n, p| add(n, p.packets.len() as u64))?;
    let _lease = crate::palw_native_resource_v1::reserve_native_requests_v1(add(
        primary_calendar_requests_v2(&book.book, add(book.book.packets.len() as u64, extra)?)?,
        mul(graph.edge_count_v3(), 4 * 64)?,
    )?)
    .map_err(PalwStateV2Error::NativeResource)?;
    let mut next = book.clone();
    reserve_family_plans_inner_v2(&mut next, claim, plans, graph)?;
    *book = next;
    Ok(())
}
fn reserve_family_plans_inner_v2(
    book: &mut crate::palw_court_service_state_v2::CourtServiceStateV2,
    claim: Hash64,
    plans: &mut [FamilySupplierPlanV1],
    graph: &crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3,
) -> Result<(), PalwStateV2Error> {
    let new = plans.iter().any(|p| !p.primary_order.is_empty());
    let mut original = Vec::new();
    if new {
        let binding = book.claims.get(&claim).ok_or_else(|| refused("material barrier original binding absent"))?;
        original.try_reserve_exact(binding.packet_ids.len()).map_err(|e| refused(e.to_string()))?;
        for id in &binding.packet_ids {
            let p = book.book.packet(id).ok_or_else(|| refused("material barrier original packet absent"))?;
            if book.book.last_observed_daa.is_some_and(|d| p.release_daa <= d) {
                return Err(refused("material barrier cannot detach an observed promise"));
            }
            original.push(p.clone());
        }
        for p in &original {
            let at = book.book.assignments.remove(&p.id).ok_or_else(|| refused("material barrier original assignment absent"))?;
            if book.book.supply.contains_key(&at) {
                return Err(refused("material barrier cannot detach recorded supply"));
            }
            let left = book.book.remaining.get_mut(&at).ok_or_else(|| refused("material barrier residual absent"))?;
            left.bytes = add(left.bytes, p.cost.bytes)?;
            left.work = add(left.work, p.cost.work)?;
            left.io = add(left.io, p.cost.io)?;
            left.controls = add(left.controls, p.cost.controls)?;
            if *left == book.book.budget {
                book.book.remaining.remove(&at);
            }
            book.book.packets.remove(&p.id);
        }
    }
    let mut global_cursor = plans
        .iter()
        .filter(|p| !p.primary_order.is_empty())
        .try_fold(0u64, |at, p| Ok::<_, PalwStateV2Error>(at.max(add(p.row.accepted_daa, p.primary_span_daa)?)))?;
    for plan in plans.iter_mut() {
        let hard = plan.row.consent.terms.respond_until_daa;
        if plan.primary_order.is_empty() {
            book.book.reserve(&plan.packets, hard).map_err(|e| refused(format!("family all-branch reservation: {e:?}")))?;
        } else {
            plan.windows.clear();
            let id = plan.row.consent.id_v1().map_err(refused)?;
            let ttl = crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1()
                .map_err(|_| refused("primary draft TTL absent"))?
                .draft_ttl_daa;
            let span = plan.primary_span_daa;
            if span == 0 {
                return Err(refused("primary inclusion clock absent"));
            }
            let mut cursor = global_cursor;
            for target_id in &plan.primary_order {
                let target =
                    plan.row.targets.iter().find(|t| t.id == *target_id).ok_or_else(|| refused("primary order lost native target"))?;
                let duration = mul(target.packets.len() as u64, span)?;
                if duration > ttl {
                    return Err(refused("native target cannot complete its existing multipart TTL/inclusion chain"));
                }
                let last = hard
                    .checked_sub(crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.service_span_daa)
                    .and_then(|v| v.checked_sub(duration))
                    .ok_or_else(|| refused("primary window cannot fit original hard"))?;
                let first = cursor;
                let requests = primary_calendar_requests_v2(&book.book, target.packets.len() as u64)?;
                let _search_lease =
                    crate::palw_native_resource_v1::reserve_native_requests_v1(requests).map_err(PalwStateV2Error::NativeResource)?;
                let mut trial = Vec::with_capacity(target.packets.len());
                for packet in &target.packets {
                    trial.push(
                        plan.packets
                            .iter()
                            .find(|p| p.id == *packet)
                            .ok_or_else(|| refused("primary calendar packet absent"))?
                            .clone(),
                    );
                }
                let costs = trial.iter().map(|p| p.cost).collect::<Vec<_>>();
                let start = calendar_search::first_fit_width_v2(&book.book, &costs, first, last, span, 1)
                    .map_err(refused)?
                    .ok_or_else(|| refused("no legal causal primary chain fits existing native capacity/TTL"))?;
                for (ordinal, p) in trial.iter_mut().enumerate() {
                    p.release_daa = add(start, mul(ordinal as u64, span)?)?;
                    p.service_before = add(p.release_daa, 1)?;
                }
                book.book.reserve(&trial, hard).map_err(|e| refused(format!("selected primary calendar: {e:?}")))?;
                let window = FamilyPrimaryWindowV2 { version: 2, contract: id, target: *target_id, first_daa: start, span_daa: span };
                cursor = add(window.first_daa, duration)?;
                global_cursor = cursor;
                for p in trial {
                    let old = plan
                        .packets
                        .iter_mut()
                        .find(|old| old.id == p.id)
                        .ok_or_else(|| refused("primary calendar publication packet absent"))?;
                    *old = p;
                }
                plan.windows.push(window);
            }
        }
        let binding = book.claims.get_mut(&claim).ok_or_else(|| refused("family original binding absent"))?;
        binding.packet_ids.extend(plan.packets.iter().map(|p| p.id));
        for target in &plan.row.targets {
            let latest = target
                .packets
                .iter()
                .filter_map(|id| book.book.assigned_daa(id))
                .max()
                .ok_or_else(|| refused("family assignment absent"))?;
            if add(latest, crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.service_span_daa)? > hard {
                return Err(refused("family response exceeds original hard"));
            }
        }
    }
    if new {
        let barrier = family_material_barrier_v2(plans)?;
        let source_ids = graph.packet_ids_v3();
        #[cfg(test)]
        {
            let source_earliest = original.iter().filter(|p| source_ids.contains(&p.id)).map(|p| p.service_before).min();
            let signer_earliest =
                original.iter().filter(|p| p.cost.io > 0 && !source_ids.contains(&p.id)).map(|p| p.service_before).min();
            let transfer_count = plans.iter().map(|p| p.row.targets.iter().map(|t| t.packets.len()).sum::<usize>()).sum::<usize>();
            let total = plans.iter().map(|p| p.packets.len()).sum::<usize>();
            eprintln!(
                "[native-family-temporal-capacity] claim={} materialBarrier={} earliestOriginalSource={:?} earliestSigner={:?} familyTransfers={} familyHolding={} nativeBudget={:?}",
                claim,
                barrier,
                source_earliest,
                signer_earliest,
                transfer_count,
                total - transfer_count,
                book.book.budget
            );
            for p in plans.iter() {
                for w in &p.windows {
                    let t = p.row.targets.iter().find(|t| t.id == w.target).unwrap();
                    eprintln!(
                        "[native-family-target-window] role={:?} target={} first={} stride={} transfers={} finish={} afterFinish={}",
                        p.row.consent.terms.role,
                        t.id,
                        w.first_daa,
                        w.span_daa,
                        t.packets.len(),
                        w.first_daa + (t.packets.len() as u64 - 1) * w.span_daa,
                        w.first_daa + t.packets.len() as u64 * w.span_daa
                    );
                }
            }
        }

        let mut sources = Vec::new();
        let mut signers = Vec::new();
        let mut holding = Vec::new();
        for mut p in original {
            if p.cost.io > 0 {
                p.release_daa = p.release_daa.max(barrier);
                if p.release_daa >= p.service_before {
                    return Err(refused("material barrier cannot precede an existing Court early deadline"));
                }
            }
            if source_ids.contains(&p.id) {
                sources.push(p)
            } else if p.cost.io > 0 {
                signers.push(p)
            } else {
                holding.push(p)
            }
        }
        let hard = book.claims.get(&claim).ok_or_else(|| refused("material barrier original lost"))?.hard_deadline_daa;
        book.book.reserve(&signers, hard).map_err(|e| refused(format!("material to signer barrier: {e:?}")))?;
        graph.bind_projection_v3(sources).map_err(refused)?.reserve_v3(&mut book.book, hard).map_err(refused)?;
        book.book.reserve(&holding, hard).map_err(|e| refused(format!("original holding after material barrier: {e:?}")))?;
        for plan in plans.iter() {
            let holding = plan
                .packets
                .iter()
                .filter(|p| !plan.row.targets.iter().any(|t| t.packets.contains(&p.id)))
                .cloned()
                .collect::<Vec<_>>();
            book.book.reserve(&holding, hard).map_err(|e| refused(format!("supplier holding after material barrier: {e:?}")))?;
        }
    }
    book.check_live_v1().map_err(refused)
}
pub(super) fn family_material_barrier_v2(plans: &[FamilySupplierPlanV1]) -> Result<u64, PalwStateV2Error> {
    let count = plans.iter().try_fold(0u64, |n, p| add(n, p.windows.len() as u64))?;
    let _lease = crate::palw_native_resource_v1::reserve_native_requests_v1(mul(count, std::mem::size_of::<(u64, u64)>() as u64)?)
        .map_err(PalwStateV2Error::NativeResource)?;
    let mut intervals = Vec::with_capacity(usize::try_from(count).map_err(refused)?);
    for p in plans {
        for w in &p.windows {
            let t = p.row.targets.iter().find(|t| t.id == w.target).ok_or_else(|| refused("material barrier target absent"))?;
            intervals.push((w.first_daa, add(w.first_daa, mul(t.packets.len() as u64, w.span_daa)?)?));
        }
    }
    intervals.sort_unstable();
    let mut end = 0;
    for (first, until) in intervals {
        if first < end {
            return Err(refused("restored primary roles overlap their native inclusion stride"));
        }
        end = until;
    }
    Ok(end)
}
fn load_primary_windows_v2(state: &PalwChainStateV2, plan: &mut FamilySupplierPlanV1) -> Result<(), PalwStateV2Error> {
    let id = plan.row.consent.id_v1().map_err(refused)?;
    let ttl = crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1()
        .map_err(|_| refused("restored primary TTL absent"))?
        .draft_ttl_daa;
    let mut cursor = add(plan.row.accepted_daa, plan.primary_span_daa)?;
    for target_id in &plan.primary_order {
        let target =
            plan.row.targets.iter().find(|t| t.id == *target_id).ok_or_else(|| refused("restored primary role order lost target"))?;
        let key = family_primary_window_key_v2(id, target.id);
        let raw = state
            .kernel_route_state_v1()
            .and_then(|r| r.aux.iter().find_map(|((t, k), v)| (*t == FAMILY_SUPPLIER_TABLE_V1 && k.as_slice() == key).then_some(v)))
            .ok_or_else(|| refused("restored primary calendar absent"))?;
        let w: FamilyPrimaryWindowV2 = borsh::from_slice(raw).map_err(|e| refused(e.to_string()))?;
        let span = plan.primary_span_daa;
        let duration = mul(target.packets.len() as u64, span)?;
        let end = add(w.first_daa, duration)?;
        if w.version != 2
            || w.contract != id
            || w.target != target.id
            || w.span_daa != span
            || w.first_daa < cursor
            || duration > ttl
            || add(end, crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.service_span_daa)?
                > plan.row.consent.terms.respond_until_daa
        {
            return Err(refused("restored primary calendar changes native clock/TTL/scope"));
        }
        for (ordinal, id) in target.packets.iter().enumerate() {
            let p = plan.packets.iter_mut().find(|p| p.id == *id).ok_or_else(|| refused("restored primary packet absent"))?;
            p.release_daa = add(w.first_daa, mul(ordinal as u64, span)?)?;
            p.service_before = add(p.release_daa, 1)?;
        }
        cursor = end;
        plan.windows.push(w);
    }
    Ok(())
}

/// Free real principal after both ledgers. R-core separates an accuser's
/// Court/DA deposits from A-1 work commitments; fees and supplier capital must
/// preserve both. The shared fee reader subtracts only the classic deposits
/// already present in pre-R-core reserved exposure, without dropping new A-6 rows.
pub(super) fn family_free_principal_v1(b: &TransitionBuilder<'_>, bond: &PalwBondKeyV2, now: u64) -> Result<u128, PalwStateV2Error> {
    let collateral = b.state.bond(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?.collateral;
    let reserved = super::palw_court_capacity_fold_v1::fee_reserved_principal_v1(b, bond, now)?;
    Ok(u128::from(collateral).saturating_sub(reserved))
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
        if family_free_principal_v1(b, &who, now)? < needed {
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
        if let Some(facts) = plan.carrier_projection_v3 {
            let birth = FamilyCarrierProjectionRowV3 {
                version: 3,
                contract: id,
                claim: row.consent.terms.claim,
                accepted_daa: row.accepted_daa,
                accepted_block: ctx.block,
                accepted_blue_score: ctx.blue_score,
                source: row.consent.terms.source,
                allocation: row.allocation,
                funding_contract: b
                    .extras
                    .tir_funded_originals
                    .get(&row.consent.terms.claim)
                    .ok_or_else(|| refused("profile3 native funding pairing vanished"))?
                    .envelope()
                    .funding
                    .terms
                    .id_v1(),
                facts,
            };
            b.write_kernel_row_encoded_v1(FAMILY_SUPPLIER_TABLE_V1, &family_carrier_projection_key_v3(id), Some(&birth))?;
        }
        for window in plan.windows {
            b.write_kernel_row_encoded_v1(FAMILY_SUPPLIER_TABLE_V1, &family_primary_window_key_v2(id, window.target), Some(&window))?;
        }
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
        let first = b.entries.len();
        reserve_copy(b, original)?;
        let mut row = original.clone();
        let t = row.consent.terms.clone();
        // Supplied reserved units are opportunities only under the adopted
        // inclusion premise, inside the original's native bounded duty clock.
        if row.defaulted_daa.is_none()
            && ctx.daa_score <= t.material_until_daa
            && let (Some(service), Some(premise)) = (b.state.court_service_state_v1(), b.extras.court_service_inclusion_premise_v1)
        {
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
        super::palw_claim_local_material_projection_v2::mark(
            b,
            t.claim,
            super::palw_claim_local_material_projection_v2::FamilyEventV2::Finance,
            first,
        )?;
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
        let first = b.entries.len();
        reserve_copy(b, original)?;
        let mut row = original.clone();
        row.fee_state = FamilySupplierFeeStateV1::Paid { payee, daa: ctx.daa_score };
        write_family_supplier_v1(b, *id, &row)?;
        super::palw_claim_local_material_projection_v2::mark(
            b,
            row.consent.terms.claim,
            super::palw_claim_local_material_projection_v2::FamilyEventV2::Finance,
            first,
        )?;
    }
    Ok(())
}

pub(crate) fn validate_family_suppliers_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
    let rows = state.family_supplier_rows_v1()?;
    let now = state.last_point().map_or(0, |p| p.daa_score);
    // The new clock is never an interpretation of a historical TFO1 frame or
    // a standalone owner row. It requires its own admitted, signed cohort.
    for stored in state.staged_courts.values() {
        match stored {
            Row::TirClaim(frame) if crate::palw_tir_frame_cost_v1::is_family_clock_v3(frame.plan_v1().acquisition.clock.version) => {
                let p = frame.plan_v1();
                if p.acquisition.clock.version == crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_CLOCK_FAMILY_MATERIAL_V4 {
                    if params.court_clock_v2_from_daa().is_none_or(|from| p.terms.accepted_daa < from) {
                        return Err(refused("family material4 lacks its original clock-v2 fence"));
                    }
                    let disclose = crate::palw_state_v2::palw_da_disclose_window_daa_v1(params);
                    let reorg = frame.descriptor_v1().native_config.reorg_retention_daa;
                    if reorg < disclose {
                        return Err(refused("family material4 retained profile does not cover actual disclosure"));
                    }
                    use crate::palw_court_clock_v2::{PalwCourtDependencyKindV2 as Kind, PalwCourtDependencyV2 as Dep};
                    let deps = [
                        Dep { kind: Kind::Custody, signed_horizon_daa: frame.custody_v1().pledge_v1().material_until_daa },
                        Dep { kind: Kind::Custody, signed_horizon_daa: p.terms.material_until_daa },
                    ];
                    crate::palw_court_clock_v2::palw_court_clock_v2_dependencies_cover_v2(
                        p.terms.accepted_daa,
                        disclose,
                        reorg,
                        deps.iter(),
                    )
                    .map_err(|_| refused("family material4 retained custody/route fails actual F07 coverage"))?;
                }
                if !params.family_supplier_active_at(p.terms.accepted_daa)
                    || !rows.iter().any(|(_, r)| r.consent.terms.claim == p.original_claim)
                {
                    return Err(refused("family clock lacks owning fork and signed supplier cohort"));
                }
            }
            Row::TirAcquisition(row)
                if crate::palw_tir_frame_cost_v1::is_family_clock_v3(row.owned.contract_v1().plan.clock.version) =>
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
        if crate::palw_court_family_supplier_v1::signed_profile_generation_v3(t).map_err(refused)? == 3 {
            let key = family_carrier_projection_key_v3(*id);
            let raw = state
                .kernel_route_state_v1()
                .and_then(|route| {
                    route
                        .aux
                        .iter()
                        .find_map(|((table, k), v)| (*table == FAMILY_SUPPLIER_TABLE_V1 && k.as_slice() == key).then_some(v))
                })
                .ok_or_else(|| refused("retained profile3 carrier birth facts absent"))?;
            let birth: FamilyCarrierProjectionRowV3 = borsh::from_slice(raw).map_err(refused)?;
            state.check_family_carrier_birth_v3(row, &birth)?;
        }
        if now <= t.material_until_daa {
            live_claims.insert(t.claim);
            let inv = crate::palw_court_family_material_v1::derive_contract_family_inventory_classified_v3(state, row).map_err(
                |e| match e {
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c) => PalwStateV2Error::NativeResource(c),
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e),
                },
            )?;
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
    if !crate::palw_tir_frame_cost_v1::is_family_clock_v3(plan.acquisition.clock.version) {
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
            crate::palw_court_family_material_v1::derive_contract_family_inventory_classified_v3(state, row).map_err(map_native)?;
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
    let independent = super::palw_court_family_material_fold_v1::draft_reservations::rows(state)?;
    let mut independent_count = 0u64;
    for row in independent.iter().filter(|r| r.claim == claim) {
        independent_count =
            add(independent_count, super::palw_court_family_material_fold_v1::draft_reservations::packets(row)?.len() as u64)?;
    }
    let count = family
        .iter()
        .try_fold(original.branch_count(), |n, p| n.checked_add(p.packets.len() as u64))
        .ok_or_else(|| refused("family restored packet count overflow"))?;
    let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<[u8; 64], ()>>(add(count, independent_count)?)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _universe_lease =
        crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let binding = service.claims.get(&claim).ok_or_else(|| refused("family restored original binding missing"))?;
    let mut universe = std::collections::BTreeSet::new();
    let barrier = family_material_barrier_v2(&family)?;
    for packet in original
        .signer_packets
        .iter()
        .chain(&original.source_packets)
        .chain(std::iter::once(&original.retention_packet))
        .chain(&original.retention_tail_v2)
    {
        let mut expected = packet.clone();
        if barrier != 0 && expected.cost.io > 0 {
            expected.release_daa = expected.release_daa.max(barrier);
        }
        if !universe.insert(expected.id) || service.book.packet(&expected.id) != Some(&expected) {
            return Err(refused("family restored original packet/barrier missing, duplicated or cost changed"));
        }
    }
    for packet in family.iter().flat_map(|p| p.packets.iter()) {
        if !universe.insert(packet.id) || service.book.packet(&packet.id) != Some(packet) {
            return Err(refused("family restored supplier packet missing or cost changed"));
        }
    }
    for row in independent.iter().filter(|r| r.claim == claim) {
        for packet in super::palw_court_family_material_fold_v1::draft_reservations::packets(row)?.iter() {
            if !universe.insert(packet.id) || service.book.packet(&packet.id) != Some(packet) {
                return Err(refused("restored independent draft packet universe differs"));
            }
        }
    }
    let _archive_universe_lease =
        super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::validate_original_archive_packets_v1(
            state,
            claim,
            service,
            &mut universe,
        )?;
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
    if !b.state.pending_payouts.contains_key(&queue) && b.state.pending_payouts.len() >= PALW_V2_MAX_PAYOUTS_PER_BLOCK {
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

fn primary_calendar_requests_v2(
    book: &crate::palw_court_service_v1::ServiceReservations,
    packets: u64,
) -> Result<u64, PalwStateV2Error> {
    use crate::palw_native_resource_v1::BTreeNodeEnvelopeV1 as Node;
    let old = crate::palw_native_resource_v1::native_value_requests_v1(book).map_err(PalwStateV2Error::NativeResource)?;
    // Trial Vec, seen/owner membership, sort references, residual/assignment/
    // packet map extension, plus the constructor's two whole service-book owners.
    let entry = (std::mem::size_of::<ServicePacket>()
        + std::mem::size_of::<ServiceCost>()
        + 2 * std::mem::size_of::<Node<[u8; 64], ()>>()
        + std::mem::size_of::<&ServicePacket>()
        + std::mem::size_of::<Node<u64, ServiceCost>>()
        + std::mem::size_of::<Node<[u8; 64], u64>>()
        + std::mem::size_of::<Node<[u8; 64], ServicePacket>>()) as u64;
    add(mul(old, 4)?, mul(packets, mul(entry, 4)?)?)
}

/// Exact source birth authentication for retained new3 metadata. No future
/// block or fabricated subsidy-bearing point participates in pre-signing.
impl PalwChainStateV2 {
    pub(crate) fn check_family_carrier_birth_v3(
        &self,
        contract: &FamilySupplierRowV1,
        birth: &FamilyCarrierProjectionRowV3,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_claim_local_v1::PalwClaimLocalInputV1 as Input;
        let terms = &contract.consent.terms;
        if crate::palw_court_family_supplier_v1::signed_profile_generation_v3(terms).map_err(refused)? != 3
            || birth.contract != contract.consent.id_v1().map_err(refused)?
            || birth.claim != terms.claim
            || birth.accepted_daa != contract.accepted_daa
            || birth.source != terms.source
            || birth.allocation != contract.allocation
        {
            return Err(refused("profile3 carrier birth contract scope differs"));
        }
        birth.facts.check_v3().map_err(refused)?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(self, terms.claim);
        if let Some(frame) = frame
            && (frame.plan_v1().allocation_id != birth.allocation
                || frame.plan_v1().terms.accepted_daa != birth.accepted_daa
                || frame.plan_v1().source_id != birth.source)
        {
            return Err(refused("profile3 retained frame differs from its own native source birth"));
        }
        if let Some(view) = super::palw_claim_local_source_admission_v8::for_birth_point_v8(
            self,
            birth.claim,
            birth.accepted_block,
            birth.accepted_daa,
            birth.accepted_blue_score,
        )? {
            // This full point is returned by the authenticated directory; do
            // not reconstruct it from the tuple with a guessed subsidy.
            let actual_point = view.accepting_point();
            let original = view.original_claim();
            if (actual_point.block, actual_point.daa_score, actual_point.blue_score)
                != (birth.accepted_block, birth.accepted_daa, birth.accepted_blue_score)
                || !matches!(original.source, PalwClaimSourceV2::FreePrompt { .. })
            {
                return Err(refused("profile3 archived carrier birth point/source differs"));
            }
            let mut jobs = view.inputs().iter().filter_map(|input| match input {
                Input::FreePromptNative { claim, job_pin, public_job: Some(job), .. } if *claim == birth.claim => {
                    Some((*job_pin, job.as_ref()))
                }
                _ => None,
            });
            let (pin, job) = jobs.next().ok_or_else(|| refused("profile3 exact source archive lacks its native FP job"))?;
            if jobs.next().is_some() || pin != original.job_identity {
                return Err(refused("profile3 exact source archive FP job is ambiguous"));
            }
            self.check_public_fp_job_v1(&birth.claim, original, job)?;
            let mut originals = view.inputs().iter().filter_map(|input| match input {
                Input::TirOriginal { claim, envelope } if *claim == birth.claim => Some(envelope.as_ref()),
                _ => None,
            });
            let archived = originals.next().ok_or_else(|| refused("profile3 exact source archive lacks actual TFP1"))?;
            if originals.next().is_some()
                || archived.pledge.claim != birth.claim
                || archived.pledge.source_id != birth.source
                || archived.pledge.job_identity != original.job_identity
                || archived.pledge.producer != original.bond
            {
                return Err(refused("profile3 archived TFP1 does not match source birth"));
            }
            if let Some(frame) = frame
                && (frame.original_v1() != job
                    || frame.header_v1() != &archived.header
                    || frame.plan_v1().resource_plan_id != archived.pledge.resource_plan_id)
            {
                return Err(refused("profile3 retained original differs from exact archived TFP1"));
            }
            let prefix = view.prefix_use().map_err(super::palw_claim_local_paid_archive_v2::state_error)?;
            if prefix.map(|signed| signed.intent.id_v1()) != birth.facts.prefix_use {
                return Err(refused("profile3 signed PrefixUse differs from actual archived source"));
            }
            if let Some(signed) = prefix {
                signed.intent.verify_signature_v1(&job.job.executor_pubkey, &signed.signature).map_err(|e| refused(e.to_string()))?;
            }
            let edges = view
                .inputs()
                .iter()
                .filter(|input| matches!(input,Input::ConsumedCarrier{consumer,..}if *consumer==birth.claim))
                .count() as u64;
            if edges > birth.facts.distinct_txids {
                return Err(refused("profile3 archived carrier edges exceed signed native input bound"));
            }
            return Ok(());
        }
        // Non-scoped direct fixtures legitimately have no V8 archive. The
        // still-retained exact same-born native claim is independent authority;
        // a later raw-ID claim or a frame alone cannot supply it.
        if let Some(live) = self.claim(&birth.claim) {
            if !matches!(live.source, PalwClaimSourceV2::FreePrompt { .. })
                || (live.accepted_block, live.accepted_daa, live.accepted_blue_score)
                    != (birth.accepted_block, birth.accepted_daa, birth.accepted_blue_score)
            {
                return Err(refused("profile3 carrier metadata cannot alias a later raw claim birth"));
            }
            if let Some(frame) = frame {
                self.check_public_fp_job_v1(&birth.claim, live, frame.original_v1())?;
            }
            return Ok(());
        }
        Err(PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<
            FamilyCarrierProjectionRowV3,
        >(
            "profile3 exact source birth unavailable",
            "no exact V8 admission or same-born live claim; frame/funding alone is not full birth provenance",
        )))
    }
}
