//! Actual producer material -> the newly signed complete provider codec. The
//! returned owner keeps construction reservations through signing and transport.
//! Neither an SDK draft nor a successful upload is native covenant acceptance.
use super::{PaidClaimMaterialWireV2, Refusal};
use borsh::BorshSerialize;
use kaspa_consensus_core::{
    Hash64,
    palw_kernel_parameter_material_v4::*,
    palw_material_capacity_v1::{PALW_MATERIAL_MAX_UNITS_V1, PalwMaterialManifestV1},
    palw_native_resource_v1::{NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_prospective_kernel_material_v4::*,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2},
};
use misaka_palw_kernel::{
    hash::Digest,
    ledger::{ClaimBodyV1, ClaimRowV1, KernelLedgerV1},
    public::TensorWireV1,
    route::SaltedCommitV1,
    spec::{
        SpecClaimV1, SpecClassKindV1,
        composite::{ComponentV1, StageClaimV1},
        retrieval::{EntryResponseV1, RetrievalRootV1, RetrievedV1, SnapshotDataV1},
    },
};
use std::{io, mem::size_of};
fn bad(e: impl ToString) -> Refusal {
    Refusal::new("PROVIDER_HANDOFF_V4", e.to_string())
}
fn add(a: u64, b: u64) -> Result<u64, Refusal> {
    a.checked_add(b).ok_or_else(|| bad("provider construction size overflow"))
}
fn mul(a: u64, b: u64) -> Result<u64, Refusal> {
    a.checked_mul(b).ok_or_else(|| bad("provider construction size overflow"))
}
#[derive(Clone, Copy, Debug)]
pub struct ProviderHandoffLimitsV4 {
    pub max_material_bytes: u64,
    pub max_workspace_bytes: u64,
    pub max_units: u32,
}
pub struct KernelProviderHandoffV4 {
    request: PalwProspectiveKernelMaterialRequestV4,
    // Ownership is intentionally not detachable from the request's lifetime.
    _candidate: kaspa_consensus_core::palw_kernel_producer_candidate_v4::PalwKernelProducerCandidateOwnedV4,
    _workspace: Option<NativeDemandLeaseV1>,
    _output: Option<NativeDemandLeaseV1>,
}
impl KernelProviderHandoffV4 {
    pub fn request(&self) -> &PalwProspectiveKernelMaterialRequestV4 {
        &self.request
    }
}
struct Output {
    lease: Option<NativeDemandLeaseV1>,
    bytes: u64,
    cap: u64,
}
impl Output {
    fn reserve(&mut self, n: u64) -> Result<(), Refusal> {
        self.bytes = add(self.bytes, n)?;
        if self.bytes > self.cap {
            return Err(bad("complete material exceeds explicit producer byte cap"));
        }
        if let Some(lease) = &mut self.lease {
            lease.grow_memory_v1(n).map_err(bad)?
        } else {
            self.lease = reserve_native_requests_v1(n).map_err(bad)?;
        }
        Ok(())
    }
    fn encode<T: BorshSerialize>(&mut self, value: &T) -> Result<Vec<u8>, Refusal> {
        let n = borsh::object_length(value).map_err(bad)? as u64;
        if n > PALW_KERNEL_MATERIAL_UNIT_BYTES_V4 {
            return Err(bad("unit exceeds complete codec carrier"));
        }
        self.reserve(n)?;
        let mut out = Vec::new();
        out.try_reserve_exact(usize::try_from(n).map_err(bad)?).map_err(bad)?;
        value.serialize(&mut out).map_err(bad)?;
        Ok(out)
    }
}
/// Decode + dual-tree/opening scratch from actual dtype and dimensions. V2's
/// row/column leaf lists and a replacement level coexist; V3 also clones one
/// selected tree while retaining both cached trees. Three complete leaf lists
/// bound those explicitly named native allocations, rather than codec bytes.
fn tensor_workspace(w: &TensorWireV1) -> Result<u64, Refusal> {
    let dtype = misaka_palw_kernel::plan::dtype_of_tag(w.dtype).ok_or_else(|| bad("unknown material dtype"))?;
    if w.shape.len() > misaka_palw_tir::types::MAX_RANK || w.shape.contains(&0) {
        return Err(bad("invalid material shape"));
    }
    let mut n = 1u64;
    for d in &w.shape {
        n = mul(n, *d)?;
    }
    if mul(n, dtype.width() as u64)? != w.bytes.len() as u64 {
        return Err(bad("tensor byte length differs from declared shape"));
    }
    let (rows, row_len, cols, col_len) = if w.shape.len() <= 1 {
        (1, n, 1, n)
    } else {
        let m = w.shape[w.shape.len() - 2];
        let last = w.shape[w.shape.len() - 1];
        (n / last, last, n / m, m)
    };
    let row_leaves = mul(rows, row_len.div_ceil(misaka_palw_kernel::merkle3::TILE_V3))?;
    let col_leaves = mul(cols, col_len.div_ceil(misaka_palw_kernel::merkle3::TILE_V3))?;
    // Both suites are supported: retain the larger native leaf layout.
    let leaves = add(rows, cols)?.max(add(row_leaves, col_leaves)?);
    let shape = mul(w.shape.len() as u64, (size_of::<usize>() + size_of::<u64>()) as u64)?;
    let tensor = add(size_of::<misaka_palw_tir::Tensor>() as u64, add(shape, mul(n, 16)?)?)?;
    let opening = add(mul(row_len, 16)?, add(shape, mul(misaka_palw_kernel::merkle::depth(leaves), 64)?)?)?;
    add(tensor, add(mul(leaves, 3 * 64)?, opening)?)
}
fn workspace(material: &PaidClaimMaterialWireV2) -> Result<u64, Refusal> {
    let mut total = 0u64;
    for wire in material.nodes.values().chain(material.inputs.values()).chain(material.params.values()) {
        total = add(total, tensor_workspace(wire)?)?;
    }
    for position in material.segmented_positions.values() {
        for occurrence in position {
            for wire in occurrence {
                total = add(total, tensor_workspace(wire)?)?;
            }
        }
    }
    // SnapshotData's item copy, persistent leaves, root/path old+new levels,
    // and a complete slice's cloned item/path payloads may coexist. No item is
    // decoded or copied until this actual-item bound is reserved.
    let items = material.snapshot_items.len() as u64;
    let depth = misaka_palw_kernel::merkle::depth(items.max(1));
    for item in material.snapshot_items.values() {
        total = add(total, mul((item.key.len() + item.payload.len()) as u64, 32)?)?;
        total =
            add(total, (2 * size_of::<misaka_palw_kernel::spec::retrieval::RetrievalItemV1>() + 3 * size_of::<Vec<Digest>>()) as u64)?;
        total = add(total, add(3 * 64, mul(depth, 64)?)?)?;
    }
    Ok(total)
}
struct BorrowedPosition<'a> {
    material: &'a PaidClaimMaterialWireV2,
    stage: u8,
    position: u32,
    committed: &'a [Vec<Digest>],
    inputs: &'a [Digest],
    derived: &'a [Vec<bool>],
}
impl BorshSerialize for BorrowedPosition<'_> {
    fn serialize<W: io::Write>(&self, w: &mut W) -> io::Result<()> {
        let missing = || io::Error::new(io::ErrorKind::InvalidData, "missing exact producer position material");
        u32::try_from(self.committed.len()).map_err(io::Error::other)?.serialize(w)?;
        for (o, nodes) in self.committed.iter().enumerate() {
            u32::try_from(nodes.len()).map_err(io::Error::other)?.serialize(w)?;
            for n in 0..nodes.len() {
                if self.derived.get(o).and_then(|r| r.get(n)).copied().unwrap_or(false) {
                    2u8.serialize(w)?;
                } else {
                    0u8.serialize(w)?;
                    self.material.nodes.get(&(self.stage, self.position, o as u16, n as u16)).ok_or_else(missing)?.serialize(w)?;
                }
            }
        }
        u32::try_from(self.inputs.len()).map_err(io::Error::other)?.serialize(w)?;
        for i in 0..self.inputs.len() {
            0u8.serialize(w)?;
            self.material.inputs.get(&(self.stage, self.position, i as u16)).ok_or_else(missing)?.serialize(w)?;
        }
        Ok(())
    }
}
fn retrieval<'a>(
    ledger: &'a KernelLedgerV1,
    row: &'a ClaimRowV1,
    component: u8,
) -> Result<(&'a RetrievalRootV1, &'a [RetrievedV1]), Refusal> {
    let ClaimBodyV1::Spec(body) = &row.body else { return Err(bad("not a retrieval claim")) };
    let class = ledger.typed.classes.get(&row.class_binding_id).ok_or_else(|| bad("retrieval class absent"))?;
    match (&class.kind, &body.claim) {
        (SpecClassKindV1::Retrieval { root }, SpecClaimV1::Retrieval(claim)) if component == 0 => Ok((root, &claim.result)),
        (SpecClassKindV1::Composite { components, .. }, SpecClaimV1::Composite(claim)) => {
            match (components.get(component as usize), claim.stages.get(component as usize)) {
                (Some(ComponentV1::Retrieval(root)), Some(StageClaimV1::Retrieval { result, .. })) => Ok((root, result)),
                _ => Err(bad("retrieval component mismatch")),
            }
        }
        _ => Err(bad("retrieval class kind mismatch")),
    }
}
fn snapshot(material: &PaidClaimMaterialWireV2, root: &RetrievalRootV1) -> Result<SnapshotDataV1, Refusal> {
    let key = root.snapshot.root();
    let available = material.snapshot_items.range((key, 0)..=(key, u64::MAX)).count() as u64;
    if available != root.snapshot.items {
        return Err(bad("full snapshot count differs before allocation"));
    }
    // Check complete membership before reserving the class-declared count.
    for id in 0..root.snapshot.items {
        if !material.snapshot_items.contains_key(&(key, id)) {
            return Err(bad("incomplete snapshot before allocation"));
        }
    }
    let mut items = Vec::new();
    items.try_reserve_exact(usize::try_from(root.snapshot.items).map_err(bad)?).map_err(bad)?;
    for id in 0..root.snapshot.items {
        items.push(material.snapshot_items.get(&(root.snapshot.root(), id)).ok_or_else(|| bad("incomplete full snapshot"))?.clone());
    }
    let data = SnapshotDataV1::new(items, root.snapshot.dim, root.snapshot.max_payload, root.snapshot.slice_items).map_err(bad)?;
    if data.snapshot != root.snapshot {
        return Err(bad("snapshot differs from native registered root"));
    }
    Ok(data)
}
/// All four actual producer targets use one new complete codec. Existing V1
/// positions/parts codecs remain unchanged and are not relabelled complete.
/// `commit` and `providers` are moved from their already-owned caller; no copy of
/// their pre-existing allocation is attributed to this construction ticket.
#[allow(clippy::too_many_arguments)]
pub fn build_kernel_provider_handoff_v4(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    checkpoint: Hash64,
    commit_daa: u64,
    expires_daa: u64,
    producer: PalwBondKeyV2,
    providers: Vec<PalwBondKeyV2>,
    commit: SaltedCommitV1,
    salt: Digest,
    material: &PaidClaimMaterialWireV2,
    limits: ProviderHandoffLimitsV4,
    raw_depth: Option<u64>,
    signer: &dyn kaspa_pq_validator_core::MessageSigner,
) -> Result<KernelProviderHandoffV4, Refusal> {
    let owned = kaspa_consensus_core::palw_kernel_producer_candidate_v4::kernel_producer_candidate_v4(
        state,
        params,
        checkpoint,
        commit_daa,
        producer,
        &commit,
        &salt,
        commit_daa,
        expires_daa,
        raw_depth,
    )
    .map_err(bad)?;
    build_kernel_provider_handoff_from_candidate_v4(owned, providers, commit, salt, material, limits, signer)
}
/// Use the opaque grant returned by the producer's OWN Consensus API; no raw
/// peer state/key/collateral projection is accepted by this interface.
pub fn build_kernel_provider_handoff_from_candidate_v4(
    mut owned: kaspa_consensus_core::palw_kernel_producer_candidate_v4::PalwKernelProducerCandidateOwnedV4,
    providers: Vec<PalwBondKeyV2>,
    commit: SaltedCommitV1,
    salt: Digest,
    material: &PaidClaimMaterialWireV2,
    limits: ProviderHandoffLimitsV4,
    signer: &dyn kaspa_pq_validator_core::MessageSigner,
) -> Result<KernelProviderHandoffV4, Refusal> {
    owned.validate_work(&commit, &salt).map_err(bad)?;
    if signer.public_key() != owned.producer_pubkey()
        || providers.len() < 2
        || providers.len() > kaspa_consensus_core::palw_monetary_material_v1::PALW_MONETARY_MATERIAL_MAX_PROVIDERS_V1
        || providers.windows(2).any(|p| p[0] >= p[1])
    {
        return Err(bad("native producer key/roster invalid"));
    }
    let scratch = workspace(material)?;
    if scratch > limits.max_workspace_bytes {
        return Err(bad("producer decoded/Merkle workspace exceeds explicit cap"));
    }
    let workspace_lease = reserve_native_requests_v1(scratch).map_err(bad)?;
    let (claim, _, _) = commit_identity_v4(&commit);
    owned.reserve_material_v4().map_err(bad)?;
    let candidate = owned.ledger();
    let row = candidate.claims.get(&claim).ok_or_else(|| bad("actual candidate absent"))?;
    let mut ranges = [0u32; 256];
    let count = typed_kernel_unit_ranges_v4(&candidate, &claim, |s, n| {
        let at = &mut ranges[s as usize];
        if *at != 0 {
            return Err("duplicate complete unit namespace".into());
        }
        *at = n;
        Ok(())
    })
    .map_err(bad)?;
    if count > u64::from(limits.max_units) || count > PALW_MATERIAL_MAX_UNITS_V1 as u64 {
        return Err(bad("complete unit cap exceeded"));
    }
    let mut output = Output { lease: None, bytes: 0, cap: limits.max_workspace_bytes };
    output.reserve(add(68, mul(count, (size_of::<(u8, u32, Vec<u8>)>() + 9) as u64)?)?)?;
    let mut units = Vec::new();
    units.try_reserve_exact(usize::try_from(count).map_err(bad)?).map_err(bad)?;
    let mut cached_snapshot: Option<SnapshotDataV1> = None;
    for (stage, range) in ranges.into_iter().enumerate() {
        let stage = stage as u8;
        for position in 0..range {
            let bytes = if stage == PALW_KERNEL_PARAMETER_STAGE_V4 {
                let (spec, at) = kernel_parameter_at_v4(&candidate, &claim, position).map_err(bad)?;
                let wire = material
                    .params
                    .get(&(spec.program, spec.parameter, spec.layer))
                    .ok_or_else(|| bad("missing immutable artifact"))?;
                let tensor = wire.decode().map_err(bad)?;
                let opening = match spec.suite {
                    2 => PalwKernelParameterOpeningV4::RowV2(
                        misaka_palw_kernel::merkle::TensorOpeningV1::row(&tensor, at).ok_or_else(|| bad("native row absent"))?,
                    ),
                    3 => PalwKernelParameterOpeningV4::TileV3(
                        misaka_palw_kernel::merkle3::LeafOpeningV3::of(
                            &tensor,
                            misaka_palw_kernel::merkle::AXIS_ROW,
                            at / spec.row_tiles,
                            at % spec.row_tiles,
                        )
                        .ok_or_else(|| bad("native tile absent"))?,
                    ),
                    _ => return Err(bad("unsupported actual parameter commitment")),
                };
                output.encode(&PalwKernelParameterRowV4 {
                    program: spec.program,
                    parameter: spec.parameter,
                    layer: spec.layer,
                    row: at,
                    opening,
                })?
            } else if stage == PALW_KERNEL_SEGMENTED_PART_STAGE_V4 {
                let (p, part) = kernel_segmented_at_v4(&candidate, &claim, position).map_err(bad)?;
                let class = candidate.classes.get(&row.class_binding_id).ok_or_else(|| bad("segmented class absent"))?;
                use misaka_palw_kernel::element::SegMaterialV1;
                let values = material.position(p).ok_or_else(|| bad("segmented material absent"))?;
                let wiring = misaka_palw_kernel::trace::WiringV1::new(&class.program).map_err(bad)?;
                if values.len() != wiring.occurrences.len() {
                    return Err(bad("segmented occurrence count mismatch"));
                }
                for (o, nodes) in values.iter().enumerate() {
                    let block = wiring.occurrences[o].0 as usize;
                    if nodes.len() != class.program.blocks[block].nodes.len() {
                        return Err(bad("segmented node count mismatch"));
                    }
                    for (n, tensor) in nodes.iter().enumerate() {
                        let declared = wiring.node(o as u16, n as u16);
                        if tensor.dtype != declared.out.dtype || tensor.shape != declared.out.resolve(wiring.h(o as u16, p)) {
                            return Err(bad("segmented tensor differs from actual native dimensions"));
                        }
                    }
                }
                let siblings = material.position_siblings(p).ok_or_else(|| bad("segmented siblings absent"))?;
                let response =
                    misaka_palw_kernel::seg_da::position_part_v1(&class.program, p, &values, siblings, part).map_err(bad)?;
                let inner = output.encode(&response)?;
                // The inner allocation is temporarily live with the outer frame;
                // both exact byte vectors are admitted before construction.
                output.encode(&PalwKernelSegmentedPartV4 { position: p, part, bytes: inner })?
            } else if let Some((committed, inputs)) = row.body.position(stage, position) {
                let derived = candidate.derived_mask(row, stage);
                output.encode(&BorrowedPosition { material, stage, position, committed, inputs, derived: &derived })?
            } else {
                let entry = stage >= misaka_palw_kernel::spec::RETRIEVAL_ENTRY_STAGE_BASE_V1;
                let base = if entry {
                    misaka_palw_kernel::spec::RETRIEVAL_ENTRY_STAGE_BASE_V1
                } else {
                    misaka_palw_kernel::spec::SNAPSHOT_STAGE_BASE_V1
                };
                let (root, result) =
                    retrieval(&candidate, row, stage.checked_sub(base).ok_or_else(|| bad("unknown native unit namespace"))?)?;
                if cached_snapshot.as_ref().is_none_or(|s| s.snapshot != root.snapshot) {
                    drop(cached_snapshot.take());
                    cached_snapshot = Some(snapshot(material, root)?);
                }
                let data = cached_snapshot.as_ref().ok_or_else(|| bad("snapshot absent"))?;
                if entry {
                    let selected = result.get(position as usize).ok_or_else(|| bad("selected entry absent"))?;
                    let (item, path) = data.open(selected.id).ok_or_else(|| bad("snapshot selected entry absent"))?;
                    output.encode(&EntryResponseV1 { item, path })?
                } else {
                    // Native helper allocates its output: reserve its exact
                    // signed slice maximum before invoking it, then account
                    // the actual frame separately in the retained output owner.
                    let range = root.snapshot.slice_range(u64::from(position)).ok_or_else(|| bad("snapshot slice absent"))?;
                    let count = range.end - range.start;
                    let max = add(
                        8,
                        mul(
                            count,
                            add(
                                16 + u64::from(root.snapshot.dim) * 4 + u64::from(root.snapshot.max_payload) * 4,
                                mul(misaka_palw_kernel::merkle::depth(root.snapshot.items), 64)?,
                            )?,
                        )?,
                    )?;
                    output.reserve(max)?;
                    data.slice_response(u64::from(position)).ok_or_else(|| bad("snapshot response absent"))?
                }
            };
            units.push((stage, position, bytes));
        }
    }
    let bundle =
        PalwProspectiveKernelBundleV4::TypedKernelV4(PalwTypedKernelMaterialBundleV4 { claim: Hash64::from_bytes(claim), units });
    let decode = kaspa_consensus_core::palw_material_capacity_v1::prospective_kernel_bundle_requests_v4(&bundle).map_err(bad)?;
    let _decode = reserve_native_requests_v1(decode.bytes().ok_or_else(|| bad(decode.to_string()))?).map_err(bad)?;
    let auth = kaspa_consensus_core::palw_material_capacity_v1::prospective_kernel_bundle_auth_requests_v4(&bundle, candidate)
        .map_err(bad)?;
    let _auth = reserve_native_requests_v1(auth.bytes().ok_or_else(|| bad(auth.to_string()))?).map_err(bad)?;
    let scope = bundle.verify(candidate, limits.max_material_bytes).map_err(bad)?;
    output.reserve(
        (size_of::<PalwMaterialManifestV1>() + size_of::<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialScopeV1>())
            as u64,
    )?;
    let manifest = PalwMaterialManifestV1 { root: Hash64::from_bytes(claim), scopes: vec![scope] };
    let intent = PalwProspectiveKernelMaterialIntentV4 {
        version: 4,
        checkpoint: owned.checkpoint(),
        ledger_root: owned.ledger_root(),
        aux_root: owned.aux_root(),
        commit_daa: owned.commit_daa(),
        expires_daa: owned.expires_daa(),
        producer: owned.producer(),
        providers,
        commit,
        salt,
        manifest,
    };
    output.reserve(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)?;
    let message = intent.signing_message(owned.network());
    let signature = signer.sign_message(message.as_byte_slice(), PALW_PROSPECTIVE_KERNEL_MATERIAL_CONTEXT_V4).map_err(bad)?;
    if signature.len() != kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
        return Err(bad("producer signer returned wrong signature length"));
    }
    Ok(KernelProviderHandoffV4 {
        request: PalwProspectiveKernelMaterialRequestV4 { intent, producer_signature: signature, bundle },
        _candidate: owned,
        _workspace: workspace_lease,
        _output: output.lease,
    })
}

// Exact SDK-owned file grammar: nested collection lengths are scanned before
// Borsh may allocate. File byte caps alone do not authorize native heap demand.
impl kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1 for PaidClaimMaterialWireV2 {
    fn scan_native_wire_v1<R: io::Read>(
        r: &mut kaspa_consensus_core::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> io::Result<kaspa_consensus_core::palw_native_resource_v1::NativeWireShapeV1> {
        use kaspa_consensus_core::palw_native_resource_v1::{NativeWireScanV1, NativeWireShapeV1};
        use std::collections::BTreeMap;
        r.step()?;
        let mut shape = NativeWireShapeV1::default();
        macro_rules! field {
            ($t:ty) => {
                shape = shape.merge(<$t>::scan_native_wire_v1(r)?)?;
            };
        }
        field!(BTreeMap<(u8,u32,u16,u16),TensorWireV1>);
        field!(BTreeMap<(u8,u32,u16),TensorWireV1>);
        field!(BTreeMap<(u16, u16, Option<u16>), TensorWireV1>);
        field!(BTreeMap<u32,Vec<Vec<TensorWireV1>>>);
        field!(BTreeMap<u32,Vec<Digest>>);
        field!(Vec<u32>);
        field!(BTreeMap<(Digest,u64),misaka_palw_kernel::spec::retrieval::RetrievalItemV1>);
        Ok(shape)
    }
}
impl kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1 for super::private_bootstrap::ProgramPaidBootstrapWorkV3 {
    fn scan_native_wire_v1<R: io::Read>(
        r: &mut kaspa_consensus_core::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> io::Result<kaspa_consensus_core::palw_native_resource_v1::NativeWireShapeV1> {
        use kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1;
        r.step()?;
        <(u16, Digest)>::scan_native_wire_v1(r)?
            .merge(SaltedCommitV1::scan_native_wire_v1(r)?)?
            .merge(PaidClaimMaterialWireV2::scan_native_wire_v1(r)?)?
            .merge(<(u128, u128, u128)>::scan_native_wire_v1(r)?)
    }
}
