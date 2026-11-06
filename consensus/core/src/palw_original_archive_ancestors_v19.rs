//! Native immutable ancestor identities; financial standing is separate.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1 as finance;
use super::*;
use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1, NativeCoverageV1, NativeRetainedV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
type PaidProof = finance::NativeOriginalPaidArchiveProofV19;
type Point = PalwBlockContextV2;
type Key = (Hash64, u64, u64, Hash64, u64);
fn key(c: Hash64, p: Point) -> Key {
    (c, p.daa_score, p.blue_score, p.block, p.subsidy)
}
fn bad(s: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.into())
}
fn map(e: ContextErrorV2) -> PalwStateV2Error {
    super::palw_claim_local_paid_archive_v2::state_error(e)
}
fn add(a: u64, b: u64) -> Result<u64, PalwStateV2Error> {
    a.checked_add(b).ok_or(PalwStateV2Error::Overflow("ancestor archive geometry"))
}
fn width<T: BorshSerialize>(v: &T) -> Result<u64, PalwStateV2Error> {
    borsh::object_length(v).map(|n| n as u64).map_err(|_| bad("ancestor archive native codec"))
}
fn native_context(e: PalwStateV2Error) -> ContextErrorV2 {
    match e {
        PalwStateV2Error::NativeResource(r) => ContextErrorV2::Resource(r),
        _ => ContextErrorV2::Malformed("ancestor canonical storage geometry"),
    }
}
fn retained_body(bytes: u64) -> Result<u64, PalwStateV2Error> {
    finance::canonical_aux_entry_retained_bytes_v19(65, add(4, bytes)?)
}
fn rv(raw: &[u8]) -> RowValueV2 {
    RowValueV2 { data: crate::palw_claim_local_v1::palw_liability_material_id_v1(raw), bytes: raw.len() as u64 }
}
fn aux(prefix: u8, h: Hash64) -> Vec<u8> {
    let mut out = Vec::with_capacity(65);
    out.push(prefix);
    out.extend_from_slice(h.as_byte_slice());
    out
}
#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub(super) struct NativeAncestorDiscoveryRowV19 {
    spec: ComponentSpecV2,
    key: Vec<u8>,
    bytes: u64,
    direct: bool,
}
crate::palw_native_resource_v1::wire::wire_struct!(NativeAncestorDiscoveryRowV19;ComponentSpecV2,Vec<u8>,u64,bool);
impl NativeAncestorDiscoveryRowV19 {
    pub(super) fn spec_v19(&self) -> ComponentSpecV2 {
        self.spec
    }
    pub(super) fn key_v19(&self) -> &[u8] {
        &self.key
    }
    pub(super) fn body_bytes_v19(&self) -> u64 {
        self.bytes
    }
    pub(super) fn mutation_count_v19(&self) -> u64 {
        1
    }
    pub(super) fn is_direct_aux_v19(&self) -> bool {
        self.direct
    }
}
#[derive(BorshSerialize, BorshDeserialize)]
pub(super) struct NativeAncestorArchiveDescriptorV19 {
    version: u16,
    origin: u8,
    claim: Hash64,
    point: Point,
    material_until: u64,
    contract: Option<Hash64>,
    old_paid_until: Option<u64>,
    paid_proof: Option<PaidProof>,
    terms: Option<crate::palw_court_original_escrow_v1::FundingTermsV1>,
    references: store::ArchiveReferencesV2,
    discovery: Vec<NativeAncestorDiscoveryRowV19>,
    legacy_key: Option<Vec<u8>>,
    retained: u64,
    publication: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(NativeAncestorArchiveDescriptorV19;u16,u8,Hash64,Point,u64,Option<Hash64>,Option<u64>,Option<PaidProof>,Option<crate::palw_court_original_escrow_v1::FundingTermsV1>,store::ArchiveReferencesV2,Vec<NativeAncestorDiscoveryRowV19>,Option<Vec<u8>>,u64,u64);
impl Clone for NativeAncestorArchiveDescriptorV19 {
    fn clone(&self) -> Self {
        Self {
            version: self.version,
            origin: self.origin,
            claim: self.claim,
            point: self.point,
            material_until: self.material_until,
            contract: self.contract,
            old_paid_until: self.old_paid_until,
            paid_proof: self.paid_proof.clone(),
            terms: self.terms.clone(),
            references: store::ArchiveReferencesV2 {
                version: self.references.version,
                components: self.references.components.clone(),
                literals: self.references.literals.clone(),
            },
            discovery: self.discovery.clone(),
            legacy_key: self.legacy_key.clone(),
            retained: self.retained,
            publication: self.publication,
        }
    }
}
impl NativeAncestorArchiveDescriptorV19 {
    pub(super) fn claim_v19(&self) -> Hash64 {
        self.claim
    }
    pub(super) fn point_v19(&self) -> Point {
        self.point
    }
    pub(super) fn contract_v19(&self) -> Option<Hash64> {
        self.contract
    }
    pub(super) fn material_until_v19(&self) -> u64 {
        self.material_until
    }
    pub(super) fn old_paid_until_v19(&self) -> Option<u64> {
        self.old_paid_until
    }
    pub(super) fn archive_references_v19(&self) -> &store::ArchiveReferencesV2 {
        &self.references
    }
    pub(super) fn discovery_rows_v19(&self) -> &[NativeAncestorDiscoveryRowV19] {
        &self.discovery
    }
    pub(super) fn retained_bytes_v19(&self) -> u64 {
        self.retained
    }
    pub(super) fn publication_bytes_v19(&self) -> u64 {
        self.publication
    }
    pub(super) fn original_funding_terms_v19(&self) -> Option<&crate::palw_court_original_escrow_v1::FundingTermsV1> {
        self.terms.as_ref()
    }
    /// Includes fixed descriptor and every owned Vec/key; reserve before Clone.
    pub(super) fn clone_requests_v19(&self) -> Result<u64, NativeCoverageV1> {
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Self>(1)?;
        q.exact_v1::<ComponentRootV2>(self.references.components.capacity() as u64)?;
        q.exact_v1::<RowValueV2>(self.references.literals.capacity() as u64)?;
        q.exact_v1::<NativeAncestorDiscoveryRowV19>(self.discovery.capacity() as u64)?;
        for row in &self.discovery {
            q.exact_v1::<u8>(row.key.capacity() as u64)?;
        }
        if let Some(k) = &self.legacy_key {
            q.exact_v1::<u8>(k.capacity() as u64)?;
        }
        Ok(q.bytes_v1())
    }
}
fn descriptor_workspace() -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, PalwStateV2Error> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<NativeAncestorArchiveDescriptorV19>(1).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<ComponentRootV2>(2).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<RowValueV2>(11).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<NativeAncestorDiscoveryRowV19>(8).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<u8>(8 * 84 + 65).map_err(PalwStateV2Error::NativeResource)?;
    reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)
}
fn source8_descriptor(
    s: &PalwChainStateV2,
    claim: Hash64,
    point: Point,
    old_proof: Option<&PaidProof>,
    capture: bool,
) -> Result<NativeRetainedV1<NativeAncestorArchiveDescriptorV19>, PalwStateV2Error> {
    let lease = descriptor_workspace()?;
    let view = super::palw_claim_local_source_admission_v8::for_accepting_point_v8(s, claim, point)?
        .ok_or_else(|| bad("ancestor native Source8 absent"))?;
    if !matches!(view.original_claim().source, PalwClaimSourceV2::FreePrompt { .. }) {
        return Err(bad("ancestor is not exact native FP"));
    }
    let mut references =
        store::ArchiveReferencesV2 { version: 2, components: Vec::with_capacity(2), literals: Vec::with_capacity(11) };
    let mut discovery = Vec::with_capacity(8);
    let format = super::palw_claim_local_source_admission_v8::fresh_fp_source_codec_v18(&point)?;
    super::palw_claim_local_source_admission_v8::visit_native_archive_references_v19(
        s,
        claim,
        point,
        |refs, manifest, exact, stored, a2| {
            references.literals.extend_from_slice(&refs.literals);
            for (spec, k) in [(format.directory_spec_v18(), exact), (format.stored_spec_v18(), stored)] {
                discovery.push(NativeAncestorDiscoveryRowV19 {
                    spec,
                    key: k.as_byte_slice().to_vec(),
                    bytes: manifest.bytes,
                    direct: false,
                });
            }
            discovery.push(NativeAncestorDiscoveryRowV19 {
                spec: format.directory_spec_v18(),
                key: aux(b'A', a2),
                bytes: width(refs)?,
                direct: true,
            });
            Ok(())
        },
    )?;
    let (facts, facts_key) = super::palw_claim_local_carrier_lineage_v9::native_consumer_fact_reference_v19(s, claim, point)?
        .ok_or_else(|| bad("scoped ancestor lacks actual native consumer facts"))?;
    references.literals.push(facts);
    discovery.push(NativeAncestorDiscoveryRowV19 {
        spec: super::palw_claim_local_carrier_lineage_v9::consumer_facts_component_spec_v18(),
        key: facts_key.as_byte_slice().to_vec(),
        bytes: facts.bytes,
        direct: false,
    });
    let meta = super::palw_claim_local_born_tir_source_v16::native_original_archive_metadata_v19(s, claim, point)?;
    let (mut contract, mut terms, mut until) = (None, None, view.original_claim().trace_retention_daa);
    if let Some(meta) = meta {
        contract = Some(meta.contract);
        terms = Some(meta.terms);
        until = meta.material_until;
        let specs = super::palw_claim_local_born_tir_source_v16::native_original_archive_specs_v19();
        super::palw_claim_local_born_tir_source_v16::visit_original_retention_inventory_v16(s, claim, point, |root, manifest| {
            references.components.push(root);
            references.literals.push(manifest);
            discovery.push(NativeAncestorDiscoveryRowV19 {
                spec: specs[0],
                key: meta.directory.as_byte_slice().to_vec(),
                bytes: manifest.bytes,
                direct: false,
            });
            discovery.push(NativeAncestorDiscoveryRowV19 {
                spec: specs[2],
                key: meta.allocation.as_byte_slice().to_vec(),
                bytes: width(&(claim, point))?,
                direct: false,
            });
            let mut k = Vec::with_capacity(84);
            k.push(b'I');
            k.extend_from_slice(&2u16.to_le_bytes());
            k.extend_from_slice(meta.scope.as_byte_slice());
            root.spec.serialize(&mut k).map_err(|_| bad("ancestor owned root key"))?;
            discovery.push(NativeAncestorDiscoveryRowV19 { spec: root.spec, key: k, bytes: width(&root)?, direct: true });
            discovery.push(NativeAncestorDiscoveryRowV19 {
                spec: root.spec,
                key: aux(b'A', meta.inventory),
                bytes: 2 + 4 + 90 + 4 + 72,
                direct: true,
            });
            Ok(())
        })?;
    }
    let paid_proof = if capture { finance::capture_original_paid_archive_proof_v19(s, claim, point)? } else { old_proof.cloned() };
    if let Some(proof) = &paid_proof {
        proof.recheck_archived_v19(s)?;
        let expected = super::palw_claim_local_born_tir_source_v16::original_funding_evidence_refs_v19(s, claim, point)?
            .ok_or_else(|| bad("paid ancestor proof lost native funding evidence"))?;
        if proof.evidence_literals_v19() != &[expected.0, expected.1] {
            return Err(bad("paid ancestor proof belongs to foreign source"));
        }
        references.components.push(proof.financial_root_v19());
        references.literals.extend_from_slice(proof.evidence_literals_v19());
    }
    let old_paid_until = paid_proof.as_ref().map(PaidProof::material_until_v19);
    let retained = measure(&references, s)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        NativeAncestorArchiveDescriptorV19 {
            version: 19,
            origin: 1,
            claim,
            point,
            material_until: until,
            contract,
            old_paid_until,
            paid_proof,
            terms,
            references,
            discovery,
            legacy_key: None,
            retained,
            publication: 0,
        },
        lease,
    ))
}
fn measure(refs: &store::ArchiveReferencesV2, s: &PalwChainStateV2) -> Result<u64, PalwStateV2Error> {
    // Only source-owned archive roots, never global discovery roots.
    let count = refs.components.iter().try_fold(refs.literals.len() as u64, |n, r| add(n, r.rows))?;
    let node_count = refs.components.iter().try_fold(0u64, |n, r| {
        add(
            n,
            if r.rows == 0 {
                0
            } else {
                r.rows.checked_mul(2).and_then(|n| n.checked_sub(1)).ok_or(PalwStateV2Error::Overflow("ancestor node set"))?
            },
        )
    })?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(add(count, node_count)?).map_err(PalwStateV2Error::NativeResource)?;
    let _lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut bodies = BTreeSet::new();
    let mut total = 0u64;
    let reader = StateNodesV2(s);
    let mut literal = |v: RowValueV2| -> Result<(), ContextErrorV2> {
        if bodies.insert(v.data) {
            reader.body(v)?;
            total = total
                .checked_add(retained_body(v.bytes).map_err(native_context)?)
                .ok_or(ContextErrorV2::Malformed("ancestor retained bytes overflow"))?;
        }
        Ok(())
    };
    for r in &refs.components {
        nodes::visit_nodes(&reader, *r, |_, node| {
            if let nodes::ContextNodeV2::Leaf { value, .. } = node {
                literal(*value)?;
            }
            Ok(())
        })
        .map_err(map)?;
    }
    for v in &refs.literals {
        literal(*v).map_err(map)?;
    }
    let mut seen_nodes = BTreeSet::new();
    for r in &refs.components {
        nodes::visit_nodes(&reader, *r, |id, node| {
            if !seen_nodes.insert(id) {
                return Ok(());
            }
            let n = borsh::object_length(node).map_err(|_| ContextErrorV2::Malformed("ancestor node codec"))? as u64;
            total = total
                .checked_add(finance::canonical_aux_entry_retained_bytes_v19(65, n).map_err(native_context)?)
                .ok_or(ContextErrorV2::Malformed("ancestor node bytes overflow"))?;
            Ok(())
        })
        .map_err(map)?;
    }
    Ok(total)
}
fn same_native(a: &NativeAncestorArchiveDescriptorV19, b: &NativeAncestorArchiveDescriptorV19) -> bool {
    a.old_paid_until == b.old_paid_until
        && a.claim == b.claim
        && a.point == b.point
        && a.origin == b.origin
        && a.material_until == b.material_until
        && a.contract == b.contract
        && a.terms == b.terms
        && a.references.version == b.references.version
        && a.references.components == b.references.components
        && a.references.literals == b.references.literals
        && a.retained == b.retained
        && a.publication == b.publication
        && a.discovery.len() == b.discovery.len()
        && a.discovery
            .iter()
            .zip(&b.discovery)
            .all(|(a, b)| a.spec == b.spec && a.key == b.key && a.bytes == b.bytes && a.direct == b.direct)
}
fn legacy_body(
    s: &PalwChainStateV2,
    d: &NativeAncestorArchiveDescriptorV19,
    value: RowValueV2,
) -> Result<NativeRetainedV1<Vec<u8>>, PalwStateV2Error> {
    if let Some(k) = &d.legacy_key {
        let _key = reserve_native_requests_v1(k.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        if let Some(raw) = s.kernel_route().and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, k.clone()))) {
            if rv(raw) != value {
                return Err(bad("legacy discovery body changed"));
            }
            return NativeRetainedV1::try_from_clone_v1(raw).map_err(PalwStateV2Error::NativeResource);
        }
    }
    StateNodesV2(s).body(value).map_err(map)
}
fn legacy_descriptor(
    s: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim: Hash64,
    point: Point,
) -> Result<NativeRetainedV1<NativeAncestorArchiveDescriptorV19>, PalwStateV2Error> {
    let lease = descriptor_workspace()?;
    let (mut origin, mut legacy_key, mut body, mut until) = (0u8, None, None, 0u64);
    let frozen = finance::with_verified_ancestor_descriptor_v19(
        s,
        claim,
        point.block,
        point.daa_score,
        point.blue_score,
        s.last_point.ok_or_else(|| bad("legacy ancestor lacks native tip"))?.daa_score,
        |d| {
            if d.point_v19() != point {
                return Err(bad("legacy ancestor retained point differs"));
            }
            Ok(d.legacy_f1_literal_v19().map(|v| (v, d.material_until_v19())))
        },
    )?
    .flatten();
    let k = crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(point.block);
    let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
    if let Some(raw) = s.kernel_route().and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, k.to_vec()))) {
        let actual = super::palw_claim_local_source_origin_v8::SourceOriginV8::native_legacy_at_point_v19(s, params, claim, &point)?
            .ok_or_else(|| bad("legacy F1 lacks actual native admission"))?;
        origin = 2;
        body = Some(rv(raw));
        legacy_key = Some(k.to_vec());
        until = actual.original_claim().trace_retention_daa;
        if let Some(e) = s.prefix_export_reservation_v1(&point, claim)? {
            until = until.max(e.material_until);
        }
    } else if let Some((value, material)) = frozen {
        origin = 2;
        body = Some(value);
        until = material;
    } else if let Some(closed) = super::palw_claim_local_exact_closed_source_v5::read_exact_closed_source_v5(s, params, claim, &point)?
    {
        until = closed.material_until;
        if let Some(value) = super::palw_claim_local_exact_closed_source_v5::native_closed_literal_v19(s, claim, point)? {
            origin = 4;
            body = Some(value);
        } else {
            let k = aux(b'S', claim);
            let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
            let raw = s
                .kernel_route()
                .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, k.clone())))
                .ok_or_else(|| bad("actual legacy S1 disappeared"))?;
            origin = 3;
            body = Some(rv(raw));
            legacy_key = Some(k);
        }
    }
    let body = body.ok_or_else(|| bad("ancestor lacks native scoped or immutable legacy provenance"))?;
    let retained = retained_body(body.bytes)?;
    let mut literals = Vec::with_capacity(9);
    literals.push(body);
    let publication = if origin == 4 || legacy_key.is_none() { 0 } else { body.bytes };
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        NativeAncestorArchiveDescriptorV19 {
            version: 19,
            origin,
            claim,
            point,
            material_until: until,
            contract: None,
            old_paid_until: None,
            paid_proof: None,
            terms: None,
            references: store::ArchiveReferencesV2 { version: 2, components: Vec::with_capacity(1), literals },
            discovery: Vec::with_capacity(8),
            legacy_key,
            retained,
            publication,
        },
        lease,
    ))
}
impl NativeAncestorArchiveDescriptorV19 {
    pub(super) fn recheck_native_v19(&self, s: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
        if self.version != 19
            || self.references.version != 2
            || self.old_paid_until != self.paid_proof.as_ref().map(PaidProof::material_until_v19)
        {
            return Err(bad("ancestor descriptor version/unproven old-paid interval"));
        }
        if self.origin == 1 {
            let actual = source8_descriptor(s, self.claim, self.point, self.paid_proof.as_ref(), false)?;
            if !same_native(self, &actual) {
                return Err(bad("ancestor native immutable/discovery binding changed"));
            }
        } else if matches!(self.origin, 2..=4) {
            let value = *self.references.literals.first().ok_or_else(|| bad("legacy ancestor body absent"))?;
            if self.references.literals.len() != 1
                || !self.references.components.is_empty()
                || !self.discovery.is_empty()
                || self.contract.is_some()
                || self.terms.is_some()
                || self.paid_proof.is_some()
            {
                return Err(bad("legacy ancestor descriptor shape"));
            }
            let raw = legacy_body(s, self, value)?;
            if self.origin == 2 {
                let frame = crate::palw_claim_local_v1::palw_decode_claim_liability_frame_v1(&raw).map_err(|e| match e {
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
                    _ => bad("frozen legacy F1 codec"),
                })?;
                let origin =
                    super::palw_claim_local_fold_v1::admission_from_native_retained_frame_v19(frame, &self.point, self.claim)?
                        .ok_or_else(|| bad("frozen legacy F1 lacks original admission"))?;
                if origin.original_claim().trace_retention_daa > self.material_until {
                    return Err(bad("frozen legacy material interval shorter than native source"));
                }
            } else {
                let row = super::palw_claim_local_compact_sources_v1::decode_checked_index_v15(&raw, self.claim)?;
                if row.birth != self.point || row.material_until != self.material_until {
                    return Err(bad("frozen native closed-source identity"));
                }
            }
            if self.retained != retained_body(value.bytes)? || (self.publication != 0 && self.publication != value.bytes) {
                return Err(bad("legacy ancestor physical geometry"));
            }
        } else {
            return Err(bad("unknown ancestor native origin"));
        }
        Ok(())
    }
}
/// Called by the financial installer only after exact native recheck, before
/// child A2/index publication. Copies one authenticated frozen artifact only.
pub(super) fn publish_native_ancestor_artifacts_v19(
    b: &mut TransitionBuilder<'_>,
    descriptors: &[NativeAncestorArchiveDescriptorV19],
) -> Result<(), PalwStateV2Error> {
    for d in descriptors {
        d.recheck_native_v19(&b.state)?;
        if !matches!(d.origin, 2 | 3) {
            continue;
        }
        let value = d.references.literals[0];
        let raw = legacy_body(&b.state, d, value)?;
        let k = aux(b'B', value.data);
        if b.state.kernel_route().and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, k.clone()))).is_some() {
            let body = StateNodesV2(&b.state).body(value).map_err(map)?;
            if body.as_slice() != raw.as_slice() {
                return Err(bad("legacy immutable artifact collision"));
            }
        } else {
            b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &k, Some(&raw.as_slice()))?;
        }
    }
    Ok(())
}
fn grow(lease: &mut Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, bytes: u64) -> Result<(), PalwStateV2Error> {
    if let Some(ticket) = lease {
        ticket.grow_memory_v1(bytes).map_err(PalwStateV2Error::NativeResource)
    } else {
        *lease = reserve_native_requests_v1(bytes).map_err(PalwStateV2Error::NativeResource)?;
        Ok(())
    }
}
fn enqueue(
    pending: &mut BTreeMap<Key, (Point, bool)>,
    done: &BTreeMap<Key, NativeAncestorArchiveDescriptorV19>,
    lease: &mut Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
    claim: Hash64,
    point: Point,
    consumer: Point,
    prefix: bool,
) -> Result<(), PalwStateV2Error> {
    if point.blue_score >= consumer.blue_score || point.daa_score > consumer.daa_score {
        return Err(bad("ancestor is not causally prior"));
    }
    let k = key(claim, point);
    if let Some(d) = done.get(&k) {
        if prefix && matches!(d.origin, 3 | 4) {
            return Err(bad("closed scalar origin cannot supply missing prefix archive"));
        }
        return Ok(());
    }
    if let Some((_, was_prefix)) = pending.get_mut(&k) {
        *was_prefix |= prefix;
        return Ok(());
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<BTreeNodeEnvelopeV1<Key, (Point, bool)>>(1).map_err(PalwStateV2Error::NativeResource)?;
    grow(lease, q.bytes_v1())?;
    pending.insert(k, (point, prefix));
    Ok(())
}
/// The only constructor: requirements are the actual sealed native preparation,
/// inputs are the actual paired carrier, and lineage reads the ORIGINAL parent.
/// Existing legacy artifacts are supported without pretending they were paid.
pub(super) fn prepare_native_ancestor_archive_descriptors_v19(
    b: &TransitionBuilder<'_>,
    ctx: &Point,
    requirements: &super::palw_tir_public_fold_v1::PreparedOriginalRequirementsV1,
) -> Result<NativeRetainedV1<Vec<NativeAncestorArchiveDescriptorV19>>, PalwStateV2Error> {
    let parent = b.claim_local_original_parent_v19.ok_or_else(|| bad("ancestor quote lacks sealed native original parent"))?;
    let frame = requirements.frame_v1();
    let claim = frame.plan_v1().original_claim;
    let pairing = b.extras.tir_funded_originals.get(&claim).ok_or_else(|| bad("ancestor quote lacks actual paired carrier"))?;
    if pairing.claim() != claim {
        return Err(bad("ancestor carrier changes native claim"));
    }
    let mut lease = reserve_native_requests_v1(0).map_err(PalwStateV2Error::NativeResource)?;
    let mut pending = BTreeMap::new();
    let mut done = BTreeMap::new();
    let mut ledger_seen = BTreeSet::new();
    if let Some(used) = b.extras.prefix_dependencies_v1.get(&claim).and_then(|e| e.consumed.as_ref()) {
        used.intent
            .verify_signature_v1(&frame.original_v1().job.executor_pubkey, &used.signature)
            .map_err(|_| bad("ancestor quote consumer prefix signature"))?;
        let p = parent.claim_local_prefix_source_point_v1(&used.intent.source)?;
        enqueue(&mut pending, &done, &mut lease, used.intent.source.source_claim, p, *ctx, true)?;
    }
    for txid in pairing
        .carrier()
        .inputs
        .iter()
        .map(|i| i.previous_outpoint.transaction_id)
        .chain(b.extras.carrier_input_spends.get(&claim).into_iter().flat_map(|v| v.iter().copied()))
    {
        super::palw_claim_local_carrier_lineage_v9::visit_original_parent_source_v19(
            parent,
            &b.state,
            b.params,
            txid,
            |source, p| enqueue(&mut pending, &done, &mut lease, source, p, *ctx, false),
        )?;
    }
    while let Some((k, (point, needs_prefix))) = pending.pop_first() {
        if done.contains_key(&k) || !needs_prefix && ledger_seen.contains(&k) {
            continue;
        }
        let source = k.0;
        let has_source8 = super::palw_claim_local_source_admission_v8::for_accepting_point_v8(parent, source, point)?.is_some();
        let ledger_complete = has_source8
            && super::palw_claim_local_carrier_lineage_v9::native_consumer_fact_reference_v19(parent, source, point)?.is_some();
        let descriptor = if !needs_prefix && ledger_complete {
            // Existing fixed eligibility ledger is paid once; an ordinary UTXO edge
            // does not create a new all21 material-serving duty or format prohibition.
            None
        } else if has_source8
            && super::palw_claim_local_born_tir_source_v16::native_original_archive_metadata_v19(parent, source, point)?.is_some()
        {
            Some(source8_descriptor(parent, source, point, None, true)?)
        } else {
            Some(legacy_descriptor(parent, b.params, source, point)?)
        };
        if let Some(d) = descriptor.as_ref() {
            if needs_prefix && matches!(d.origin, 3 | 4) {
                return Err(bad("legacy scalar outcome lacks required prefix material"));
            }
            d.recheck_native_v19(&b.state)?;
        }
        if let Some(origin) = super::palw_claim_local_source_origin_v8::SourceOriginV8::at_point(parent, b.params, source, &point)? {
            if let Some(used) = origin.prefix_use()? {
                let p = parent.claim_local_prefix_source_point_v1(&used.intent.source)?;
                enqueue(&mut pending, &done, &mut lease, used.intent.source.source_claim, p, point, needs_prefix)?;
            }
            let present = super::palw_claim_local_carrier_lineage_v9::visit_consumer_facts_v9(parent, source, point, |fact, _| {
                enqueue(&mut pending, &done, &mut lease, fact.source(), fact.source_point(), point, false)
            })?;
            if !present
                && !origin.visit_legacy_carrier_facts_v9(parent, b.params, |fact, _| {
                    enqueue(&mut pending, &done, &mut lease, fact.source(), fact.source_point(), point, false)
                })?
            {
                return Err(bad("ancestor native or immutable legacy input provenance missing"));
            }
        } else if descriptor.as_ref().is_none_or(|d| !matches!(d.origin, 3 | 4)) {
            return Err(bad("ancestor immutable source origin missing"));
        }
        if let Some(descriptor) = descriptor {
            let mut q = TirNativeRequestsV1::new();
            q.exact_v1::<BTreeNodeEnvelopeV1<Key, NativeAncestorArchiveDescriptorV19>>(1).map_err(PalwStateV2Error::NativeResource)?;
            grow(&mut lease, add(q.bytes_v1(), descriptor.clone_requests_v19().map_err(PalwStateV2Error::NativeResource)?)?)?;
            let (d, old_lease) = descriptor.into_parts_v1();
            done.insert(k, d);
            drop(old_lease);
        } else {
            let mut q = TirNativeRequestsV1::new();
            q.exact_v1::<BTreeNodeEnvelopeV1<Key, ()>>(1).map_err(PalwStateV2Error::NativeResource)?;
            grow(&mut lease, q.bytes_v1())?;
            ledger_seen.insert(k);
        }
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<NativeAncestorArchiveDescriptorV19>(done.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    grow(&mut lease, q.bytes_v1())?;
    let mut result = Vec::with_capacity(done.len());
    for (_, d) in done {
        result.push(d);
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(result, lease))
}
/// Owner-native closure query used by financial release preflight. The final
/// mutation still authenticates the executing point and every paid owner.
pub(super) fn native_source_archive_duties_closed_v19(
    s: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim: Hash64,
    point: Point,
    now: u64,
) -> Result<bool, PalwStateV2Error> {
    super::palw_claim_local_prefix_reservation_v1::native_source_archive_duties_closed_v19(s, params, claim, point, now)
}
pub(super) fn collect_ancestor_archive_after_lease_v19(
    b: &mut TransitionBuilder<'_>,
    descriptor: &NativeAncestorArchiveDescriptorV19,
) -> Result<(), PalwStateV2Error> {
    super::palw_claim_local_born_tir_source_v16::collect_ancestor_archive_after_lease_v19(b, descriptor)
}

impl NativeAncestorArchiveDescriptorV19 {
    pub(super) fn legacy_f1_literal_v19(&self) -> Option<RowValueV2> {
        (self.origin == 2).then(|| self.references.literals[0])
    }
    pub(super) fn with_native_legacy_origin_v19(
        &self,
        s: &PalwChainStateV2,
        params: &PalwStateParamsV2,
    ) -> Result<Option<super::palw_claim_local_source_origin_v8::SourceOriginV8>, PalwStateV2Error> {
        if self.origin != 2 {
            return Ok(None);
        }
        self.recheck_native_v19(s)?;
        let raw = legacy_body(s, self, self.references.literals[0])?;
        let frame = crate::palw_claim_local_v1::palw_decode_claim_liability_frame_v1(&raw).map_err(|e| match e {
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
            _ => bad("leased legacy F1 codec"),
        })?;
        super::palw_claim_local_source_origin_v8::SourceOriginV8::from_native_retained_legacy_frame_v19(
            s, params, self.claim, self.point, frame,
        )
    }
}
impl NativeAncestorArchiveDescriptorV19 {
    /// Exact paid immutable membership, never physical presence alone. This
    /// authenticates only the B row's storage credit, not computation or class policy.
    pub(super) fn proves_paid_immutable_body_v19(&self, s: &PalwChainStateV2, value: RowValueV2) -> Result<bool, PalwStateV2Error> {
        let Some(proof) = &self.paid_proof else { return Ok(false) };
        self.recheck_native_v19(s)?;
        proof.recheck_archived_v19(s)?;
        if self.references.literals.contains(&value) {
            return Ok(true);
        }
        let reader = StateNodesV2(s);
        let mut found = false;
        for root in &self.references.components {
            nodes::visit_nodes(&reader, *root, |_, node| {
                if let nodes::ContextNodeV2::Leaf { value: actual, .. } = node
                    && *actual == value
                {
                    reader.body(*actual)?;
                    found = true;
                }
                Ok(())
            })
            .map_err(map)?;
        }
        Ok(found)
    }
}
#[cfg(test)]
impl NativeAncestorArchiveDescriptorV19 {
    pub(super) fn assert_unpaid_same_body_refusal_v19(&self, s: &PalwChainStateV2, value: RowValueV2) -> Result<(), PalwStateV2Error> {
        let before = s.state_root();
        assert!(self.proves_paid_immutable_body_v19(s, value)?);
        let lease = reserve_native_requests_v1(self.clone_requests_v19().map_err(PalwStateV2Error::NativeResource)?)
            .map_err(PalwStateV2Error::NativeResource)?;
        let mut unpaid = self.clone();
        unpaid.paid_proof = None;
        unpaid.old_paid_until = None;
        // Keep exact immutable references and actual physical B unchanged.
        StateNodesV2(s).body(value).map_err(map)?;
        assert!(
            !unpaid.proves_paid_immutable_body_v19(s, value)?,
            "physical source body without verified paid proof grants no storage credit"
        );
        assert_eq!(s.state_root(), before);
        drop(unpaid);
        drop(lease);
        Ok(())
    }
}
