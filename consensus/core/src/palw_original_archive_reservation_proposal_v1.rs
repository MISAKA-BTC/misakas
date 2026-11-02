//! Original-local all-resource implementation. No public preset is selected.
//! Existing byte/capital units; old IDs and old profile meanings stay intact.
use super::super::palw_claim_local_legacy_terminal_v18::NativeOwner18FpEnvelopeV1;
use super::super::palw_original_archive_ancestors_v19::NativeAncestorArchiveDescriptorV19;
use super::archive_service_view_v11::PreparedOriginalArchiveServiceViewV11;
use super::*;
use crate::palw_claim_local_context_nodes_v2::{ComponentRootV2, ComponentSpecV2};
use crate::palw_court_original_escrow_v1::OriginalArchiveProfileKindV2;
use crate::palw_court_service_v1::{ServiceCost, ServicePacket};
use crate::palw_native_resource_v1::{BTreeNodeEnvelopeV1, NativeRetainedV1, TirNativeRequestsV1, reserve_native_requests_v1};
#[derive(Clone, Copy, Debug)]
pub(super) struct OriginalArchivePayerCeilingsV1 {
    // Budget CHOICES, never caller geometry/rates or paid coverage evidence.
    pub additional_carrier_rent: u64,
    pub dedicated_escrow_capital: u64,
}
#[derive(Clone, Copy, borsh::BorshSerialize, borsh::BorshDeserialize)]
struct PaidKnownBodyAliasV19 {
    tag: u8,
    value: RowValueV2,
    parent: u32,
}
crate::palw_native_resource_v1::wire::wire_struct!(PaidKnownBodyAliasV19;u8,RowValueV2,u32);
type PaidKnownBodyAliasesV19 = [Option<PaidKnownBodyAliasV19>; 3];
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct LifetimeGeometryV1 {
    pub retained: u64,
    pub journal: u64,
    pub holding: u64,
    pub native: u64,
}
pub(super) struct NativeOriginalArchiveReservationV1 {
    scope: (Hash64, Hash64, Hash64, Hash64, Hash64, Hash64, Hash64),
    point: PalwBlockContextV2,
    hard: u64,
    material_until: u64,
    source_min_material_until: u64,
    bodies: [u64; 21],
    input_capacity: u64,
    consumer_facts_bytes: u64,
    ancestors: NativeRetainedV1<Vec<NativeAncestorArchiveDescriptorV19>>,
    known_aliases: PaidKnownBodyAliasesV19,
    geometry: LifetimeGeometryV1,
    packets: u64,
    quantum: u64,
    minimum_rent: u64,
    reserved_rent: u64,
    capital: u64,
    profiles: [Hash64; 4],
    base_rent: u64,
    base_required: u64,
    owner18: Option<NativeOwner18FpEnvelopeV1>,
    actual_inputs: Option<u64>,
    capital_ceiling: u64,
    row_price: u64,
    stream_price: u64,
    capital_per_packet: u64,
    base_profiles: [Hash64; 4],
    legacy_frame_scope: (Hash64, u64, u64, u64),
}
fn bad(s: &'static str) -> FundedOriginalRefusalV1 {
    FundedOriginalRefusalV1::Invalid(s)
}
fn overflow() -> FundedOriginalRefusalV1 {
    FundedOriginalRefusalV1::Resource(
        crate::palw_native_resource_v1::NativeCoverageV1::overflow::<NativeOriginalArchiveReservationV1>(
            "whole original archive reservation",
        ),
    )
}
fn add(a: u64, b: u64) -> Result<u64, FundedOriginalRefusalV1> {
    a.checked_add(b).ok_or_else(overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, FundedOriginalRefusalV1> {
    a.checked_mul(b).ok_or_else(overflow)
}
fn len<T: borsh::BorshSerialize>(v: &T) -> Result<u64, FundedOriginalRefusalV1> {
    borsh::object_length(v).map(|n| n as u64).map_err(|_| bad("archive native codec"))
}
fn hash<T: borsh::BorshSerialize>(tag: u8, v: &T) -> Result<Hash64, FundedOriginalRefusalV1> {
    struct Sink(blake2b_simd::State);
    impl std::io::Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut s = Sink(blake2b_simd::Params::new().hash_length(64).key(b"misaka/original-archive/reservation/v1").to_state());
    s.0.update(&[tag]);
    borsh::BorshSerialize::serialize(v, &mut s).map_err(|_| bad("archive profile codec"))?;
    let mut out = [0; 64];
    out.copy_from_slice(s.0.finalize().as_bytes());
    Ok(Hash64::from_bytes(out))
}
fn aux_entry_retained_v19(key: u64, value: u64) -> Result<u64, FundedOriginalRefusalV1> {
    // Actual Borsh aux-map entry ((table:u8,key:Vec<u8>),value:Vec<u8>).
    // Measure its empty shape; this is canonical framing, not a new tariff.
    let empty: &[u8] = &[];
    let framing = len(&((crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, empty), empty))?;
    add(framing, add(key, value)?)
}
pub(crate) fn canonical_aux_entry_retained_bytes_v19(key: u64, value: u64) -> Result<u64, PalwStateV2Error> {
    aux_entry_retained_v19(key, value).map_err(FundedOriginalRefusalV1::into_state_v1)
}
fn delta(key: u64, old: Option<u64>, new: Option<u64>) -> Result<u64, FundedOriginalRefusalV1> {
    let mut n = add(8, key)?;
    for v in [old, new].into_iter().flatten() {
        n = add(n, add(4, v)?)?;
    }
    Ok(n)
}
fn sum(a: LifetimeGeometryV1, b: LifetimeGeometryV1) -> Result<LifetimeGeometryV1, FundedOriginalRefusalV1> {
    Ok(LifetimeGeometryV1 {
        retained: add(a.retained, b.retained)?,
        journal: add(a.journal, b.journal)?,
        holding: add(a.holding, b.holding)?,
        native: add(a.native, b.native)?,
    })
}
fn times(a: LifetimeGeometryV1, n: u64) -> Result<LifetimeGeometryV1, FundedOriginalRefusalV1> {
    Ok(LifetimeGeometryV1 {
        retained: mul(a.retained, n)?,
        journal: mul(a.journal, n)?,
        holding: mul(a.holding, n)?,
        native: mul(a.native, n)?,
    })
}
fn component(spec: ComponentSpecV2, body: u64, owned: bool) -> Result<LifetimeGeometryV1, FundedOriginalRefusalV1> {
    let g = crate::palw_claim_local_context_nodes_v2::mutation_geometry(spec, body).map_err(|e| match e {
        crate::palw_claim_local_context_nodes_v2::ContextErrorV2::Resource(c) => FundedOriginalRefusalV1::Resource(c),
        _ => bad("archive native mutation geometry"),
    })?;
    let key = if owned { 84 } else { 18 };
    let retained = add(
        add(aux_entry_retained_v19(65, add(4, body)?)?, mul(g.max_nodes, aux_entry_retained_v19(65, g.max_node_wire)?)?)?,
        aux_entry_retained_v19(key, 90)?,
    )?;
    // One immutable create and one physical collection. Reorg inverse uses
    // this same old/new journal, not another fictional publication event.
    let immutable = add(delta(65, None, Some(add(4, body)?))?, mul(g.max_nodes, delta(65, None, Some(g.max_node_wire))?)?)?;
    let journal = add(mul(immutable, 2)?, add(delta(key, Some(90), Some(90))?, delta(key, Some(90), None)?)?)?;
    Ok(LifetimeGeometryV1 { retained, journal, holding: add(retained, journal)?, native: g.retained_heap })
}
fn literal_entry_geometry_v19(bytes: u64) -> Result<LifetimeGeometryV1, FundedOriginalRefusalV1> {
    let retained = aux_entry_retained_v19(65, add(4, bytes)?)?;
    let journal = mul(2, delta(65, None, Some(add(4, bytes)?))?)?;
    Ok(LifetimeGeometryV1 { retained, journal, holding: add(retained, journal)?, native: 0 })
}
fn canonical_body_value_v19<T: borsh::BorshSerialize>(v: &T) -> Result<RowValueV2, FundedOriginalRefusalV1> {
    struct Sink {
        state: blake2b_simd::State,
        bytes: u64,
    }
    impl std::io::Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.bytes = self.bytes.checked_add(b.len() as u64).ok_or_else(|| std::io::Error::other("native body width overflow"))?;
            self.state.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut sink =
        Sink { state: blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/liability-material/v1").to_state(), bytes: 0 };
    borsh::BorshSerialize::serialize(v, &mut sink).map_err(|_| bad("native paid alias canonical body"))?;
    let mut id = [0; 64];
    id.copy_from_slice(sink.state.finalize().as_bytes());
    Ok(RowValueV2 { data: Hash64::from_bytes(id), bytes: sink.bytes })
}
fn known_aliases_from_native_v19(
    b: &TransitionBuilder<'_>,
    req: &PreparedOriginalRequirementsV1,
    parents: &[NativeAncestorArchiveDescriptorV19],
) -> Result<PaidKnownBodyAliasesV19, FundedOriginalRefusalV1> {
    let class_id = req.frame_v1().original_v1().job.class_id;
    let class = b.state.tir_class_v1(&class_id).ok_or_else(|| bad("paid alias actual native class absent"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, class_id)
        .ok_or_else(|| bad("paid alias actual native layout absent"))?;
    let known = [
        (4u8, canonical_body_value_v19(class)?),
        (5, canonical_body_value_v19(layout)?),
        (18, canonical_body_value_v19(&req.archive_rules_v10())?),
    ];
    let mut result = [None; 3];
    for (slot, (tag, value)) in known.into_iter().enumerate() {
        for (index, parent) in parents.iter().enumerate() {
            if parent.proves_paid_immutable_body_v19(&b.state, value).map_err(FundedOriginalRefusalV1::State)? {
                result[slot] = Some(PaidKnownBodyAliasV19 { tag, value, parent: u32::try_from(index).map_err(|_| overflow())? });
                break;
            }
        }
    }
    Ok(result)
}
fn validate_known_alias_shapes_v19(
    aliases: &PaidKnownBodyAliasesV19,
    bodies: &[u64; 21],
    parents: &[NativeAncestorArchiveDescriptorV19],
) -> Result<(), FundedOriginalRefusalV1> {
    for (slot, alias) in aliases.iter().enumerate() {
        if let Some(alias) = alias {
            let parent = parents.get(alias.parent as usize).ok_or_else(|| bad("paid alias changes actual ancestor index"))?;
            if alias.tag != [4, 5, 18][slot]
                || alias.value.bytes != bodies[alias.tag as usize - 1]
                || parent.old_paid_until_v19().is_none()
            {
                return Err(bad("paid alias lacks exact native body/proven scope"));
            }
        }
    }
    Ok(())
}
fn recheck_published_known_aliases_v19(state: &PalwChainStateV2, r: &OriginalArchiveReceiptV1) -> Result<(), PalwStateV2Error> {
    if receipt_uses_literal_alias_geometry_v3(r).map_err(FundedOriginalRefusalV1::into_state_v1)? {
        recheck_intra_publication_aliases_v3(state, r)?;
    }
    validate_known_alias_shapes_v19(&r.known_aliases, &r.bodies, &r.ancestors).map_err(FundedOriginalRefusalV1::into_state_v1)?;
    for alias in r.known_aliases.iter().flatten() {
        let actual = super::super::palw_claim_local_born_tir_source_v16::original_archive_body_reference_v19(
            state,
            r.claim,
            r.point,
            alias.tag as u64,
        )?;
        if actual != Some(alias.value) || !r.ancestors[alias.parent as usize].proves_paid_immutable_body_v19(state, alias.value)? {
            return Err(refused("published native body differs from exact paid alias proof"));
        }
    }
    Ok(())
}
#[cfg(test)]
pub(super) fn assert_actual_paid_alias_boundaries_v19(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
) -> Result<usize, PalwStateV2Error> {
    let before = state.state_root();
    let r = verified_original_archive_reservation_v1(state, claim, point)?
        .ok_or_else(|| refused("paid alias fixture lacks actual receipt"))?;
    recheck_published_known_aliases_v19(state, &r)?;
    let mut count = 0;
    for (slot, alias) in r.known_aliases.iter().enumerate() {
        if let Some(alias) = alias {
            count += 1;
            r.ancestors[alias.parent as usize].assert_unpaid_same_body_refusal_v19(state, alias.value)?;
            let _copy = receipt_edit_lease_v1(&r)?;
            let mut changed = (*r).clone();
            let alternative = r
                .known_aliases
                .iter()
                .flatten()
                .find(|other| other.value != alias.value)
                .ok_or_else(|| refused("shared paid fixture needs two distinct native bodies"))?;
            changed.known_aliases[slot].as_mut().unwrap().value = alternative.value;
            assert!(
                recheck_published_known_aliases_v19(state, &changed).is_err(),
                "another real paid body cannot credit this tag's changed publication"
            );
            assert!(check_receipt(state, &changed).is_err(), "signed profile binds exact body identity and credit selection");
        }
    }
    assert_eq!(state.state_root(), before);
    Ok(count)
}
fn lifetime(
    bodies: &[u64; 21],
    manifest: u64,
    reverse: u64,
    receipt: u64,
    point: &PalwBlockContextV2,
    consumer_facts_bytes: u64,
    ancestors: &[NativeAncestorArchiveDescriptorV19],
    known_aliases: &PaidKnownBodyAliasesV19,
    material_until: u64,
) -> Result<LifetimeGeometryV1, FundedOriginalRefusalV1> {
    let spec = ComponentSpecV2 { owner: 16, table: 2, schema: 10, key_bytes: 64, value_bytes: u64::MAX };
    let mut total = LifetimeGeometryV1::default();
    for bytes in bodies {
        total = sum(total, component(spec, *bytes, true)?)?;
    }
    validate_known_alias_shapes_v19(known_aliases, bodies, ancestors)?;
    for alias in known_aliases.iter().flatten() {
        let old = literal_entry_geometry_v19(alias.value.bytes)?;
        total.retained = total.retained.checked_sub(old.retained).ok_or_else(overflow)?;
        total.journal = total.journal.checked_sub(old.journal).ok_or_else(overflow)?;
        total.holding = total.holding.checked_sub(old.holding).ok_or_else(overflow)?;
    }
    // DIRECTORY: original/completed/delete; reverse: create/delete.
    total = sum(total, times(component(ComponentSpecV2 { table: 1, ..spec }, manifest, false)?, 3)?)?;
    total = sum(total, times(component(ComponentSpecV2 { table: 3, ..spec }, reverse, false)?, 2)?)?;
    // Two Source16 manifest-body/A2 generations. Exact generic A2 schema2:
    // version2 + component Vec1 + literal Vec1.
    let a2 = 2 + 4 + 90 + 4 + 72;
    let retained = aux_entry_retained_v19(65, a2)?;
    let journal = add(delta(65, None, Some(a2))?, delta(65, Some(a2), None)?)?;
    total = sum(total, times(LifetimeGeometryV1 { retained, journal, holding: add(retained, journal)?, native: retained }, 2)?)?;
    // Own financial receipt: pending, constructed, released, collected. This
    // is accounting metadata, never an invented whole-actor inventory codec.
    total = sum(total, times(component(ComponentSpecV2 { owner: 17, table: 1, schema: 1, ..spec }, receipt, true)?, 4)?)?;
    // Financial receipt three live A2 generations (100-byte schema2 body).
    let a2bytes = financial_inventory_width_v1(ancestors)?;
    let ar = aux_entry_retained_v19(65, a2bytes)?;
    let aj = add(delta(65, None, Some(a2bytes))?, delta(65, Some(a2bytes), None)?)?;
    total = sum(total, times(LifetimeGeometryV1 { retained: ar, journal: aj, holding: add(ar, aj)?, native: ar }, 3)?)?;
    // Exact source-birth lookup index, not another principal ledger.
    total = sum(total, times(component(ComponentSpecV2 { owner: 17, table: 2, schema: 1, ..spec }, 64, false)?, 4)?)?;
    // Source8 temporary reference paths are separate publications, while five
    // canonical literal B bodies alias Source16 tags3/10/11/12/13 exactly.
    // Charge those immutable B bytes once. The dependency Vec is Source8-only.
    let source = super::super::palw_claim_local_source_admission_v8::fresh_fp_source_codec_v18(point)
        .map_err(|_| bad("native Source8 codec geometry unavailable"))?;
    let body_geometry = |bytes: u64| -> Result<LifetimeGeometryV1, FundedOriginalRefusalV1> {
        let retained = aux_entry_retained_v19(65, add(4, bytes)?)?;
        let journal = mul(2, delta(65, None, Some(add(4, bytes)?))?)?;
        Ok(LifetimeGeometryV1 { retained, journal, holding: add(retained, journal)?, native: 0 })
    };
    let without_body = |mut geometry: LifetimeGeometryV1, bytes: u64| -> Result<LifetimeGeometryV1, FundedOriginalRefusalV1> {
        let body = body_geometry(bytes)?;
        geometry.retained = geometry.retained.checked_sub(body.retained).ok_or_else(overflow)?;
        geometry.journal = geometry.journal.checked_sub(body.journal).ok_or_else(overflow)?;
        geometry.holding = geometry.holding.checked_sub(body.holding).ok_or_else(overflow)?;
        Ok(geometry)
    };
    total = sum(total, component(source.directory_spec_v18(), source.manifest_bytes_v18(), false)?)?;
    total = sum(
        total,
        without_body(component(source.stored_spec_v18(), source.manifest_bytes_v18(), true)?, source.manifest_bytes_v18())?,
    )?;
    for bytes in [bodies[2], bodies[9], bodies[12], bodies[10], bodies[11]] {
        total = sum(total, without_body(component(source.temporary_literal_spec_v18(), bytes, false)?, bytes)?)?;
    }
    total = sum(total, component(source.temporary_literal_spec_v18(), source.dependency_bytes_max_v18(), false)?)?;
    let archive = source.archive_bytes_v18();
    let retained = aux_entry_retained_v19(65, archive)?;
    let journal = add(delta(65, None, Some(archive))?, delta(65, Some(archive), None)?)?;
    total = sum(total, LifetimeGeometryV1 { retained, journal, holding: add(retained, journal)?, native: retained })?;
    total = sum(
        total,
        component(
            super::super::palw_claim_local_carrier_lineage_v9::consumer_facts_component_spec_v18(),
            consumer_facts_bytes,
            false,
        )?,
    )?;
    total = sum(total, ancestor_lease_geometry_v1(ancestors, material_until)?)?;
    Ok(total)
}
fn financial_inventory_width_v1(ancestors: &[NativeAncestorArchiveDescriptorV19]) -> Result<u64, FundedOriginalRefusalV1> {
    let mut components = 1u64;
    let mut literals = 0u64;
    for parent in ancestors {
        let refs = parent.archive_references_v19();
        components = add(components, refs.components.len() as u64)?;
        literals = add(literals, refs.literals.len() as u64)?;
    }
    add(add(2 + 4 + 4, mul(components, 90)?)?, mul(literals, 72)?)
}
fn ancestor_lease_geometry_v1(
    ancestors: &[NativeAncestorArchiveDescriptorV19],
    _material_until: u64,
) -> Result<LifetimeGeometryV1, FundedOriginalRefusalV1> {
    let mut total = LifetimeGeometryV1::default();
    for parent in ancestors {
        let already_paid = parent.old_paid_until_v19().is_some();
        let retained = parent.retained_bytes_v19();
        // Row rent is a one-time canonical-byte charge, not byte*DAA. The
        // exact archived paid proof credits the unchanged immutable entries;
        // Every child reserves its OWN contingent holding: accepted unresolved
        // material duties can survive minimumH even if signed childH<=oldH.
        // Old row-rent credit is not a transferable capital reservation; the
        // original can release/refund while this constructed child still owns
        // material. No duration multiplier or second immutable row burn.
        total = sum(
            total,
            LifetimeGeometryV1 { retained: if already_paid { 0 } else { retained }, journal: 0, holding: retained, native: 0 },
        )?;
        for row in parent.discovery_rows_v19() {
            let geometry = if row.is_direct_aux_v19() {
                let retained = aux_entry_retained_v19(row.key_v19().len() as u64, row.body_bytes_v19())?;
                let journal = delta(row.key_v19().len() as u64, Some(row.body_bytes_v19()), None)?;
                LifetimeGeometryV1 { retained, journal, holding: add(retained, journal)?, native: retained }
            } else {
                // Row B is included in the exact immutable closure. The
                // additional bounded paths/current pointer/delete are here.
                let mut g = component(row.spec_v19(), row.body_bytes_v19(), false)?;
                let body_retained = aux_entry_retained_v19(65, add(4, row.body_bytes_v19())?)?;
                let body_journal = mul(2, delta(65, None, Some(add(4, row.body_bytes_v19())?))?)?;
                g.retained = g.retained.checked_sub(body_retained).ok_or_else(overflow)?;
                g.journal = g.journal.checked_sub(body_journal).ok_or_else(overflow)?;
                g.holding = add(g.retained, g.journal)?;
                g
            };
            // The new owner's bounded discovery-path/delete obligation and
            // contingent capacity must also exist before original release.
            // Only exact immutable closure entries get old row-rent credit.
            total = sum(total, times(geometry, row.mutation_count_v19())?)?;
        }
        let publication = parent.publication_bytes_v19();
        if publication > 0 {
            if already_paid {
                return Err(bad("paid ancestor cannot fabricate a fresh legacy publication"));
            }
            let journal = mul(2, delta(65, None, Some(add(4, publication)?))?)?;
            total = sum(total, LifetimeGeometryV1 { retained: 0, journal, holding: journal, native: publication })?;
        }
        // New child-owned lease metadata always pays its own create/delete.
        total = sum(total, component(ancestor_index_spec_v1(), ancestor_index_width_v1(parent)?, true)?)?;
    }
    Ok(total)
}

impl NativeOriginalArchiveReservationV1 {
    pub(super) fn paid_known_alias_count_v19(&self) -> usize {
        self.known_aliases.iter().flatten().count()
    }
    pub(super) fn material_until_v1(&self) -> u64 {
        self.material_until
    }
    pub(super) fn profiles_v1(&self) -> [Hash64; 4] {
        self.profiles
    }
    pub(super) fn body_bounds_v1(&self) -> &[u64; 21] {
        &self.bodies
    }
    pub(super) fn input_capacity_v1(&self) -> u64 {
        self.input_capacity
    }
    pub(super) fn geometry_v1(&self) -> LifetimeGeometryV1 {
        self.geometry
    }
    pub(super) fn capital_v1(&self) -> u64 {
        self.capital
    }
    pub(super) fn packet_count_v1(&self) -> u64 {
        self.packets
    }
    pub(super) fn minimum_rent_v1(&self) -> u64 {
        self.minimum_rent
    }
    pub(super) fn total_declared_rent_v1(&self) -> Result<u64, FundedOriginalRefusalV1> {
        add(self.base_rent, self.reserved_rent)
    }
    pub(super) fn total_required_funding_v1(&self) -> Result<u64, FundedOriginalRefusalV1> {
        add(self.base_required, self.capital)
    }
    pub(super) fn signed_funding_amount_v1(&self) -> Result<u64, FundedOriginalRefusalV1> {
        add(self.base_required, self.capital_ceiling)
    }
    pub(super) fn legacy_frame_scope_v1(&self) -> (Hash64, u64, u64, u64) {
        self.legacy_frame_scope
    }
    pub(super) fn check_actual_inputs_v1(&self) -> Result<(), FundedOriginalRefusalV1> {
        match self.actual_inputs {
            Some(n) if n <= self.input_capacity => Ok(()),
            Some(_) => Err(bad("native actual input basis exceeds payer archive capacity")),
            None => Err(bad("archive admission requires actual native input basis")),
        }
    }
    pub(super) fn packet_v1(&self, ordinal: u64) -> Result<ServicePacket, FundedOriginalRefusalV1> {
        if ordinal >= self.packets {
            return Err(bad("archive holding ordinal outside native reservation"));
        }
        let remaining = self.geometry.holding.checked_sub(mul(ordinal, self.quantum)?).ok_or_else(overflow)?;
        let id = hash(6, &(self.scope.0, self.scope.2, self.profiles, ordinal))?;
        let mut bytes = [0; 64];
        bytes.copy_from_slice(id.as_byte_slice());
        Ok(ServicePacket {
            id: bytes,
            release_daa: self.point.daa_score.checked_add(1).ok_or_else(overflow)?,
            service_before: self.hard,
            cost: ServiceCost { bytes: remaining.min(self.quantum), work: 1, io: 0, controls: 1 },
        })
    }
}
/// Native complete own-lifetime quote; budget inversion accounts for tag6/7
/// growth from its own holding packets. No future Claim or accepted point is
/// fabricated. A signing quote can lack actual work; admission cannot.
pub(super) fn quote_original_archive_reservation_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    view: &PreparedOriginalArchiveServiceViewV11<'_>,
    object: &PalwConsensusObjectV2,
    ceilings: OriginalArchivePayerCeilingsV1,
) -> Result<NativeRetainedV1<NativeOriginalArchiveReservationV1>, FundedOriginalRefusalV1> {
    quote_internal_v1(b, ctx, view, Some(object), Some(ceilings), false)
}
pub(super) fn quote_original_archive_at_native_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    view: &PreparedOriginalArchiveServiceViewV11<'_>,
    ceilings: OriginalArchivePayerCeilingsV1,
) -> Result<NativeRetainedV1<NativeOriginalArchiveReservationV1>, FundedOriginalRefusalV1> {
    quote_internal_v1(b, ctx, view, None, Some(ceilings), false)
}
pub(super) fn quote_minimum_original_archive_at_native_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    view: &PreparedOriginalArchiveServiceViewV11<'_>,
) -> Result<NativeRetainedV1<NativeOriginalArchiveReservationV1>, FundedOriginalRefusalV1> {
    quote_internal_v1(b, ctx, view, None, None, false)
}
pub(super) fn quote_original_archive_reservation_v2(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    view: &PreparedOriginalArchiveServiceViewV11<'_>,
    object: &PalwConsensusObjectV2,
    ceilings: OriginalArchivePayerCeilingsV1,
) -> Result<NativeRetainedV1<NativeOriginalArchiveReservationV1>, FundedOriginalRefusalV1> {
    quote_internal_v1(b, ctx, view, Some(object), Some(ceilings), true)
}
pub(super) fn minimum_source_material_until_v19(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    frame: &Frame,
) -> Result<u64, FundedOriginalRefusalV1> {
    // Same source-clock choices as the actual FP Claim writer. No synthetic
    // future Claim or guessed terminal verdict is used to obtain this floor.
    let trace = if b.extras.fp_da_pins_active {
        ctx.daa_score
            .saturating_add(crate::palw_producer_v2::palw_min_trace_retention_daa_at_v2(b.params, ctx.daa_score))
            .saturating_add(crate::palw_rfc0018_v1::palw_gate_c_retention_extension_daa_v1(b.params, ctx.daa_score))
    } else {
        frame.original_v1().trace_retention_daa
    };
    let retire = ctx.daa_score.checked_add(b.params.claim_retirement_daa()).ok_or_else(overflow)?;
    let mut until = trace.max(retire).max(frame.plan_v1().terms.material_until_daa);
    if let Some(declarations) = b.extras.prefix_dependencies_v1.get(&frame.plan_v1().original_claim) {
        for export in &declarations.exports {
            until = until.max(export.intent.material_until);
        }
    }
    Ok(until)
}
fn quote_internal_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    view: &PreparedOriginalArchiveServiceViewV11<'_>,
    object: Option<&PalwConsensusObjectV2>,
    requested: Option<OriginalArchivePayerCeilingsV1>,
    mixed: bool,
) -> Result<NativeRetainedV1<NativeOriginalArchiveReservationV1>, FundedOriginalRefusalV1> {
    quote_internal_geometry_v3(b, ctx, view, object, requested, mixed, mixed)
}
fn quote_internal_geometry_v3(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    view: &PreparedOriginalArchiveServiceViewV11<'_>,
    object: Option<&PalwConsensusObjectV2>,
    requested: Option<OriginalArchivePayerCeilingsV1>,
    mixed: bool,
    dedup: bool,
) -> Result<NativeRetainedV1<NativeOriginalArchiveReservationV1>, FundedOriginalRefusalV1> {
    if mixed && b.extras.native_fact_ruleset.is_none() {
        return Err(FundedOriginalRefusalV1::State(PalwStateV2Error::NativeContextUnavailable(
            crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
        )));
    }
    let req = view.native_requirements_for_archive_v1();
    let adjunct = object
        .map(|object| {
            req.quote_archive_adjuncts_v11(b, ctx, object).map_err(|e| match e {
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c) => FundedOriginalRefusalV1::Resource(c),
                _ => bad("archive native adjunct source"),
            })
        })
        .transpose()?;
    let frame = req.frame_v1();
    let p = frame.plan_v1();
    let scope = view.scope_v11();
    if mixed {
        let parent =
            b.claim_local_original_parent_v19.ok_or(FundedOriginalRefusalV1::State(PalwStateV2Error::NativeContextUnavailable(
                crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
            )))?;
        // Actual parent members and their transitive native material only.
        // No future Claim/accepted-work facts or unrelated legacy scopes are
        // projected; final actual15/18 publication independently rechecks them.
        // This executes before Source17 installation/reservation writes.
        let native_material = super::super::palw_claim_local_retired_dependency_material_v24::preflight_parent_cohort_material_v26(
            parent,
            b.params,
            ctx,
            frame.original_v1().execution_root,
        )
        .map_err(FundedOriginalRefusalV1::State)?
        .ok_or(FundedOriginalRefusalV1::State(PalwStateV2Error::NativeContextUnavailable(
            crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
        )))?;
        if native_material.execution_root_v26() != frame.original_v1().execution_root
            || native_material.parent_point_v26() != parent.last_point
            || native_material.candidate_point_v26() != *ctx
        {
            return Err(bad("mixed original parent material scope differs"));
        }
    }
    let class = b.state.tir_class_v1(&frame.original_v1().job.class_id).ok_or_else(|| bad("archive actual class missing"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, frame.original_v1().job.class_id)
        .ok_or_else(|| bad("archive actual layout missing"))?;
    let mut bodies = [0; 21];
    bodies[0] = len(frame)?;
    bodies[1] = p.control_footprint.maximum_frame_bytes;
    bodies[3] = len(class)?;
    bodies[4] = len(layout)?;
    bodies[19] = bodies[3];
    bodies[20] = bodies[4];
    bodies[5] = len(view.binding_v11())?;
    bodies[6] = add(4, mul(view.packet_ids_v11().len() as u64, 64 + 8 + 8 + 4 * 8)?)?;
    bodies[17] = len(&req.archive_rules_v10())?;
    if let Some(adjunct) = adjunct {
        for body in adjunct.bodies() {
            bodies[body.tag as usize - 1] = body.canonical_bytes_max;
        }
    } else {
        native_metadata_bounds_v1(req, b, ctx, &mut bodies)?;
    }
    let input = req.input_archive_footprint_v12();
    if let Some(input) = input
        && input.scope_v12() != (scope.0, scope.1, scope.2, scope.3, *ctx)
    {
        return Err(bad("archive actual input basis substitutes scope"));
    }
    let prices = frame.descriptor_v1().native_config.prices;
    let quantum = view.native_calendar_for_archive_v1().budget.bytes;
    let cap_per = crate::palw_g14_values_v1::PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1;
    if quantum == 0 || cap_per == 0 || prices.rent_per_row_byte == 0 || prices.rent_per_stream_byte == 0 {
        return Err(bad("archive existing byte/capital unit missing"));
    }
    let hard = view.binding_v11().hard_deadline_daa;
    let first = ctx.daa_score.checked_add(1).ok_or_else(overflow)?;
    let capacity_limit = mul(hard.checked_sub(first).ok_or_else(overflow)?, view.native_calendar_for_archive_v1().budget.controls)?;
    let max_packets = requested.map(|c| c.dedicated_escrow_capital / cap_per).unwrap_or(capacity_limit).min(capacity_limit);
    // Shape-only canonical codec widths, not state or accepted witnesses.
    let root = ComponentRootV2 {
        spec: ComponentSpecV2 { owner: 16, table: 2, schema: 10, key_bytes: 64, value_bytes: u64::MAX },
        node: Some(scope.0),
        rows: 21,
    };
    let manifest = len(&(16u16, scope.0, *ctx, scope.0, scope.0, root, 0u64, 0u64, false))?;
    let reverse = len(&(scope.0, *ctx))?;
    let actual_pair = b.extras.tir_funded_originals.get(&scope.0).ok_or_else(|| bad("archive actual funding terms absent"))?;
    let source_min_material_until = minimum_source_material_until_v19(b, ctx, frame)?;
    let material_until = actual_pair.envelope().funding.terms.material_until_daa.max(source_min_material_until);
    let ancestors = super::super::palw_original_archive_ancestors_v19::prepare_native_ancestor_archive_descriptors_v19(b, ctx, req)
        .map_err(FundedOriginalRefusalV1::State)?;
    let known_aliases = known_aliases_from_native_v19(b, req, &ancestors)?;
    let owner18 = if mixed {
        Some(
            super::super::palw_claim_local_legacy_terminal_v18::prepare_fp_owner18_envelope_v1(b, ctx, req)
                .map_err(FundedOriginalRefusalV1::State)?,
        )
    } else {
        None
    };
    let receipt = add(
        receipt_codec_upper_v1(ctx, &actual_pair.envelope().funding.terms, &ancestors, &known_aliases)?,
        owner18.as_ref().map(len).transpose()?.unwrap_or(0),
    )?;
    let consumer_facts_bytes =
        super::super::palw_claim_local_carrier_lineage_v9::consumer_facts_codec_from_native_inputs_v18(b.extras, scope.0, ctx)
            .map_err(|_| bad("native Carrier9 input geometry unavailable"))?
            .body_bytes_max_v18();
    let max_packets = max_packets.min((u32::MAX as u64).checked_sub(view.packet_ids_v11().len() as u64).ok_or_else(overflow)?);
    let evaluate = |input_bytes: u64| -> Result<Option<(u64, [u64; 21], LifetimeGeometryV1, u64)>, FundedOriginalRefusalV1> {
        let mut zero = bodies;
        zero[9] = input_bytes;
        let base = lifetime_composed_geometry_v3(
            dedup,
            &zero,
            manifest,
            reverse,
            receipt,
            ctx,
            consumer_facts_bytes,
            &ancestors,
            &known_aliases,
            material_until,
            owner18.as_ref(),
        )?;
        // One retained body + create/delete journal, times binding ID64 plus
        // ServicePacket112: exact affine geometry H(n)=H(0)+528*n.
        let feedback = mul(3, add(64, 64 + 8 + 8 + 4 * 8)?)?;
        let denominator = quantum
            .checked_sub(feedback)
            .filter(|d| *d > 0)
            .ok_or_else(|| bad("archive byte quantum cannot close native packet feedback"))?;
        let n = base.holding.div_ceil(denominator);
        if n > max_packets {
            return Ok(None);
        }
        let mut body = zero;
        body[5] = add(body[5], mul(n, 64)?)?;
        body[6] = add(body[6], mul(n, 64 + 8 + 8 + 4 * 8)?)?;
        let g = lifetime_composed_geometry_v3(
            dedup,
            &body,
            manifest,
            reverse,
            receipt,
            ctx,
            consumer_facts_bytes,
            &ancestors,
            &known_aliases,
            material_until,
            owner18.as_ref(),
        )?;
        if g.holding.div_ceil(quantum) > n {
            return Err(bad("archive packet feedback changed; no bounded native fixed point"));
        }
        let fee = add(mul(g.retained, prices.rent_per_row_byte)?, mul(g.journal, prices.rent_per_stream_byte)?)?;
        Ok(requested.is_none_or(|c| fee <= c.additional_carrier_rent).then_some((n, body, g, fee)))
    };
    let (input_capacity, (n, bodies, geometry, minimum_rent), ceilings) = if let Some(ceilings) = requested {
        // Validate the constant/minimum geometry first. Larger byte probes
        // whose checked arithmetic overflows cannot fit a u64 signed budget;
        // they are monotone upper failures, not fabricated missing resources.
        if evaluate(4)?.is_none() {
            return Err(bad("payer ceilings cannot reserve minimum canonical input Vec"));
        }
        let (mut lo, mut hi) = (4u64, ceilings.additional_carrier_rent / prices.rent_per_row_byte);
        let mut best = None;
        while lo <= hi {
            let mid = lo + (hi - lo) / 2;
            let candidate = match evaluate(mid) {
                Err(FundedOriginalRefusalV1::Resource(crate::palw_native_resource_v1::NativeCoverageV1::Overflow { .. })) => None,
                other => other?,
            };
            match candidate {
                Some(v) => {
                    best = Some((mid, v));
                    lo = mid.checked_add(1).ok_or_else(overflow)?;
                }
                None => {
                    if mid == 0 {
                        break;
                    }
                    hi = mid - 1;
                }
            }
        }
        let (i, v) = best.ok_or_else(|| bad("payer ceilings cannot reserve complete own archive lifecycle"))?;
        (i, v, ceilings)
    } else {
        let i = input.ok_or_else(|| bad("minimum archive signing quote needs actual native input basis"))?.canonical_bytes_max_v12();
        let v = evaluate(i)?.ok_or_else(|| bad("native minimum own archive exceeds current service/capital envelope"))?;
        let c = OriginalArchivePayerCeilingsV1 { additional_carrier_rent: v.3, dedicated_escrow_capital: mul(v.0, cap_per)? };
        (i, v, c)
    };
    let schedule = palw_funded_original_schedule_v1(b.extras).map_err(FundedOriginalRefusalV1::Missing)?;
    // Old producer-deposit credit is applied to OLD original capital only.
    let base_required = crate::palw_funded_original_admission_v1::funded_required_amount_v1(
        schedule.fee_sompi(),
        schedule.capital_floor_sompi(),
        view.base_capital_for_archive_v1(),
        p.terms.plan.deposit_sompi,
    )
    .ok_or_else(overflow)?;
    let capital = mul(n, cap_per)?;
    let base =
        [schedule.fee_profile_id(), schedule.capital_profile_id(), schedule.retention_profile_id(), schedule.settlement_policy_id()];
    let terms = (
        1u16,
        base,
        (scope.1, scope.5, material_until, source_min_material_until),
        (bodies, consumer_facts_bytes, ancestors.as_slice(), known_aliases),
        (n, p.terms.plan.rent_sompi, base_required),
        (ceilings.additional_carrier_rent, ceilings.dedicated_escrow_capital),
        (prices.rent_per_row_byte, prices.rent_per_stream_byte, quantum, cap_per),
    );
    let profiles = if let Some(envelope) = owner18.as_ref() {
        if dedup {
            [
                profile_v2(0, &(2u16, &terms, envelope, OAR2_LITERAL_ALIAS_GEOMETRY_DOMAIN_V3))?,
                profile_v2(1, &(2u16, &terms, envelope, OAR2_LITERAL_ALIAS_GEOMETRY_DOMAIN_V3))?,
                profile_v2(2, &(2u16, &terms, envelope, OAR2_LITERAL_ALIAS_GEOMETRY_DOMAIN_V3))?,
                profile_v2(3, &(2u16, &terms, envelope, OAR2_LITERAL_ALIAS_GEOMETRY_DOMAIN_V3))?,
            ]
        } else {
            [
                profile_v2(0, &(2u16, &terms, envelope))?,
                profile_v2(1, &(2u16, &terms, envelope))?,
                profile_v2(2, &(2u16, &terms, envelope))?,
                profile_v2(3, &(2u16, &terms, envelope))?,
            ]
        }
    } else {
        [profile(0, &terms)?, profile(1, &terms)?, profile(2, &terms)?, profile(3, &terms)?]
    };
    let mut memory = TirNativeRequestsV1::new();
    memory.exact_v1::<NativeRetainedV1<NativeOriginalArchiveReservationV1>>(1).map_err(FundedOriginalRefusalV1::Resource)?;
    memory.exact_v1::<FundedOriginalPlanV1>(1).map_err(FundedOriginalRefusalV1::Resource)?;
    let lease = reserve_native_requests_v1(memory.bytes_v1()).map_err(FundedOriginalRefusalV1::Resource)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        NativeOriginalArchiveReservationV1 {
            scope,
            point: *ctx,
            hard,
            material_until,
            source_min_material_until,
            bodies,
            input_capacity,
            consumer_facts_bytes,
            ancestors,
            known_aliases,
            geometry,
            packets: n,
            quantum,
            minimum_rent,
            reserved_rent: ceilings.additional_carrier_rent,
            capital,
            profiles,
            base_rent: p.terms.plan.rent_sompi,
            base_required,
            capital_ceiling: ceilings.dedicated_escrow_capital,
            row_price: prices.rent_per_row_byte,
            stream_price: prices.rent_per_stream_byte,
            capital_per_packet: cap_per,
            base_profiles: base,
            owner18,
            actual_inputs: input.map(|i| i.canonical_bytes_max_v12()),
            legacy_frame_scope: (p.resource_plan_id, p.terms.material_until_daa, p.controls.row_bytes, p.controls.rent_sompi),
        },
        lease,
    ))
}

#[derive(Clone)]
pub(crate) struct OriginalArchiveReceiptV1 {
    version: u16,
    claim: Hash64,
    source: Hash64,
    allocation: Hash64,
    resource_plan: Hash64,
    rules: Hash64,
    relative_graph: Hash64,
    absolute_graph: Hash64,
    contract: Hash64,
    point: PalwBlockContextV2,
    hard: u64,
    material_until: u64,
    source_min_material_until: u64,
    bodies: [u64; 21],
    input_capacity: u64,
    consumer_facts_bytes: u64,
    holding: u64,
    packets: u64,
    quantum: u64,
    capital: u64,
    base_rent: u64,
    reserved_rent: u64,
    base_required: u64,
    capital_ceiling: u64,
    row_price: u64,
    stream_price: u64,
    capital_per_packet: u64,
    base_profiles: [Hash64; 4],
    profiles: [Hash64; 4],
    funding_terms: crate::palw_court_original_escrow_v1::FundingTermsV1,
    ancestors: Vec<NativeAncestorArchiveDescriptorV19>,
    known_aliases: PaidKnownBodyAliasesV19,
    constructed: bool,
    released_at: Option<u64>,
    owner18: Option<NativeOwner18FpEnvelopeV1>,
}
impl borsh::BorshSerialize for OriginalArchiveReceiptV1 {
    fn serialize<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.version, w)?;
        borsh::BorshSerialize::serialize(&self.claim, w)?;
        borsh::BorshSerialize::serialize(&self.source, w)?;
        borsh::BorshSerialize::serialize(&self.allocation, w)?;
        borsh::BorshSerialize::serialize(&self.resource_plan, w)?;
        borsh::BorshSerialize::serialize(&self.rules, w)?;
        borsh::BorshSerialize::serialize(&self.relative_graph, w)?;
        borsh::BorshSerialize::serialize(&self.absolute_graph, w)?;
        borsh::BorshSerialize::serialize(&self.contract, w)?;
        borsh::BorshSerialize::serialize(&self.point, w)?;
        borsh::BorshSerialize::serialize(&self.hard, w)?;
        borsh::BorshSerialize::serialize(&self.material_until, w)?;
        borsh::BorshSerialize::serialize(&self.source_min_material_until, w)?;
        borsh::BorshSerialize::serialize(&self.bodies, w)?;
        borsh::BorshSerialize::serialize(&self.input_capacity, w)?;
        borsh::BorshSerialize::serialize(&self.consumer_facts_bytes, w)?;
        borsh::BorshSerialize::serialize(&self.holding, w)?;
        borsh::BorshSerialize::serialize(&self.packets, w)?;
        borsh::BorshSerialize::serialize(&self.quantum, w)?;
        borsh::BorshSerialize::serialize(&self.capital, w)?;
        borsh::BorshSerialize::serialize(&self.base_rent, w)?;
        borsh::BorshSerialize::serialize(&self.reserved_rent, w)?;
        borsh::BorshSerialize::serialize(&self.base_required, w)?;
        borsh::BorshSerialize::serialize(&self.capital_ceiling, w)?;
        borsh::BorshSerialize::serialize(&self.row_price, w)?;
        borsh::BorshSerialize::serialize(&self.stream_price, w)?;
        borsh::BorshSerialize::serialize(&self.capital_per_packet, w)?;
        borsh::BorshSerialize::serialize(&self.base_profiles, w)?;
        borsh::BorshSerialize::serialize(&self.profiles, w)?;
        borsh::BorshSerialize::serialize(&self.funding_terms, w)?;
        borsh::BorshSerialize::serialize(&self.ancestors, w)?;
        borsh::BorshSerialize::serialize(&self.known_aliases, w)?;
        borsh::BorshSerialize::serialize(&self.constructed, w)?;
        borsh::BorshSerialize::serialize(&self.released_at, w)?;
        match (self.version, self.owner18.as_ref()) {
            (1, None) => {}
            (2, Some(e)) => borsh::BorshSerialize::serialize(e, w)?,
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "archive receipt version/envelope")),
        }
        Ok(())
    }
}
impl borsh::BorshDeserialize for OriginalArchiveReceiptV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        let version = <u16 as borsh::BorshDeserialize>::deserialize_reader(r)?;
        if !matches!(version, 1 | 2) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "archive receipt version"));
        }
        Ok(Self {
            version,
            claim: borsh::BorshDeserialize::deserialize_reader(r)?,
            source: borsh::BorshDeserialize::deserialize_reader(r)?,
            allocation: borsh::BorshDeserialize::deserialize_reader(r)?,
            resource_plan: borsh::BorshDeserialize::deserialize_reader(r)?,
            rules: borsh::BorshDeserialize::deserialize_reader(r)?,
            relative_graph: borsh::BorshDeserialize::deserialize_reader(r)?,
            absolute_graph: borsh::BorshDeserialize::deserialize_reader(r)?,
            contract: borsh::BorshDeserialize::deserialize_reader(r)?,
            point: borsh::BorshDeserialize::deserialize_reader(r)?,
            hard: borsh::BorshDeserialize::deserialize_reader(r)?,
            material_until: borsh::BorshDeserialize::deserialize_reader(r)?,
            source_min_material_until: borsh::BorshDeserialize::deserialize_reader(r)?,
            bodies: borsh::BorshDeserialize::deserialize_reader(r)?,
            input_capacity: borsh::BorshDeserialize::deserialize_reader(r)?,
            consumer_facts_bytes: borsh::BorshDeserialize::deserialize_reader(r)?,
            holding: borsh::BorshDeserialize::deserialize_reader(r)?,
            packets: borsh::BorshDeserialize::deserialize_reader(r)?,
            quantum: borsh::BorshDeserialize::deserialize_reader(r)?,
            capital: borsh::BorshDeserialize::deserialize_reader(r)?,
            base_rent: borsh::BorshDeserialize::deserialize_reader(r)?,
            reserved_rent: borsh::BorshDeserialize::deserialize_reader(r)?,
            base_required: borsh::BorshDeserialize::deserialize_reader(r)?,
            capital_ceiling: borsh::BorshDeserialize::deserialize_reader(r)?,
            row_price: borsh::BorshDeserialize::deserialize_reader(r)?,
            stream_price: borsh::BorshDeserialize::deserialize_reader(r)?,
            capital_per_packet: borsh::BorshDeserialize::deserialize_reader(r)?,
            base_profiles: borsh::BorshDeserialize::deserialize_reader(r)?,
            profiles: borsh::BorshDeserialize::deserialize_reader(r)?,
            funding_terms: borsh::BorshDeserialize::deserialize_reader(r)?,
            ancestors: borsh::BorshDeserialize::deserialize_reader(r)?,
            known_aliases: borsh::BorshDeserialize::deserialize_reader(r)?,
            constructed: borsh::BorshDeserialize::deserialize_reader(r)?,
            released_at: borsh::BorshDeserialize::deserialize_reader(r)?,
            owner18: if version == 2 { Some(borsh::BorshDeserialize::deserialize_reader(r)?) } else { None },
        })
    }
}
impl crate::palw_native_resource_v1::NativeWireScanV1 for OriginalArchiveReceiptV1 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        r.step()?;
        // Preserve the old wire_struct root + u16 field CPU-step accounting.
        r.step()?;
        let version = r.short()?;
        if !matches!(version, 1 | 2) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "archive receipt scan version"));
        }
        let mut shape = crate::palw_native_resource_v1::NativeWireShapeV1::default();
        shape = shape.merge(<Hash64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<Hash64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<Hash64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<Hash64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<Hash64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<Hash64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<Hash64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<Hash64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<PalwBlockContextV2 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<[u64; 21] as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<u64 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<[Hash64; 4] as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<[Hash64; 4] as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape=shape.merge(<crate::palw_court_original_escrow_v1::FundingTermsV1 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(
            <Vec<NativeAncestorArchiveDescriptorV19> as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?,
        )?;
        shape = shape.merge(<PaidKnownBodyAliasesV19 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<bool as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        shape = shape.merge(<Option<u64> as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        if version == 2 {
            shape = shape
                .merge(<NativeOwner18FpEnvelopeV1 as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r)?)?;
        }
        Ok(shape)
    }
}
impl OriginalArchiveReceiptV1 {
    pub(crate) fn profile_kind_v2(&self) -> OriginalArchiveProfileKindV2 {
        if self.version == 2 { OriginalArchiveProfileKindV2::MixedV2 } else { OriginalArchiveProfileKindV2::MaterialV1 }
    }
    pub(crate) fn source_v1(&self) -> Hash64 {
        self.source
    }
    pub(crate) fn claim_v1(&self) -> Hash64 {
        self.claim
    }
    pub(crate) fn allocation_v1(&self) -> Hash64 {
        self.allocation
    }
    pub(crate) fn resource_plan_v1(&self) -> Hash64 {
        self.resource_plan
    }
    pub(crate) fn accepting_point_v1(&self) -> PalwBlockContextV2 {
        self.point
    }
    pub(crate) fn body_bounds_v1(&self) -> &[u64; 21] {
        &self.bodies
    }
    pub(crate) fn input_capacity_v1(&self) -> u64 {
        self.input_capacity
    }
    pub(crate) fn material_until_v1(&self) -> u64 {
        self.material_until
    }
    pub(crate) fn contract_v1(&self) -> Hash64 {
        self.contract
    }
    pub(crate) fn constructed_v1(&self) -> bool {
        self.constructed
    }
    pub(crate) fn released_v1(&self) -> bool {
        self.released_at.is_some()
    }
}
fn receipt_codec_upper_v1(
    ctx: &PalwBlockContextV2,
    t: &crate::palw_court_original_escrow_v1::FundingTermsV1,
    ancestors: &[NativeAncestorArchiveDescriptorV19],
    known_aliases: &PaidKnownBodyAliasesV19,
) -> Result<u64, FundedOriginalRefusalV1> {
    // Only a fixed-width schema calculation; no synthetic Claim/frame/birth.
    len(&(
        1u16,
        [Hash64::default(); 8],
        *ctx,
        [0u64; 3],
        [0u64; 21],
        [0u64; 13],
        [Hash64::default(); 4],
        [Hash64::default(); 4],
        t,
        ancestors,
        known_aliases,
        false,
        Some(0u64),
    ))
}
const PROFILE_TAGS: [[u8; 8]; 4] = [*b"OAR1FEE0", *b"OAR1CAP0", *b"OAR1RET0", *b"OAR1SET0"];
fn profile<T: borsh::BorshSerialize>(kind: usize, v: &T) -> Result<Hash64, FundedOriginalRefusalV1> {
    let digest = hash(kind as u8 + 1, v)?;
    let mut out = [0; 64];
    out.copy_from_slice(digest.as_byte_slice());
    out[..8].copy_from_slice(&PROFILE_TAGS[kind]);
    Ok(Hash64::from_bytes(out))
}
pub(crate) fn original_archive_terms_selected_v1(t: &crate::palw_court_original_escrow_v1::FundingTermsV1) -> bool {
    crate::palw_court_original_escrow_v1::original_archive_profile_selected_v1(t)
}
pub(crate) fn original_archive_terms_selected_v2(t: &crate::palw_court_original_escrow_v1::FundingTermsV1) -> bool {
    crate::palw_court_original_escrow_v1::original_archive_profile_kind_v2(t).is_some()
}
pub(crate) fn original_archive_profiles_selected_v1(extras: &PalwTransitionExtrasV1) -> bool {
    extras.tir_funded_originals.values().any(|p| original_archive_terms_selected_v1(&p.envelope().funding.terms))
}
pub(crate) fn original_archive_profiles_selected_v2(extras: &PalwTransitionExtrasV1) -> bool {
    extras.tir_funded_originals.values().any(|p| original_archive_terms_selected_v2(&p.envelope().funding.terms))
}

fn native_metadata_bounds_v1(
    req: &PreparedOriginalRequirementsV1,
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    bodies: &mut [u64; 21],
) -> Result<(), FundedOriginalRefusalV1> {
    let frame = req.frame_v1();
    let p = frame.plan_v1();
    let input = req.input_archive_footprint_v12().ok_or_else(|| bad("native archive control/input footprint absent"))?;
    if input.scope_v12() != (p.original_claim, p.source_id, p.allocation_id, p.resource_plan_id, *ctx) {
        return Err(bad("archive captured control/input scope differs"));
    }
    let pair = b.extras.tir_funded_originals.get(&p.original_claim).ok_or_else(|| bad("archive actual funded pair absent"))?;
    bodies[2] = super::native_fresh_fp_claim_codec_v11();
    bodies[7] = super::native_fresh_original_escrow_codec_v11();
    bodies[8] = 1 + 2 + 6 * 64 + 4 * 8;
    bodies[12] = input.control_bytes_v12();
    bodies[13] = add(4, len(pair.envelope())?)?;
    let semantics = req.archive_semantics_v10();
    bodies[14] = len(&semantics)?;
    let eof = semantics.and_then(|s| b.state.staged_tokenizer_inventory_v1(s.tokenizer_id));
    bodies[15] = len(&eof)?;
    bodies[16] = if let Some(table) = pair.semantic_table_v2() {
        add(1 + 64 + 68 + 64 + 1 + 4 + 8, add(len(&req.archive_rules_v10())?, table.len() as u64)?)?
    } else {
        1
    };
    let cover = crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1()
        .map_err(|_| bad("archive existing cover schedule missing"))?
        .cover;
    bodies[18] = len(&(cover.max_original_leaves, cover.max_total_edges, cover.max_total_coordinates, cover.max_root_memberships))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_detection_policy_v1::PalwDetectionPolicyV1>(
        crate::palw_claim_local_v1::PalwClaimLocalPolicyContextV1::detection_at_v1(b.params, ctx.daa_score).len() as u64,
    )
    .map_err(FundedOriginalRefusalV1::Resource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(FundedOriginalRefusalV1::Resource)?;
    bodies[10] = len(&crate::palw_claim_local_v1::PalwClaimLocalPolicyContextV1::from_rules_v1(b.params, ctx.daa_score))?;
    bodies[11] = add(4, len(b.params)?)?;
    Ok(())
}

use super::super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ContextErrorV2, RowValueV2};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
fn state_error(e: ContextErrorV2) -> PalwStateV2Error {
    super::super::palw_claim_local_paid_archive_v2::state_error(e)
}
fn context_refusal_v1(e: PalwStateV2Error, s: &'static str) -> ContextErrorV2 {
    match e {
        PalwStateV2Error::NativeResource(c) => ContextErrorV2::Resource(c),
        _ => ContextErrorV2::Malformed(s),
    }
}
fn refused(s: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.into())
}
fn clear_empty_owned_financial_root_v1(
    b: &mut TransitionBuilder<'_>,
    scope: Hash64,
    spec: ComponentSpecV2,
) -> Result<(), PalwStateV2Error> {
    let root = StateNodesV2(&b.state).owned_current(scope, spec).map_err(state_error)?;
    if root.rows != 0 || root.node.is_some() || spec.owner != 17 {
        return Err(refused("clearing nonempty or foreign financial owned root"));
    }
    let mut key = [0u8; 84];
    key[0] = b'I';
    key[1..3].copy_from_slice(&2u16.to_le_bytes());
    key[3..67].copy_from_slice(scope.as_byte_slice());
    borsh::BorshSerialize::serialize(&spec, &mut &mut key[67..]).map_err(|_| refused("financial owned root key codec"))?;
    b.write_kernel_row_encoded_v1::<ComponentRootV2>(PALW_CLAIM_LOCAL_TABLE_V1, &key, None)
}
fn receipt_spec() -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 17, table: 1, schema: 1, key_bytes: 64, value_bytes: u64::MAX }
}
fn body_value(raw: &[u8]) -> RowValueV2 {
    RowValueV2 { data: crate::palw_claim_local_v1::palw_liability_material_id_v1(raw), bytes: raw.len() as u64 }
}
fn decode_receipt(raw: &[u8]) -> Result<NativeRetainedV1<OriginalArchiveReceiptV1>, PalwStateV2Error> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c) => PalwStateV2Error::NativeResource(c),
        _ => refused("original archive financial receipt codec"),
    })
}
fn receipt_for_contract(
    state: &PalwChainStateV2,
    contract: Hash64,
) -> Result<Option<NativeRetainedV1<OriginalArchiveReceiptV1>>, PalwStateV2Error> {
    let reader = StateNodesV2(state);
    let root = reader.owned_current(contract, receipt_spec()).map_err(state_error)?;
    let Some(v) = nodes::lookup(&reader, root, contract.as_byte_slice()).map_err(state_error)? else { return Ok(None) };
    let raw = reader.body(v).map_err(state_error)?;
    decode_receipt(&raw).map(Some)
}
fn receipt_inventory_v1(root: ComponentRootV2, r: &OriginalArchiveReceiptV1) -> Result<store::ArchiveReferencesV2, PalwStateV2Error> {
    let active = if r.released_at.is_none() { r.ancestors.as_slice() } else { &[] };
    let component_count = active.iter().try_fold(1usize, |n, p| {
        n.checked_add(p.archive_references_v19().components.len()).ok_or(PalwStateV2Error::Overflow("ancestor component inventory"))
    })?;
    let literal_count = active.iter().try_fold(0usize, |n, p| {
        n.checked_add(p.archive_references_v19().literals.len()).ok_or(PalwStateV2Error::Overflow("ancestor literal inventory"))
    })?;
    let mut components = Vec::with_capacity(component_count);
    components.push(root);
    let mut literals = Vec::with_capacity(literal_count);
    for parent in active {
        let refs = parent.archive_references_v19();
        components.extend_from_slice(&refs.components);
        literals.extend_from_slice(&refs.literals);
    }
    Ok(store::ArchiveReferencesV2 { version: 2, components, literals })
}
fn write_receipt(b: &mut TransitionBuilder<'_>, r: &OriginalArchiveReceiptV1, remove: bool) -> Result<(), PalwStateV2Error> {
    let before = StateNodesV2(&b.state).owned_current(r.contract, receipt_spec()).map_err(state_error)?;
    let old = nodes::lookup(&StateNodesV2(&b.state), before, r.contract.as_byte_slice()).map_err(state_error)?;
    let bytes = borsh::object_length(r).map_err(|_| refused("archive receipt codec width"))?;
    let archive_width = financial_inventory_width_v1(&r.ancestors).map_err(FundedOriginalRefusalV1::into_state_v1)?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(
        (bytes as u64)
            .checked_add(archive_width.checked_mul(2).ok_or(PalwStateV2Error::Overflow("ancestor inventory buffers"))?)
            .ok_or(PalwStateV2Error::Overflow("ancestor inventory buffers"))?,
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    let components = r.ancestors.iter().try_fold(1u64, |n, p| {
        n.checked_add(p.archive_references_v19().components.len() as u64).ok_or(PalwStateV2Error::Overflow("ancestor component count"))
    })?;
    let literals = r.ancestors.iter().try_fold(0u64, |n, p| {
        n.checked_add(p.archive_references_v19().literals.len() as u64).ok_or(PalwStateV2Error::Overflow("ancestor literal count"))
    })?;
    q.exact_v1::<ComponentRootV2>(components.checked_mul(2).ok_or(PalwStateV2Error::Overflow("ancestor inventory roots"))?)
        .map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<RowValueV2>(literals.checked_mul(2).ok_or(PalwStateV2Error::Overflow("ancestor inventory literals"))?)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _owner = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut raw = Vec::with_capacity(bytes);
    borsh::BorshSerialize::serialize(r, &mut raw).map_err(|_| refused("archive receipt encode"))?;
    let plan = nodes::update(&StateNodesV2(&b.state), before, r.contract.as_byte_slice(), old, (!remove).then(|| body_value(&raw)))
        .map_err(state_error)?;
    let prior_receipt = old
        .map(|value| {
            let raw = StateNodesV2(&b.state).body(value).map_err(state_error)?;
            decode_receipt(&raw)
        })
        .transpose()?;
    let prior_archive = prior_receipt.as_ref().map(|prior| receipt_inventory_v1(before, prior)).transpose()?;
    // B/current BEFORE new pin; old pin survives until replacement is pinned.
    store::publish_owned_component_v2(b, r.contract, &plan, (!remove).then_some(raw.as_slice())).map_err(state_error)?;
    if !remove {
        let inventory = receipt_inventory_v1(plan.after(), r)?;
        let id = crate::palw_claim_local_v1::palw_liability_material_id_v1(
            &borsh::to_vec(&inventory).map_err(|_| refused("archive receipt A2 encode"))?,
        );
        let mut key = [0; 65];
        key[0] = b'A';
        key[1..].copy_from_slice(id.as_byte_slice());
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&inventory))?;
    }
    if let Some(inventory) = prior_archive
        && (remove || before != plan.after())
    {
        let id = crate::palw_claim_local_v1::palw_liability_material_id_v1(
            &borsh::to_vec(&inventory).map_err(|_| refused("archive prior receipt A2 encode"))?,
        );
        let mut key = [0; 65];
        key[0] = b'A';
        key[1..].copy_from_slice(id.as_byte_slice());
        b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(PALW_CLAIM_LOCAL_TABLE_V1, &key, None)?;
    }
    if remove {
        clear_empty_owned_financial_root_v1(b, r.contract, receipt_spec())?;
    }
    Ok(())
}
fn receipt_profiles(r: &OriginalArchiveReceiptV1) -> Result<[Hash64; 4], FundedOriginalRefusalV1> {
    receipt_profiles_geometry_v3(r, receipt_uses_literal_alias_geometry_v3(r)?)
}
fn receipt_profiles_geometry_v3(r: &OriginalArchiveReceiptV1, dedup: bool) -> Result<[Hash64; 4], FundedOriginalRefusalV1> {
    let terms = (
        1u16,
        r.base_profiles,
        (r.source, r.relative_graph, r.material_until, r.source_min_material_until),
        (r.bodies, r.consumer_facts_bytes, r.ancestors.as_slice(), r.known_aliases),
        (r.packets, r.base_rent, r.base_required),
        (r.reserved_rent, r.capital_ceiling),
        (r.row_price, r.stream_price, r.quantum, r.capital_per_packet),
    );
    if let Some(e) = r.owner18.as_ref() {
        if dedup {
            Ok([
                profile_v2(0, &(2u16, &terms, e, OAR2_LITERAL_ALIAS_GEOMETRY_DOMAIN_V3))?,
                profile_v2(1, &(2u16, &terms, e, OAR2_LITERAL_ALIAS_GEOMETRY_DOMAIN_V3))?,
                profile_v2(2, &(2u16, &terms, e, OAR2_LITERAL_ALIAS_GEOMETRY_DOMAIN_V3))?,
                profile_v2(3, &(2u16, &terms, e, OAR2_LITERAL_ALIAS_GEOMETRY_DOMAIN_V3))?,
            ])
        } else {
            Ok([
                profile_v2(0, &(2u16, &terms, e))?,
                profile_v2(1, &(2u16, &terms, e))?,
                profile_v2(2, &(2u16, &terms, e))?,
                profile_v2(3, &(2u16, &terms, e))?,
            ])
        }
    } else {
        Ok([profile(0, &terms)?, profile(1, &terms)?, profile(2, &terms)?, profile(3, &terms)?])
    }
}
fn check_receipt_against_row_v1(
    row: &crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1,
    r: &OriginalArchiveReceiptV1,
) -> Result<(), PalwStateV2Error> {
    row.check_v1().map_err(|_| refused("archive original escrow snapshot invalid"))?;
    if r.ancestors.iter().any(|p| p.point_v19().blue_score >= r.point.blue_score || p.point_v19().daa_score > r.point.daa_score) {
        return Err(refused("ancestor is not causally prior to paid consumer"));
    }
    if r.ancestors.windows(2).any(|w| compare_parent_v1(&w[0], &w[1]) != std::cmp::Ordering::Less) {
        return Err(refused("ancestor descriptors not canonical unique source order"));
    }
    // Cold receipt authority rederives the same owner-private physical geometry;
    // a profile-shaped hash by itself cannot turn an arbitrary holding value
    // or cheap body declaration into paid retained capacity.
    let root = ComponentRootV2 {
        spec: ComponentSpecV2 { owner: 16, table: 2, schema: 10, key_bytes: 64, value_bytes: u64::MAX },
        node: Some(r.claim),
        rows: 21,
    };
    let manifest =
        len(&(16u16, r.claim, r.point, r.claim, r.claim, root, 0u64, 0u64, false)).map_err(FundedOriginalRefusalV1::into_state_v1)?;
    let reverse = len(&(r.claim, r.point)).map_err(FundedOriginalRefusalV1::into_state_v1)?;
    let receipt = add(
        receipt_codec_upper_v1(&r.point, &r.funding_terms, &r.ancestors, &r.known_aliases)
            .map_err(FundedOriginalRefusalV1::into_state_v1)?,
        r.owner18.as_ref().map(len).transpose().map_err(FundedOriginalRefusalV1::into_state_v1)?.unwrap_or(0),
    )
    .map_err(FundedOriginalRefusalV1::into_state_v1)?;
    let geometry = lifetime_composed_geometry_v3(
        receipt_uses_literal_alias_geometry_v3(r).map_err(FundedOriginalRefusalV1::into_state_v1)?,
        &r.bodies,
        manifest,
        reverse,
        receipt,
        &r.point,
        r.consumer_facts_bytes,
        &r.ancestors,
        &r.known_aliases,
        r.material_until,
        r.owner18.as_ref(),
    )
    .map_err(FundedOriginalRefusalV1::into_state_v1)?;
    let minimum_rent = add(
        mul(geometry.retained, r.row_price).map_err(FundedOriginalRefusalV1::into_state_v1)?,
        mul(geometry.journal, r.stream_price).map_err(FundedOriginalRefusalV1::into_state_v1)?,
    )
    .map_err(FundedOriginalRefusalV1::into_state_v1)?;
    if geometry.holding != r.holding || minimum_rent > r.reserved_rent || r.capital > r.capital_ceiling {
        return Err(refused("archive receipt canonical geometry exceeds signed reservation"));
    }
    if !matches!((r.version, r.owner18.is_some()), (1, false) | (2, true))
        || crate::palw_court_original_escrow_v1::original_archive_profile_kind_v2(&r.funding_terms)
            != Some(if r.version == 1 { OriginalArchiveProfileKindV2::MaterialV1 } else { OriginalArchiveProfileKindV2::MixedV2 })
        || row.version != crate::palw_funded_escrow_v1::PALW_FUNDED_ESCROW_ORIGINAL_ARCHIVE_ROW_VERSION_V1
        || row.contract_id != r.contract
        || row.claim != r.claim
        || row.accepted_daa != r.point.daa_score
        || r.hard != row.hard_deadline_daa
        || row.amount_sompi < r.base_required.checked_add(r.capital).ok_or(PalwStateV2Error::Overflow("archive funding total"))?
        || r.input_capacity != r.bodies[9]
        || r.capital != r.packets.checked_mul(r.capital_per_packet).ok_or(PalwStateV2Error::Overflow("archive capital"))?
        || r.packets != r.holding.div_ceil(r.quantum.max(1))
        || r.quantum == 0
        || r.material_until < r.hard
        || r.material_until < r.source_min_material_until
        || r.funding_terms.id_v1() != r.contract
        || r.funding_terms.claim != r.claim
        || r.funding_terms.source_id != r.source
        || r.funding_terms.funding_amount_sompi != row.amount_sompi
        || r.funding_terms.payer_key_id != row.payer_key_id
        || r.funding_terms.refund_payload != row.refund_payload
        || r.funding_terms.material_until_daa != r.material_until
        || [
            r.funding_terms.fee_profile_id,
            r.funding_terms.capital_profile_id,
            r.funding_terms.retention_profile_id,
            r.funding_terms.settlement_policy_id,
        ] != r.profiles
        || receipt_profiles(r).map_err(FundedOriginalRefusalV1::into_state_v1)? != r.profiles
    {
        return Err(refused("archive receipt native scope/profile/capital mismatch"));
    }
    Ok(())
}
fn check_receipt(state: &PalwChainStateV2, r: &OriginalArchiveReceiptV1) -> Result<(), PalwStateV2Error> {
    let row = state.funded_escrows_v1().get(&r.contract).ok_or_else(|| refused("archive receipt actual original escrow absent"))?;
    check_receipt_against_row_v1(row, r)
}
/// Fixed-width identity proof of a previously verified native paid interval.
/// Its exact financial component and funding evidence are pinned by the child;
/// this object never fabricates an escrow, profile or payer authorization.
#[derive(Clone, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct NativeOriginalPaidArchiveProofV19 {
    claim: Hash64,
    point: PalwBlockContextV2,
    contract: Hash64,
    material_until: u64,
    financial_root: ComponentRootV2,
    evidence: [RowValueV2; 2],
}
crate::palw_native_resource_v1::wire::wire_struct!(NativeOriginalPaidArchiveProofV19;Hash64,PalwBlockContextV2,Hash64,u64,ComponentRootV2,[RowValueV2;2]);
impl NativeOriginalPaidArchiveProofV19 {
    pub(crate) fn material_until_v19(&self) -> u64 {
        self.material_until
    }
    pub(crate) fn financial_root_v19(&self) -> ComponentRootV2 {
        self.financial_root
    }
    pub(crate) fn evidence_literals_v19(&self) -> &[RowValueV2; 2] {
        &self.evidence
    }
    pub(crate) fn recheck_archived_v19(&self, state: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
        let row = super::super::palw_claim_local_born_tir_source_v16::verify_original_funding_evidence_v19(
            state,
            self.claim,
            self.point,
            self.evidence[0],
            self.evidence[1],
        )?;
        if self.financial_root.spec != receipt_spec() || self.financial_root.rows != 1 || row.contract_id != self.contract {
            return Err(refused("ancestor old-paid exact financial proof root"));
        }
        let reader = StateNodesV2(state);
        let value = nodes::lookup(&reader, self.financial_root, self.contract.as_byte_slice())
            .map_err(state_error)?
            .ok_or_else(|| refused("ancestor old-paid archived financial receipt absent"))?;
        let raw = reader.body(value).map_err(state_error)?;
        let r = decode_receipt(&raw)?;
        check_receipt_against_row_v1(&row, &r)?;
        if !r.constructed
            || r.released_at.is_some()
            || r.claim != self.claim
            || r.point != self.point
            || r.contract != self.contract
            || r.material_until != self.material_until
        {
            return Err(refused("ancestor immutable old-paid scope differs"));
        }
        Ok(())
    }
}
pub(crate) fn capture_original_paid_archive_proof_v19(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
) -> Result<Option<NativeOriginalPaidArchiveProofV19>, PalwStateV2Error> {
    let Some(r) = verified_original_archive_reservation_v1(state, claim, point)? else { return Ok(None) };
    if !r.constructed || r.released_at.is_some() {
        return Ok(None);
    }
    let Some((escrow, envelope)) =
        super::super::palw_claim_local_born_tir_source_v16::original_funding_evidence_refs_v19(state, claim, point)?
    else {
        return Err(refused("native old-paid scope lacks actual Source16 funding evidence"));
    };
    let proof = NativeOriginalPaidArchiveProofV19 {
        claim,
        point,
        contract: r.contract,
        material_until: r.material_until,
        financial_root: StateNodesV2(state).owned_current(r.contract, receipt_spec()).map_err(state_error)?,
        evidence: [escrow, envelope],
    };
    proof.recheck_archived_v19(state)?;
    Ok(Some(proof))
}
pub(crate) fn verified_original_archive_reservation_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
) -> Result<Option<NativeRetainedV1<OriginalArchiveReceiptV1>>, PalwStateV2Error> {
    let Some(contract) = contract_for_birth_v1(state, claim, point)? else { return Ok(None) };
    let row =
        state.funded_escrows_v1().get(&contract).ok_or_else(|| refused("new original archive birth index lacks native escrow"))?;
    if row.version != crate::palw_funded_escrow_v1::PALW_FUNDED_ESCROW_ORIGINAL_ARCHIVE_ROW_VERSION_V1
        || row.claim != claim
        || row.accepted_daa != point.daa_score
    {
        return Err(refused("archive birth index changes original escrow"));
    }
    let r = receipt_for_contract(state, row.contract_id)?
        .ok_or_else(|| refused("new original archive escrow lacks native financial receipt"))?;
    check_receipt(state, &r)?;
    if r.claim != claim || r.point != point {
        return Err(refused("archive financial receipt actual birth differs"));
    }
    Ok(Some(r))
}
pub(crate) fn archive_capital_pending_v1(
    state: &PalwChainStateV2,
    row: &crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1,
) -> Result<bool, PalwStateV2Error> {
    if row.version != crate::palw_funded_escrow_v1::PALW_FUNDED_ESCROW_ORIGINAL_ARCHIVE_ROW_VERSION_V1 {
        return Ok(false);
    }
    let r = receipt_for_contract(state, row.contract_id)?.ok_or_else(|| refused("archive capital receipt missing"))?;
    check_receipt(state, &r)?;
    Ok(!r.constructed || r.released_at.is_none())
}
/// Targeted retry from the existing actual escrow clock owner. It creates no
/// additional whole-world/E pass and never substitutes time for native closure.
pub(crate) fn retry_original_archive_ownership_v19(
    b: &mut TransitionBuilder<'_>,
    row: &crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    if row.version != crate::palw_funded_escrow_v1::PALW_FUNDED_ESCROW_ORIGINAL_ARCHIVE_ROW_VERSION_V1 {
        return Ok(());
    }
    let r = receipt_for_contract(&b.state, row.contract_id)?.ok_or_else(|| refused("archive ownership retry lacks receipt"))?;
    check_receipt(&b.state, &r)?;
    if !r.constructed || r.released_at.is_some() || now < r.material_until {
        return Ok(());
    }
    if b.claim_local_lock_point_v6.is_none_or(|p| p.daa_score != now) {
        return Err(refused("archive ownership retry lacks actual native clock"));
    }
    super::super::palw_claim_local_born_tir_source_v16::on_native_archive_ownership_changed_v19(b, r.claim, r.point)
}

pub(crate) fn mark_original_archive_constructed_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
) -> Result<(), PalwStateV2Error> {
    let Some(stored) = verified_original_archive_reservation_v1(&b.state, claim, *ctx)? else {
        return Err(refused("constructing archive without native funding receipt"));
    };
    let _edit = receipt_edit_lease_v1(&stored)?;
    let mut r = (*stored).clone();
    if r.released_at.is_some() {
        return Err(refused("released original archive cannot be reconstructed"));
    }
    super::super::palw_claim_local_born_tir_source_v16::verify_original_archive_body_bounds_v1(&b.state, claim, *ctx, &r.bodies)?;
    recheck_published_known_aliases_v19(&b.state, &r)?;
    let origin = super::super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&b.state, claim, *ctx)?
        .ok_or_else(|| refused("archive complete lacks actual Source8 admission"))?;
    if len(&origin.inputs()).map_err(FundedOriginalRefusalV1::into_state_v1)? > r.input_capacity {
        return Err(refused("archive actual input body exceeds paid reservation"));
    }
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state, claim)
        .ok_or_else(|| refused("archive complete actual frame absent"))?;
    if (frame.plan_v1().source_id, frame.plan_v1().allocation_id, frame.plan_v1().resource_plan_id)
        != (r.source, r.allocation, r.resource_plan)
    {
        return Err(refused("archive constructed native source changed"));
    }
    if let Some(e) = r.owner18.as_ref() {
        super::super::palw_claim_local_legacy_terminal_v18::verify_fp_owner18_envelope_in_fold_v1(b, ctx, claim, r.point, e)?;
    }
    if !r.constructed {
        r.constructed = true;
        write_receipt(b, &r, false)?;
    }
    Ok(())
}
pub(crate) fn can_release_original_archive_reservation_v19(
    b: &TransitionBuilder<'_>,
    old: &Frame,
    now: u64,
) -> Result<bool, PalwStateV2Error> {
    let claim = old.plan_v1().original_claim;
    let key = hash(8, &(claim, old.plan_v1().allocation_id)).map_err(FundedOriginalRefusalV1::into_state_v1)?;
    let Some(contract) = contract_for_index_key_v1(&b.state, key)? else { return Ok(true) };
    let r = receipt_for_contract(&b.state, contract)?.ok_or_else(|| refused("archive release preflight lost native receipt"))?;
    check_receipt(&b.state, &r)?;
    if (old.plan_v1().source_id, old.plan_v1().allocation_id, old.plan_v1().resource_plan_id)
        != (r.source, r.allocation, r.resource_plan)
        || !r.constructed
    {
        return Err(refused("archive release preflight changes native scope"));
    }
    if r.released_at.is_some() {
        return Ok(true);
    }
    if now < r.material_until {
        return Ok(false);
    }
    for parent in &r.ancestors {
        let own = verified_original_archive_reservation_v1(&b.state, parent.claim_v19(), parent.point_v19())?;
        if own.as_ref().is_some_and(|p| p.constructed && p.released_at.is_none()) {
            continue;
        }
        // Explicitly exclude this child: an unreleased lease may legitimately
        // own existing material past minimumH until its actual release.
        let point = parent.point_v19();
        let other = with_verified_ancestor_descriptor_excluding_v19(
            &b.state,
            parent.claim_v19(),
            point.block,
            point.daa_score,
            point.blue_score,
            now,
            Some(r.contract),
            None,
            |p| {
                if p.point_v19() != point {
                    return Err(refused("ancestor successor changes full native point"));
                }
                Ok(())
            },
        )?;
        if other.is_some() {
            continue;
        }
        if !super::super::palw_original_archive_ancestors_v19::native_source_archive_duties_closed_v19(
            &b.state,
            b.params,
            parent.claim_v19(),
            parent.point_v19(),
            now,
        )? {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(crate) fn release_original_archive_reservation_v1(b: &mut TransitionBuilder<'_>, old: &Frame) -> Result<(), PalwStateV2Error> {
    let claim = old.plan_v1().original_claim;
    let key = hash(8, &(claim, old.plan_v1().allocation_id)).map_err(FundedOriginalRefusalV1::into_state_v1)?;
    let Some(contract) = contract_for_index_key_v1(&b.state, key)? else { return Ok(()) };
    let stored =
        receipt_for_contract(&b.state, contract)?.ok_or_else(|| refused("archive allocation index lost financial receipt"))?;
    check_receipt(&b.state, &stored)?;
    let _edit = receipt_edit_lease_v1(&stored)?;
    let mut r = (*stored).clone();
    if (old.plan_v1().source_id, old.plan_v1().allocation_id, old.plan_v1().resource_plan_id)
        != (r.source, r.allocation, r.resource_plan)
        || !r.constructed
    {
        return Err(refused("archive release changes native owned source"));
    }
    let point = b.claim_local_lock_point_v6.ok_or_else(|| refused("archive release lacks actual native point"))?;
    if point.daa_score < r.material_until {
        return Err(refused("archive retention still inside original signed material horizon"));
    }
    if !can_release_original_archive_reservation_v19(b, old, point.daa_score)? {
        return Err(refused("ancestor retention lacks successor paid coverage or actual native closure"));
    }
    let transferred = matches!(
        archive_retention_disposition_v1(&b.state, r.claim, r.point, point.daa_score)?,
        ArchiveRetentionDispositionV1::ConsumerOwned
    );
    if !transferred
        && super::super::palw_claim_local_born_tir_source_v16::original_provenance_v16(&b.state, r.claim, r.point)?.is_some()
    {
        return Err(refused("original Source16 material has not been collected or transferred to a funded consumer"));
    }
    // Source8/Carrier9 eligibility ledger rows have already paid their one-time
    // canonical rent. Their bounded native waiting state is not an unexpired
    // all21 TIR serving duty and does not alone hold this material reservation.

    if r.released_at.is_none() {
        write_ancestor_indices_v1(b, &r, true)?;
        for parent in &r.ancestors {
            super::super::palw_original_archive_ancestors_v19::collect_ancestor_archive_after_lease_v19(b, parent)?;
        }
        r.released_at = Some(point.daa_score);
        write_receipt(b, &r, false)?;
    }
    Ok(())
}
pub(crate) fn collect_settled_original_archive_receipt_v1(
    b: &mut TransitionBuilder<'_>,
    contract: Hash64,
) -> Result<(), PalwStateV2Error> {
    let Some(row) = b.state.funded_escrows_v1().get(&contract).copied() else { return Ok(()) };
    if row.version != crate::palw_funded_escrow_v1::PALW_FUNDED_ESCROW_ORIGINAL_ARCHIVE_ROW_VERSION_V1
        || row.settled_by.is_none()
        || row.liability_open
    {
        return Ok(());
    }
    let Some(r) = receipt_for_contract(&b.state, contract)? else { return Ok(()) };
    check_receipt(&b.state, &r)?;
    if !r.constructed || r.released_at.is_none() {
        return Err(refused("settled archive receipt still owns retention"));
    }
    write_receipt(b, &r, true)?;
    write_birth_index_v1(b, &r, true)
}

fn receipt_from_plan(
    q: &NativeOriginalArchiveReservationV1,
    contract: Hash64,
    t: &crate::palw_court_original_escrow_v1::FundingTermsV1,
) -> OriginalArchiveReceiptV1 {
    OriginalArchiveReceiptV1 {
        version: if q.owner18.is_some() { 2 } else { 1 },
        claim: q.scope.0,
        source: q.scope.1,
        allocation: q.scope.2,
        resource_plan: q.scope.3,
        rules: q.scope.4,
        relative_graph: q.scope.5,
        absolute_graph: q.scope.6,
        contract,
        point: q.point,
        hard: q.hard,
        material_until: q.material_until,
        source_min_material_until: q.source_min_material_until,
        bodies: q.bodies,
        input_capacity: q.input_capacity,
        consumer_facts_bytes: q.consumer_facts_bytes,
        holding: q.geometry.holding,
        packets: q.packets,
        quantum: q.quantum,
        capital: q.capital,
        base_rent: q.base_rent,
        reserved_rent: q.reserved_rent,
        base_required: q.base_required,
        capital_ceiling: q.capital_ceiling,
        row_price: q.row_price,
        stream_price: q.stream_price,
        capital_per_packet: q.capital_per_packet,
        base_profiles: q.base_profiles,
        profiles: q.profiles,
        funding_terms: t.clone(),
        ancestors: q.ancestors.to_vec(),
        known_aliases: q.known_aliases,
        constructed: false,
        released_at: None,
        owner18: q.owner18,
    }
}
/// Installer: called after the real FP/frame/escrow/native book writes, inside
/// their existing original object checkpoint. It consumes no other principal.
pub(super) fn install_original_archive_receipt_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    q: &NativeOriginalArchiveReservationV1,
    contract: Hash64,
) -> Result<(), PalwStateV2Error> {
    if *ctx != q.point {
        return Err(refused("archive reservation install changes actual point"));
    }
    q.check_actual_inputs_v1().map_err(FundedOriginalRefusalV1::into_state_v1)?;
    if receipt_for_contract(&b.state, contract)?.is_some() {
        return Err(refused("original archive funding installs once"));
    }
    let pair = b.extras.tir_funded_originals.get(&q.scope.0).ok_or_else(|| refused("archive actual carrier pairing missing"))?;
    if pair.envelope().funding.terms.id_v1() != contract {
        return Err(refused("archive receipt substitutes original funding contract"));
    }
    let _clone = ancestor_clone_lease_v1(&q.ancestors)?;
    let r = receipt_from_plan(q, contract, &pair.envelope().funding.terms);
    check_receipt(&b.state, &r)?;
    let service = b.state.court_service_state_v1().ok_or_else(|| refused("archive actual reserved final book absent"))?;
    let binding = service.claims.get(&q.scope.0).ok_or_else(|| refused("archive actual binding absent"))?;
    for ordinal in 0..q.packets {
        let packet = q.packet_v1(ordinal).map_err(FundedOriginalRefusalV1::into_state_v1)?;
        if !binding.packet_ids.contains(&packet.id) || service.book.packet(&packet.id) != Some(&packet) {
            return Err(refused("archive native holding capacity not installed"));
        }
    }
    for parent in &r.ancestors {
        parent.recheck_native_v19(&b.state)?;
    }
    super::super::palw_original_archive_ancestors_v19::publish_native_ancestor_artifacts_v19(b, &r.ancestors)?;
    write_receipt(b, &r, false)?;
    write_birth_index_v1(b, &r, false)?;
    write_ancestor_indices_v1(b, &r, false)
}
/// Same existing service book and same capacity, with explicit native preflight
/// of its local clone/scratch. No world/context clone or new actor is created.
pub(super) fn reserve_original_archive_capacity_v1(
    complete: &mut crate::palw_court_service_state_v2::CourtServiceStateV2,
    q: &NativeOriginalArchiveReservationV1,
) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, FundedOriginalRefusalV1> {
    let mut requests = TirNativeRequestsV1::new();
    requests
        .add_v1(crate::palw_native_resource_v1::native_typed_value_requests_v1(complete).map_err(FundedOriginalRefusalV1::Resource)?)
        .map_err(FundedOriginalRefusalV1::Resource)?;
    requests.exact_v1::<ServicePacket>(q.packets).map_err(FundedOriginalRefusalV1::Resource)?;
    requests.exact_v1::<&ServicePacket>(q.packets).map_err(FundedOriginalRefusalV1::Resource)?;
    requests.exact_v1::<BTreeNodeEnvelopeV1<[u8; 64], ServicePacket>>(q.packets).map_err(FundedOriginalRefusalV1::Resource)?;
    requests.exact_v1::<BTreeNodeEnvelopeV1<[u8; 64], u64>>(q.packets).map_err(FundedOriginalRefusalV1::Resource)?;
    requests
        .exact_v1::<BTreeNodeEnvelopeV1<[u8; 64], ()>>(q.packets.checked_mul(2).ok_or_else(overflow)?)
        .map_err(FundedOriginalRefusalV1::Resource)?;
    requests.exact_v1::<BTreeNodeEnvelopeV1<u64, ServiceCost>>(q.packets).map_err(FundedOriginalRefusalV1::Resource)?;
    let lease = reserve_native_requests_v1(requests.bytes_v1()).map_err(FundedOriginalRefusalV1::Resource)?;
    let mut packets = Vec::with_capacity(usize::try_from(q.packets).map_err(|_| overflow())?);
    for ordinal in 0..q.packets {
        packets.push(q.packet_v1(ordinal)?);
    }
    complete.book.reserve(&packets, q.hard).map_err(|_| bad("archive whole native capacity does not fit existing service budget"))?;
    let binding = complete.claims.get_mut(&q.scope.0).ok_or_else(|| bad("archive original binding missing"))?;
    binding.packet_ids.extend(packets.iter().map(|p| p.id));
    complete.check_live_v1().map_err(|_| bad("archive final native book inconsistent"))?;
    Ok(lease)
}

pub(super) fn prepare_archive_finance_at_native_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    requirements: &PreparedOriginalRequirementsV1,
    family: &[super::super::palw_court_family_supplier_fold_v1::FamilySupplierPlanV1],
    derivation: &crate::palw_funded_original_admission_v1::FundedAdmissionDerivationV1,
    complete: &crate::palw_court_service_state_v2::CourtServiceStateV2,
) -> Result<NativeRetainedV1<NativeOriginalArchiveReservationV1>, FundedOriginalRefusalV1> {
    let claim = requirements.frame_v1().plan_v1().original_claim;
    let pairing = b.extras.tir_funded_originals.get(&claim).ok_or_else(|| bad("archive actual paired funding missing"))?;
    let t = &pairing.envelope().funding.terms;
    if !original_archive_terms_selected_v2(t) {
        return Err(bad("archive economic profile version not selected"));
    }
    let schedule = palw_funded_original_schedule_v1(b.extras).map_err(FundedOriginalRefusalV1::Missing)?;
    let p = requirements.frame_v1().plan_v1();
    let base = crate::palw_funded_original_admission_v1::funded_required_amount_v1(
        schedule.fee_sompi(),
        schedule.capital_floor_sompi(),
        derivation.capital_sompi,
        p.terms.plan.deposit_sompi,
    )
    .ok_or_else(overflow)?;
    let literal = b.extras.tir_public_originals.get(&claim).ok_or_else(|| bad("archive actual TFP literal missing"))?;
    let ceilings = OriginalArchivePayerCeilingsV1 {
        additional_carrier_rent: literal
            .declared_rent_sompi
            .checked_sub(p.terms.plan.rent_sompi)
            .ok_or_else(|| bad("archive rent ceiling below base original rent"))?,
        dedicated_escrow_capital: t
            .funding_amount_sompi
            .checked_sub(base)
            .ok_or_else(|| bad("archive actual funding below original base requirement"))?,
    };
    let view = archive_service_view_v11::PreparedOriginalArchiveServiceViewV11::from_native_calendar_v11(
        requirements,
        b,
        ctx,
        family,
        derivation,
        complete,
    )?;
    let mixed =
        crate::palw_court_original_escrow_v1::original_archive_profile_kind_v2(t) == Some(OriginalArchiveProfileKindV2::MixedV2);
    let mut q = quote_internal_v1(b, ctx, &view, None, Some(ceilings), mixed)?;
    let signed = [t.fee_profile_id, t.capital_profile_id, t.retention_profile_id, t.settlement_policy_id];
    if mixed && q.profiles_v1() != signed {
        // Old OAR2 signatures retain the old tuple AND admission/replay law.
        // Drop native owners before independent bounded legacy rederivation.
        drop(q);
        q = quote_internal_geometry_v3(b, ctx, &view, None, Some(ceilings), true, false)?;
    }
    q.check_actual_inputs_v1()?;
    if t.material_until_daa != q.material_until {
        return Err(bad("signed archive horizon is below actual known source duties"));
    }
    let actual = [t.fee_profile_id, t.capital_profile_id, t.retention_profile_id, t.settlement_policy_id];
    if actual != q.profiles || literal.declared_rent_sompi != q.total_declared_rent_v1()? {
        return Err(bad("signed archive profiles/rent differ from complete native reservation"));
    }
    Ok(q)
}

fn birth_key_v1(claim: Hash64, point: PalwBlockContextV2) -> Result<Hash64, PalwStateV2Error> {
    hash(7, &(claim, point)).map_err(FundedOriginalRefusalV1::into_state_v1)
}
fn ancestor_index_spec_v1() -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 17, table: 3, schema: 1, key_bytes: 64, value_bytes: u64::MAX }
}
fn ancestor_index_width_v1(parent: &NativeAncestorArchiveDescriptorV19) -> Result<u64, FundedOriginalRefusalV1> {
    len(&(parent.claim_v19(), parent.point_v19(), Hash64::default(), 0u64))
}
fn compare_parent_v1(a: &NativeAncestorArchiveDescriptorV19, b: &NativeAncestorArchiveDescriptorV19) -> std::cmp::Ordering {
    let ap = a.point_v19();
    let bp = b.point_v19();
    (a.claim_v19(), ap.daa_score, ap.blue_score, ap.block, ap.subsidy).cmp(&(
        b.claim_v19(),
        bp.daa_score,
        bp.blue_score,
        bp.block,
        bp.subsidy,
    ))
}
fn find_parent_v1(
    r: &OriginalArchiveReceiptV1,
    claim: Hash64,
    point: PalwBlockContextV2,
) -> Option<&NativeAncestorArchiveDescriptorV19> {
    let target = (claim, point.daa_score, point.blue_score, point.block, point.subsidy);
    r.ancestors
        .binary_search_by(|p| {
            let at = p.point_v19();
            (p.claim_v19(), at.daa_score, at.blue_score, at.block, at.subsidy).cmp(&target)
        })
        .ok()
        .map(|at| &r.ancestors[at])
}
fn ancestor_scope_v1(claim: Hash64, point: PalwBlockContextV2) -> Result<Hash64, PalwStateV2Error> {
    hash(9, &(claim, point.block, point.daa_score, point.blue_score)).map_err(FundedOriginalRefusalV1::into_state_v1)
}
fn ancestor_digest_v1(parent: &NativeAncestorArchiveDescriptorV19) -> Result<Hash64, PalwStateV2Error> {
    hash(10, parent).map_err(FundedOriginalRefusalV1::into_state_v1)
}
fn write_ancestor_indices_v1(
    b: &mut TransitionBuilder<'_>,
    r: &OriginalArchiveReceiptV1,
    remove: bool,
) -> Result<(), PalwStateV2Error> {
    for parent in &r.ancestors {
        let scope = ancestor_scope_v1(parent.claim_v19(), parent.point_v19())?;
        let reader = StateNodesV2(&b.state);
        let before = reader.owned_current(scope, ancestor_index_spec_v1()).map_err(state_error)?;
        let prior = nodes::lookup(&reader, before, r.contract.as_byte_slice()).map_err(state_error)?;
        let value = (parent.claim_v19(), parent.point_v19(), ancestor_digest_v1(parent)?, r.material_until);
        let bytes = borsh::object_length(&value).map_err(|_| refused("ancestor lease index canonical width"))?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<u8>(bytes as u64).map_err(PalwStateV2Error::NativeResource)?;
        let _buffer = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        let mut raw = Vec::with_capacity(bytes);
        borsh::BorshSerialize::serialize(&value, &mut raw).map_err(|_| refused("ancestor lease index canonical codec"))?;
        if remove {
            let old = prior.ok_or_else(|| refused("funded ancestor lease vanished before release"))?;
            let actual = reader.body(old).map_err(state_error)?;
            if actual.as_slice() != raw.as_slice() {
                return Err(refused("ancestor lease index changes paid descriptor or horizon"));
            }
        } else if prior.is_some() {
            return Err(refused("ancestor lease installs once per consumer contract"));
        }
        let plan = nodes::update(&reader, before, r.contract.as_byte_slice(), prior, (!remove).then(|| body_value(&raw)))
            .map_err(state_error)?;
        store::publish_owned_component_v2(b, scope, &plan, (!remove).then_some(raw.as_slice())).map_err(state_error)?;
        if remove && plan.after().rows == 0 {
            clear_empty_owned_financial_root_v1(b, scope, ancestor_index_spec_v1())?;
        }
    }
    Ok(())
}
/// Coverage classification, never caller readiness or permission. The exact
/// parent-owned index is bounded by its real admitted consumer leases.
pub(crate) enum ArchiveRetentionDispositionV1 {
    Collect,
    ConsumerOwned,
}
/// Metadata liveness only, never proof of paid interval or permission to
/// extend one. Expired but unreleased child receipts still need their exact
/// discovery refs until their own atomic release removes these leaves.
pub(crate) fn has_ancestor_archive_references_v19(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
) -> Result<bool, PalwStateV2Error> {
    let root = StateNodesV2(state).owned_current(ancestor_scope_v1(claim, point)?, ancestor_index_spec_v1()).map_err(state_error)?;
    Ok(root.rows != 0)
}
pub(crate) fn archive_retention_disposition_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
    now: u64,
) -> Result<ArchiveRetentionDispositionV1, PalwStateV2Error> {
    let paid = with_verified_ancestor_descriptor_v19(state, claim, point.block, point.daa_score, point.blue_score, now, |parent| {
        if parent.point_v19() != point {
            return Err(refused("paid ancestor changes full native point"));
        }
        Ok(())
    })?;
    Ok(if paid.is_some() { ArchiveRetentionDispositionV1::ConsumerOwned } else { ArchiveRetentionDispositionV1::Collect })
}
/// Resolve a funded immutable artifact from accepted source identity without
/// guessing an unavailable subsidy. A full native point is recovered only from
/// an actual indexed, verified live consumer receipt. The borrowed descriptor
/// cannot escape the retained owner through this fixed-result callback.
pub(crate) fn with_verified_ancestor_descriptor_v19<R, F>(
    state: &PalwChainStateV2,
    claim: Hash64,
    block: BlockHash,
    daa: u64,
    blue: u64,
    now: u64,
    consume: F,
) -> Result<Option<R>, PalwStateV2Error>
where
    F: FnOnce(&NativeAncestorArchiveDescriptorV19) -> Result<R, PalwStateV2Error>,
{
    with_verified_ancestor_descriptor_excluding_v19(state, claim, block, daa, blue, now, None, None, consume)
}
/// Guaranteed interval for a NEW material use. Unlike retained-data lookup,
/// this never extends old capital credit merely because money remains held.
/// It grants no E/pledge/supplier permission; those clocks stay independently
/// checked by the actual native admission path.
pub(crate) fn guaranteed_original_archive_coverage_v19(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
    now: u64,
    needed_until: u64,
) -> Result<bool, PalwStateV2Error> {
    if let Some(own) = verified_original_archive_reservation_v1(state, claim, point)?
        && own.constructed
        && own.released_at.is_none()
        && now <= own.material_until
        && needed_until <= own.material_until
    {
        return Ok(true);
    }
    with_verified_ancestor_descriptor_excluding_v19(
        state,
        claim,
        point.block,
        point.daa_score,
        point.blue_score,
        now,
        None,
        Some(needed_until),
        |parent| {
            if parent.point_v19() != point {
                return Err(refused("guaranteed material coverage changes native point"));
            }
            Ok(())
        },
    )
    .map(|v| v.is_some())
}
fn with_verified_ancestor_descriptor_excluding_v19<R, F>(
    state: &PalwChainStateV2,
    claim: Hash64,
    block: BlockHash,
    daa: u64,
    blue: u64,
    now: u64,
    excluded: Option<Hash64>,
    required_until: Option<u64>,
    consume: F,
) -> Result<Option<R>, PalwStateV2Error>
where
    F: FnOnce(&NativeAncestorArchiveDescriptorV19) -> Result<R, PalwStateV2Error>,
{
    let scope = hash(9, &(claim, block, daa, blue)).map_err(FundedOriginalRefusalV1::into_state_v1)?;
    let reader = StateNodesV2(state);
    let root = reader.owned_current(scope, ancestor_index_spec_v1()).map_err(state_error)?;
    let mut chosen = None::<(Hash64, PalwBlockContextV2)>;
    let mut actual_point = None;
    nodes::visit_nodes(&reader, root, |_, node| {
        if let nodes::ContextNodeV2::Leaf { key, value } = node {
            let contract: Hash64 = borsh::from_slice(key).map_err(|_| ContextErrorV2::Malformed("ancestor lease contract key"))?;
            let raw = reader.body(*value)?;
            let (parent_claim, point, digest, until): (Hash64, PalwBlockContextV2, Hash64, u64) =
                borsh::from_slice(&raw).map_err(|_| ContextErrorV2::Malformed("ancestor lease index codec"))?;
            if (parent_claim, point.block, point.daa_score, point.blue_score) != (claim, block, daa, blue) {
                return Err(ContextErrorV2::Malformed("ancestor lease parent scope changed"));
            }
            if actual_point.is_some_and(|p| p != point) {
                return Err(ContextErrorV2::Malformed("ancestor lease contradicts full native birth point"));
            }
            actual_point = Some(point);
            let child = receipt_for_contract(state, contract)
                .map_err(|e| context_refusal_v1(e, "ancestor lease child receipt absent"))?
                .ok_or(ContextErrorV2::Malformed("ancestor lease lacks paid child receipt"))?;
            check_receipt(state, &child).map_err(|e| context_refusal_v1(e, "ancestor lease child financial scope"))?;
            let parent = find_parent_v1(&child, claim, point)
                .ok_or(ContextErrorV2::Malformed("ancestor index not covered by signed child descriptor"))?;
            if ancestor_digest_v1(parent).map_err(|_| ContextErrorV2::Malformed("ancestor digest codec"))? != digest
                || until != child.material_until
                || child.contract != contract
                || child.released_at.is_some()
            {
                return Err(ContextErrorV2::Malformed("ancestor index changes child horizon or lease"));
            }
            parent.recheck_native_v19(state).map_err(|e| context_refusal_v1(e, "ancestor immutable provenance changed"))?;
            if chosen.is_none()
                && excluded != Some(contract)
                && child.constructed
                && required_until.is_none_or(|need| now <= until && need <= until)
            {
                chosen = Some((contract, point));
            }
        }
        Ok(())
    })
    .map_err(state_error)?;
    let Some((contract, point)) = chosen else { return Ok(None) };
    let child = receipt_for_contract(state, contract)?.ok_or_else(|| refused("selected ancestor paid receipt vanished"))?;
    let parent = find_parent_v1(&child, claim, point).ok_or_else(|| refused("selected ancestor descriptor vanished"))?;
    consume(parent).map(Some)
}

fn index_spec() -> ComponentSpecV2 {
    ComponentSpecV2 { owner: 17, table: 2, schema: 1, key_bytes: 64, value_bytes: 64 }
}
fn contract_for_birth_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
) -> Result<Option<Hash64>, PalwStateV2Error> {
    contract_for_index_key_v1(state, birth_key_v1(claim, point)?)
}
fn contract_for_index_key_v1(state: &PalwChainStateV2, key: Hash64) -> Result<Option<Hash64>, PalwStateV2Error> {
    let reader = StateNodesV2(state);
    let root = reader.current(index_spec()).map_err(state_error)?;
    let Some(v) = nodes::lookup(&reader, root, key.as_byte_slice()).map_err(state_error)? else { return Ok(None) };
    let raw = reader.body(v).map_err(state_error)?;
    borsh::from_slice(&raw).map(Some).map_err(|_| refused("archive native lookup index codec"))
}
fn write_birth_index_v1(b: &mut TransitionBuilder<'_>, r: &OriginalArchiveReceiptV1, remove: bool) -> Result<(), PalwStateV2Error> {
    let keys = [birth_key_v1(r.claim, r.point)?, hash(8, &(r.claim, r.allocation)).map_err(FundedOriginalRefusalV1::into_state_v1)?];
    for key in keys {
        let reader = StateNodesV2(&b.state);
        let root = reader.current(index_spec()).map_err(state_error)?;
        let old = nodes::lookup(&reader, root, key.as_byte_slice()).map_err(state_error)?;
        if remove && old.is_none() {
            return Err(refused("archive native index vanished before cleanup"));
        }
        if !remove && old.is_some() {
            return Err(refused("archive native index installs once"));
        }
        let mut raw = [0u8; 64];
        raw.copy_from_slice(r.contract.as_byte_slice());
        let plan = nodes::update(&reader, root, key.as_byte_slice(), old, (!remove).then(|| body_value(&raw))).map_err(state_error)?;
        store::publish_component_v2(b, &plan, (!remove).then_some(raw.as_slice())).map_err(state_error)?;
    }
    Ok(())
}

fn receipt_edit_lease_v1(
    r: &OriginalArchiveReceiptV1,
) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, PalwStateV2Error> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<OriginalArchiveReceiptV1>(1).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<NativeAncestorArchiveDescriptorV19>(r.ancestors.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    for parent in &r.ancestors {
        q.add_v1(parent.clone_requests_v19().map_err(PalwStateV2Error::NativeResource)?).map_err(PalwStateV2Error::NativeResource)?;
    }
    reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)
}
fn ancestor_clone_lease_v1(
    parents: &[NativeAncestorArchiveDescriptorV19],
) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, PalwStateV2Error> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<NativeAncestorArchiveDescriptorV19>(parents.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    for parent in parents {
        q.add_v1(parent.clone_requests_v19().map_err(PalwStateV2Error::NativeResource)?).map_err(PalwStateV2Error::NativeResource)?;
    }
    reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)
}

fn packet_from_receipt_v1(r: &OriginalArchiveReceiptV1, ordinal: u64) -> Result<ServicePacket, PalwStateV2Error> {
    if ordinal >= r.packets {
        return Err(refused("archive restored packet ordinal outside native reservation"));
    }
    let id = hash(6, &(r.claim, r.allocation, r.profiles, ordinal)).map_err(FundedOriginalRefusalV1::into_state_v1)?;
    let mut bytes = [0; 64];
    bytes.copy_from_slice(id.as_byte_slice());
    let consumed = ordinal.checked_mul(r.quantum).ok_or(PalwStateV2Error::Overflow("archive restored holding offset"))?;
    Ok(ServicePacket {
        id: bytes,
        release_daa: r.point.daa_score.checked_add(1).ok_or(PalwStateV2Error::Overflow("archive restored first"))?,
        service_before: r.hard,
        cost: ServiceCost {
            bytes: r.holding.checked_sub(consumed).ok_or_else(|| refused("archive restored holding underflow"))?.min(r.quantum),
            work: 1,
            io: 0,
            controls: 1,
        },
    })
}
pub(crate) fn validate_original_archive_packets_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    service: &crate::palw_court_service_state_v2::CourtServiceStateV2,
    universe: &mut std::collections::BTreeSet<[u8; 64]>,
) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, PalwStateV2Error> {
    // Caller is the native all-branch restore of this actual Frame, so its
    // Source8 original-point row is authoritative and no escrow scan is needed.
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
        .ok_or_else(|| refused("archive restored actual frame missing"))?;
    let key = hash(8, &(claim, frame.plan_v1().allocation_id)).map_err(FundedOriginalRefusalV1::into_state_v1)?;
    let Some(contract) = contract_for_index_key_v1(state, key)? else { return Ok(None) };
    let r = receipt_for_contract(state, contract)?.ok_or_else(|| refused("restored archive allocation lost receipt"))?;
    check_receipt(state, &r)?;
    let binding = service.claims.get(&claim).ok_or_else(|| refused("restored archive binding absent"))?;
    if (binding.allocation_id, binding.source_id, binding.original_accepted_daa, binding.hard_deadline_daa)
        != (r.allocation, r.source, r.point.daa_score, r.hard)
    {
        return Err(refused("restored archive binding changed scope"));
    }
    let mut demand = TirNativeRequestsV1::new();
    demand.exact_v1::<BTreeNodeEnvelopeV1<[u8; 64], ()>>(r.packets).map_err(PalwStateV2Error::NativeResource)?;
    let nodes_owner = reserve_native_requests_v1(demand.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    for ordinal in 0..r.packets {
        let packet = packet_from_receipt_v1(&r, ordinal)?;
        if !universe.insert(packet.id) || service.book.packet(&packet.id) != Some(&packet) {
            return Err(refused("restored archive packet duplicate, missing or changed"));
        }
    }
    Ok(nodes_owner)
}
pub(crate) fn validate_original_archive_receipts_v1(state: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    let map = |e: PalwStateV2Error| match e {
        PalwStateV2Error::NativeResource(c) => ContextErrorV2::Resource(c),
        _ => ContextErrorV2::Malformed("original archive financial restore mismatch"),
    };
    let mut expected_ancestor_leaves = 0u64;
    let mut expected_birth_leaves = 0u64;
    for row in state
        .funded_escrows_v1()
        .values()
        .filter(|r| r.version == crate::palw_funded_escrow_v1::PALW_FUNDED_ESCROW_ORIGINAL_ARCHIVE_ROW_VERSION_V1)
    {
        let stored = receipt_for_contract(state, row.contract_id).map_err(map)?;
        if row.settled_by.is_some() && stored.is_none() {
            continue;
        }
        let r = stored.ok_or(ContextErrorV2::Malformed("version2 archive escrow missing receipt"))?;
        check_receipt(state, &r).map_err(map)?;
        expected_birth_leaves =
            expected_birth_leaves.checked_add(2).ok_or(ContextErrorV2::Malformed("financial index count overflow"))?;
        if !r.constructed {
            return Err(ContextErrorV2::Malformed("unfinished archive financial admission cannot restore"));
        }
        if contract_for_birth_v1(state, r.claim, r.point).map_err(map)? != Some(r.contract)
            || contract_for_index_key_v1(
                state,
                hash(8, &(r.claim, r.allocation)).map_err(FundedOriginalRefusalV1::into_state_v1).map_err(map)?,
            )
            .map_err(map)?
                != Some(r.contract)
        {
            return Err(ContextErrorV2::Malformed("archive receipt financial indices differ"));
        }
        if r.released_at.is_none() {
            super::super::palw_claim_local_born_tir_source_v16::verify_original_archive_body_bounds_v1(
                state, r.claim, r.point, &r.bodies,
            )
            .map_err(map)?;
            recheck_published_known_aliases_v19(state, &r).map_err(map)?;
            if let Some(e) = r.owner18.as_ref() {
                super::super::palw_claim_local_legacy_terminal_v18::verify_fp_owner18_envelope_v1(state, r.claim, r.point, e)
                    .map_err(map)?;
            }
            expected_ancestor_leaves = expected_ancestor_leaves
                .checked_add(r.ancestors.len() as u64)
                .ok_or(ContextErrorV2::Malformed("ancestor lease count overflow"))?;
            for parent in &r.ancestors {
                parent.recheck_native_v19(state).map_err(map)?;
                let reader = StateNodesV2(state);
                let root = reader.owned_current(
                    ancestor_scope_v1(parent.claim_v19(), parent.point_v19()).map_err(map)?,
                    ancestor_index_spec_v1(),
                )?;
                let value = nodes::lookup(&reader, root, r.contract.as_byte_slice())?
                    .ok_or(ContextErrorV2::Malformed("live ancestor descriptor lacks paid lease index"))?;
                let raw = reader.body(value)?;
                let actual: (Hash64, PalwBlockContextV2, Hash64, u64) =
                    borsh::from_slice(&raw).map_err(|_| ContextErrorV2::Malformed("ancestor index codec"))?;
                if actual != (parent.claim_v19(), parent.point_v19(), ancestor_digest_v1(parent).map_err(map)?, r.material_until) {
                    return Err(ContextErrorV2::Malformed("ancestor index differs from exact child paid descriptor"));
                }
            }
        }
    }
    // Every financial root is a known current one-row receipt or the bounded
    // two-key source index. Extra owner17 roots cannot invent new authority.
    let mut actual_ancestor_leaves = 0u64;
    let mut actual_birth_leaves = 0u64;
    if let Some(route) = state.kernel_route() {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'I') {
                continue;
            }
            let root: ComponentRootV2 =
                borsh::from_slice(raw).map_err(|_| ContextErrorV2::Malformed("archive financial current root codec"))?;
            if root.spec.owner != 17 {
                continue;
            }
            if root.spec == index_spec() {
                if key.len() != 18 {
                    return Err(ContextErrorV2::Malformed("archive financial index key shape"));
                }
                actual_birth_leaves =
                    actual_birth_leaves.checked_add(root.rows).ok_or(ContextErrorV2::Malformed("financial index count overflow"))?;
                continue;
            }
            if root.spec == ancestor_index_spec_v1() {
                if key.len() != 84 || key[1..3] != [2, 0] || root.rows == 0 {
                    return Err(ContextErrorV2::Malformed("ancestor lease current root shape"));
                }
                // Generic context validation already traversed this exact root and
                // verified its actual leaf count. All expected unique keys above
                // must exist with exact values. Equal total cardinality excludes
                // orphan leaves without re-decoding each entire child receipt for
                // each ancestor, or allocating a duplicate census heap.
                actual_ancestor_leaves =
                    actual_ancestor_leaves.checked_add(root.rows).ok_or(ContextErrorV2::Malformed("ancestor lease count overflow"))?;
                continue;
            }
            if root.spec != receipt_spec() || key.len() != 84 || root.rows != 1 {
                return Err(ContextErrorV2::Malformed("archive financial owned root shape"));
            }
            nodes::visit_nodes(&StateNodesV2(state), root, |_, node| {
                if let nodes::ContextNodeV2::Leaf { key, value } = node {
                    let raw = StateNodesV2(state).body(*value)?;
                    let r = decode_receipt(&raw).map_err(map)?;
                    if key.as_slice() != r.contract.as_byte_slice() {
                        return Err(ContextErrorV2::Malformed("archive financial current leaf contract"));
                    }
                    check_receipt(state, &r).map_err(map)?;
                    if StateNodesV2(state).owned_current(r.contract, receipt_spec())? != root {
                        return Err(ContextErrorV2::Malformed("archive financial owned root scope"));
                    }
                }
                Ok(())
            })?;
        }
    }
    if actual_birth_leaves != expected_birth_leaves {
        return Err(ContextErrorV2::Malformed("financial birth index orphan or missing leaf"));
    }
    if actual_ancestor_leaves != expected_ancestor_leaves {
        return Err(ContextErrorV2::Malformed("ancestor lease orphan or missing leaf"));
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn assert_actual_ancestor_contingent_holding_v20(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
) -> Result<usize, PalwStateV2Error> {
    let before = state.state_root();
    let r = verified_original_archive_reservation_v1(state, claim, point)?
        .ok_or_else(|| refused("contingent holding fixture lacks actual funded child"))?;
    let mut checked = 0;
    for parent in &r.ancestors {
        parent.recheck_native_v19(state)?;
        let Some(old_h) = parent.old_paid_until_v19() else { continue };
        let same = ancestor_lease_geometry_v1(std::slice::from_ref(parent), old_h).map_err(FundedOriginalRefusalV1::into_state_v1)?;
        let after = ancestor_lease_geometry_v1(
            std::slice::from_ref(parent),
            old_h.checked_add(1).ok_or(PalwStateV2Error::Overflow("contingent test horizon"))?,
        )
        .map_err(FundedOriginalRefusalV1::into_state_v1)?;
        assert!(parent.retained_bytes_v19() > 0);
        assert!(
            same.holding >= parent.retained_bytes_v19(),
            "same-H successor must independently reserve actual immutable closure capacity before parent release"
        );
        assert_eq!(
            (same.retained, same.journal, same.holding),
            (after.retained, after.journal, after.holding),
            "one-time rent and contingent capital do not vanish at signed-horizon overlap"
        );
        assert_eq!(parent.publication_bytes_v19(), 0, "proved paid immutable material is not republished or burned again");
        checked += 1;
    }
    assert_eq!(state.state_root(), before);
    Ok(checked)
}

fn profile_v2<T: borsh::BorshSerialize>(kind: usize, v: &T) -> Result<Hash64, FundedOriginalRefusalV1> {
    const TAGS: [[u8; 8]; 4] = [*b"OAR2FEE0", *b"OAR2CAP0", *b"OAR2RET0", *b"OAR2SET0"];
    let digest = hash(kind as u8 + 1, v)?;
    let mut out = [0; 64];
    out.copy_from_slice(digest.as_byte_slice());
    out[..8].copy_from_slice(&TAGS[kind]);
    Ok(Hash64::from_bytes(out))
}
fn owner18_geometry_v2(e: &NativeOwner18FpEnvelopeV1) -> Result<LifetimeGeometryV1, FundedOriginalRefusalV1> {
    if !e.expects_modern_alias_v18() {
        return Err(bad("mixed original lacks finite modern cohort envelope"));
    }
    let birth = e.birth_contribution_v18();
    if !birth.needed_v18() || !birth.original_body_aliases_source8_v18() {
        return Err(bad("mixed original lacks native own cohort contribution identity"));
    }
    let mut total = LifetimeGeometryV1::default();
    let mut birth_aliases = [false; 256];
    birth_aliases[1] = true;
    for row in birth.rows_v18() {
        let bytes = row.body_bytes_v18();
        if bytes == 0 {
            continue;
        }
        let mut g = component(row.spec_v18(), bytes, row.owned_v18())?;
        let alias = row.body_alias_group_v18() as usize;
        if birth_aliases[alias] {
            let b = literal_entry_geometry_v19(bytes)?;
            g.retained = g.retained.checked_sub(b.retained).ok_or_else(overflow)?;
            g.journal = g.journal.checked_sub(b.journal).ok_or_else(overflow)?;
            g.holding = g.holding.checked_sub(b.holding).ok_or_else(overflow)?;
        }
        birth_aliases[alias] = true;
        // Scratch roots are real bounded transient publication/undo peak,
        // not a persistent copy of the existing same-root member population.
        let _transient = row.transient_scratch_v18();
        total = sum(total, g)?;
    }
    let pin = birth.a2_bytes_v18();
    let retained = aux_entry_retained_v19(65, pin)?;
    let journal = add(delta(65, None, Some(pin))?, delta(65, Some(pin), None)?)?;
    total = sum(total, LifetimeGeometryV1 { retained, journal, holding: add(retained, journal)?, native: retained })?;
    let mut aliases = [false; 256];
    for row in e.contributions_v18() {
        let bytes = row.body_bytes_v18();
        if bytes == 0 {
            continue;
        }
        let mut g = component(row.spec_v18(), bytes, row.owned_v18())?;
        let alias = row.body_alias_group_v18() as usize;
        if aliases[alias] {
            let b = literal_entry_geometry_v19(bytes)?;
            g.retained = g.retained.checked_sub(b.retained).ok_or_else(overflow)?;
            g.journal = g.journal.checked_sub(b.journal).ok_or_else(overflow)?;
            g.holding = g.holding.checked_sub(b.holding).ok_or_else(overflow)?;
        }
        aliases[alias] = true;
        total = sum(total, g)?;
    }
    let a = e.a2_bytes_v18();
    let retained = aux_entry_retained_v19(65, a)?;
    let journal = add(delta(65, None, Some(a))?, delta(65, Some(a), None)?)?;
    sum(total, LifetimeGeometryV1 { retained, journal, holding: add(retained, journal)?, native: retained })
}
fn lifetime_composed_v2(
    bodies: &[u64; 21],
    manifest: u64,
    reverse: u64,
    receipt: u64,
    ctx: &PalwBlockContextV2,
    facts: u64,
    parents: &[NativeAncestorArchiveDescriptorV19],
    aliases: &PaidKnownBodyAliasesV19,
    h: u64,
    owner18: Option<&NativeOwner18FpEnvelopeV1>,
) -> Result<LifetimeGeometryV1, FundedOriginalRefusalV1> {
    let base = lifetime(bodies, manifest, reverse, receipt, ctx, facts, parents, aliases, h)?;
    match owner18 {
        Some(e) => sum(base, owner18_geometry_v2(e)?),
        None => Ok(base),
    }
}

/// Actual initial construction capacity, never Source16 ready/material standing.
/// No Deserialize/Clone/public scalar constructor can mint it.
pub(crate) struct NativeMixedArchiveCapacityV21 {
    claim: Hash64,
    point: PalwBlockContextV2,
    source: Hash64,
    allocation: Hash64,
    resource_plan: Hash64,
    contract: Hash64,
    envelope: NativeOwner18FpEnvelopeV1,
}
impl NativeMixedArchiveCapacityV21 {
    pub(crate) fn claim_v21(&self) -> Hash64 {
        self.claim
    }
    pub(crate) fn point_v21(&self) -> PalwBlockContextV2 {
        self.point
    }
    pub(crate) fn source_v21(&self) -> Hash64 {
        self.source
    }
    pub(crate) fn allocation_v21(&self) -> Hash64 {
        self.allocation
    }
    pub(crate) fn resource_plan_v21(&self) -> Hash64 {
        self.resource_plan
    }
    pub(crate) fn contract_v21(&self) -> Hash64 {
        self.contract
    }
    pub(crate) fn owner18_envelope_v21(&self) -> &NativeOwner18FpEnvelopeV1 {
        &self.envelope
    }
    pub(crate) fn covers_owner_scope_v21(
        &self,
        claim: Hash64,
        point: PalwBlockContextV2,
        source: Hash64,
        allocation: Hash64,
        resource_plan: Hash64,
        e: &NativeOwner18FpEnvelopeV1,
    ) -> Result<(), PalwStateV2Error> {
        if (self.claim, self.point, self.source, self.allocation, self.resource_plan)
            != (claim, point, source, allocation, resource_plan)
            || self.envelope.binding_v18()? != e.binding_v18()?
        {
            return Err(refused("mixed archive capacity substitutes actual scope or native envelope"));
        }
        Ok(())
    }
}
pub(crate) fn verified_mixed_archive_capacity_v21(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
) -> Result<Option<NativeRetainedV1<NativeMixedArchiveCapacityV21>>, PalwStateV2Error> {
    let Some(r) = verified_original_archive_reservation_v1(state, claim, point)? else { return Ok(None) };
    if r.version != 2 {
        return Ok(None);
    }
    let envelope = r.owner18.ok_or_else(|| refused("mixed receipt lacks native owner18 envelope"))?;
    if r.released_at.is_some() {
        return Err(refused("released archive is not a new mixed-admission capacity"));
    }
    let c = state.claim(&claim).ok_or_else(|| refused("mixed capacity lacks actual admitted Claim"))?;
    if (c.accepted_block, c.accepted_daa, c.accepted_blue_score) != (point.block, point.daa_score, point.blue_score) {
        return Err(refused("mixed capacity changes actual Claim birth"));
    }
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
        .ok_or_else(|| refused("mixed initial capacity lacks actual native Frame"))?;
    let plan = frame.plan_v1();
    if (plan.source_id, plan.allocation_id, plan.resource_plan_id) != (r.source, r.allocation, r.resource_plan) {
        return Err(refused("mixed initial capacity changes actual Frame"));
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<NativeMixedArchiveCapacityV21>(1).map_err(PalwStateV2Error::NativeResource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(
        NativeMixedArchiveCapacityV21 {
            claim,
            point,
            source: r.source,
            allocation: r.allocation,
            resource_plan: r.resource_plan,
            contract: r.contract,
            envelope,
        },
        lease,
    )))
}

#[cfg(test)]
pub(super) fn assert_actual_receipt_version_codec_v2(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let before = state.state_root();
    let r = verified_original_archive_reservation_v1(state, claim, point)?
        .ok_or_else(|| refused("version fixture lacks actual funded receipt"))?;
    let size = borsh::object_length(&*r).map_err(|_| refused("receipt version codec extent"))? as u64;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(size.checked_mul(6).ok_or(PalwStateV2Error::Overflow("receipt test buffers"))?)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _memory = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    // Borsh nested tuples/arrays add NO framing: this is the exact pre-OAR2
    // v1 field stream, calculated from the genuine original receipt only.
    let mut expected = borsh::to_vec(&(
        r.version,
        [r.claim, r.source, r.allocation, r.resource_plan, r.rules, r.relative_graph, r.absolute_graph, r.contract],
        r.point,
        [r.hard, r.material_until, r.source_min_material_until],
        r.bodies,
        [
            r.input_capacity,
            r.consumer_facts_bytes,
            r.holding,
            r.packets,
            r.quantum,
            r.capital,
            r.base_rent,
            r.reserved_rent,
            r.base_required,
            r.capital_ceiling,
            r.row_price,
            r.stream_price,
            r.capital_per_packet,
        ],
        r.base_profiles,
        r.profiles,
        &r.funding_terms,
        r.ancestors.as_slice(),
        r.known_aliases,
        r.constructed,
        r.released_at,
    ))
    .unwrap();
    if let Some(e) = r.owner18.as_ref() {
        borsh::BorshSerialize::serialize(e, &mut expected).unwrap();
    }
    let raw = borsh::to_vec(&*r).unwrap();
    assert_eq!(raw, expected, "v1 exact bytes preserved; only explicit v2 appends its envelope");
    let decoded = decode_receipt(&raw)?;
    check_receipt(state, &decoded)?;
    assert_eq!(borsh::to_vec(&*decoded).unwrap(), raw);
    let mut wrong = point;
    wrong.blue_score = wrong.blue_score.checked_add(1).unwrap();
    assert!(verified_mixed_archive_capacity_v21(state, claim, wrong)?.is_none());
    if r.version == 1 {
        assert!(
            verified_mixed_archive_capacity_v21(state, claim, point)?.is_none(),
            "old material payment cannot grant new owner18 capacity"
        );
    } else {
        let token =
            verified_mixed_archive_capacity_v21(state, claim, point)?.ok_or_else(|| refused("actual mixed receipt lost capacity"))?;
        token.covers_owner_scope_v21(claim, point, r.source, r.allocation, r.resource_plan, r.owner18.as_ref().unwrap())?;
        let wrong = Hash64::from_u64_word(0xfee218);
        assert_ne!(wrong, r.resource_plan);
        assert!(
            token.covers_owner_scope_v21(claim, point, r.source, r.allocation, wrong, r.owner18.as_ref().unwrap()).is_err(),
            "real capacity cannot cover another resource plan"
        );
        let actual = owner18_geometry_v2(r.owner18.as_ref().unwrap()).map_err(FundedOriginalRefusalV1::into_state_v1)?;
        assert!(
            actual.retained > 0 && actual.journal > 0 && actual.holding > actual.retained,
            "actual new own 15/18 publications and bounded undo are explicitly reserved"
        );
        assert_eq!(r.owner18.as_ref().unwrap().empty_exec_codec_bytes_v18(), 11, "native FP cannot own an Attempt EXEC root");
    }
    assert_eq!(state.state_root(), before);
    Ok(())
}

pub(super) fn quote_minimum_original_archive_at_native_v2(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    view: &PreparedOriginalArchiveServiceViewV11<'_>,
) -> Result<NativeRetainedV1<NativeOriginalArchiveReservationV1>, FundedOriginalRefusalV1> {
    quote_internal_v1(b, ctx, view, None, None, true)
}

// Representation-law discriminator inside the SAME four OAR2 hash families.
// No tariff/signature/actor changes; old tuples keep their exact old geometry.
const OAR2_LITERAL_ALIAS_GEOMETRY_DOMAIN_V3: &[u8] = b"source16/shared-class-layout-literals/v1";
fn receipt_uses_literal_alias_geometry_v3(r: &OriginalArchiveReceiptV1) -> Result<bool, FundedOriginalRefusalV1> {
    Ok(r.version == 2 && r.owner18.is_some() && receipt_profiles_geometry_v3(r, true)? == r.profiles)
}
// Explicit native geometry inputs preserve the signed OAR2 versioned replay law.
#[allow(clippy::too_many_arguments)]
fn lifetime_composed_geometry_v3(
    dedup: bool,
    bodies: &[u64; 21],
    manifest: u64,
    reverse: u64,
    receipt: u64,
    ctx: &PalwBlockContextV2,
    facts: u64,
    parents: &[NativeAncestorArchiveDescriptorV19],
    aliases: &PaidKnownBodyAliasesV19,
    h: u64,
    owner18: Option<&NativeOwner18FpEnvelopeV1>,
) -> Result<LifetimeGeometryV1, FundedOriginalRefusalV1> {
    let mut g = lifetime_composed_v2(bodies, manifest, reverse, receipt, ctx, facts, parents, aliases, h, owner18)?;
    if dedup {
        if owner18.is_none() || bodies[3] != bodies[19] || bodies[4] != bodies[20] {
            return Err(bad("new OAR2 literal alias geometry lacks native class/layout shape identity"));
        }
        // Initial and postblock native class/layout are the same registered
        // immutable bodies. Remove ONLY the duplicate B, never either tag's
        // leaf/path/current pointer/undo or child contingent holding.
        for bytes in [bodies[19], bodies[20]] {
            let duplicate = literal_entry_geometry_v19(bytes)?;
            g.retained = g.retained.checked_sub(duplicate.retained).ok_or_else(overflow)?;
            g.journal = g.journal.checked_sub(duplicate.journal).ok_or_else(overflow)?;
            g.holding = g.holding.checked_sub(duplicate.holding).ok_or_else(overflow)?;
        }
    }
    Ok(g)
}
fn recheck_intra_publication_aliases_v3(state: &PalwChainStateV2, r: &OriginalArchiveReceiptV1) -> Result<(), PalwStateV2Error> {
    for (initial, completed) in [(4u64, 20u64), (5, 21)] {
        let a =
            super::super::palw_claim_local_born_tir_source_v16::original_archive_body_reference_v19(state, r.claim, r.point, initial)?
                .ok_or_else(|| refused("OAR2 initial literal alias body absent"))?;
        let b = super::super::palw_claim_local_born_tir_source_v16::original_archive_body_reference_v19(
            state, r.claim, r.point, completed,
        )?
        .ok_or_else(|| refused("OAR2 completed literal alias body absent"))?;
        if a != b || a.bytes != r.bodies[(initial - 1) as usize] {
            return Err(refused("OAR2 native canonical literal alias identity differs"));
        }
    }
    Ok(())
}
#[cfg(test)]
pub(super) fn assert_actual_literal_alias_geometry_v3(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
    expected_new: bool,
) -> Result<(), PalwStateV2Error> {
    let before = state.state_root();
    let r = verified_original_archive_reservation_v1(state, claim, point)?
        .ok_or_else(|| refused("native alias fixture lacks actual receipt"))?;
    if r.version == 1 || !expected_new {
        assert!(!receipt_uses_literal_alias_geometry_v3(&r).unwrap());
        assert_eq!(receipt_profiles(&r).unwrap(), receipt_profiles_geometry_v3(&r, false).unwrap());
        if r.version == 2 {
            recheck_intra_publication_aliases_v3(state, &r)?;
        }
        assert_eq!(state.state_root(), before);
        return Ok(());
    }
    assert!(receipt_uses_literal_alias_geometry_v3(&r).unwrap());
    recheck_intra_publication_aliases_v3(state, &r)?;
    let _copy = receipt_edit_lease_v1(&r)?;
    let mut legacy = (*r).clone();
    legacy.profiles = receipt_profiles_geometry_v3(&legacy, false).unwrap();
    assert!(
        !receipt_uses_literal_alias_geometry_v3(&legacy).unwrap(),
        "old OAR2 tuple must retain old geometry; same wire does not grant the new law"
    );
    assert_eq!(receipt_profiles(&legacy).unwrap(), legacy.profiles);
    assert_ne!(r.profiles, legacy.profiles);
    let duplicate = sum(literal_entry_geometry_v19(r.bodies[19]).unwrap(), literal_entry_geometry_v19(r.bodies[20]).unwrap()).unwrap();
    assert!(duplicate.retained > 0 && duplicate.journal > 0 && duplicate.holding > duplicate.retained);
    let root = ComponentRootV2 {
        spec: ComponentSpecV2 { owner: 16, table: 2, schema: 10, key_bytes: 64, value_bytes: u64::MAX },
        node: Some(r.claim),
        rows: 21,
    };
    let manifest = len(&(16u16, r.claim, r.point, r.claim, r.claim, root, 0u64, 0u64, false)).unwrap();
    let reverse = len(&(r.claim, r.point)).unwrap();
    let width = add(
        receipt_codec_upper_v1(&r.point, &r.funding_terms, &r.ancestors, &r.known_aliases).unwrap(),
        r.owner18.as_ref().map(len).transpose().unwrap().unwrap_or(0),
    )
    .unwrap();
    let old = lifetime_composed_v2(
        &r.bodies,
        manifest,
        reverse,
        width,
        &r.point,
        r.consumer_facts_bytes,
        &r.ancestors,
        &r.known_aliases,
        r.material_until,
        r.owner18.as_ref(),
    )
    .unwrap();
    let new = lifetime_composed_geometry_v3(
        true,
        &r.bodies,
        manifest,
        reverse,
        width,
        &r.point,
        r.consumer_facts_bytes,
        &r.ancestors,
        &r.known_aliases,
        r.material_until,
        r.owner18.as_ref(),
    )
    .unwrap();
    assert_eq!(
        (old.retained - new.retained, old.journal - new.journal, old.holding - new.holding),
        (duplicate.retained, duplicate.journal, duplicate.holding)
    );
    assert_eq!(new.holding, r.holding);
    // These are actual canonical RVs, not equality of widths or caller flags.
    // Old-paid class/layout cases are checked by the existing actual parent
    // proof fixture; dedup changes no ancestor contingent-capacity calculation.
    assert_eq!(state.state_root(), before);
    Ok(())
}

/// Test-only constructor for genuinely signing the historic OAR2 tuple before
/// any actual acceptance. It grants no state/receipt or public quote override.
#[cfg(test)]
pub(super) fn quote_minimum_original_archive_legacy_oar2_v3(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    view: &PreparedOriginalArchiveServiceViewV11<'_>,
) -> Result<NativeRetainedV1<NativeOriginalArchiveReservationV1>, FundedOriginalRefusalV1> {
    quote_internal_geometry_v3(b, ctx, view, None, None, true, false)
}
#[cfg(test)]
pub(super) fn quote_original_archive_legacy_oar2_v3(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    view: &PreparedOriginalArchiveServiceViewV11<'_>,
    object: &PalwConsensusObjectV2,
    ceilings: OriginalArchivePayerCeilingsV1,
) -> Result<NativeRetainedV1<NativeOriginalArchiveReservationV1>, FundedOriginalRefusalV1> {
    quote_internal_geometry_v3(b, ctx, view, Some(object), Some(ceilings), true, false)
}
