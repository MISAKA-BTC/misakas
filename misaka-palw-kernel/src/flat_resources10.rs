//! Explicit reference-resource vector and a prepaid streaming consumer. This
//! component preserves frozen TIR/commitments and grants no native execution,
//! model availability, reward or Final rights. Quantities are bounded payload,
//! operation and container allowances, not measured CPU time or process RSS.
//! An arbitrary provider may do unrelated work; the owned raw-byte decoder
//! below bounds its own allocation before expanding a borrowed image to i128.
use crate::{
    element::PreparedV1,
    flat_court5::flat_profile_commitment_v5,
    flat_court9::complete_scope_api_bounds_v9,
    flat_scope9::{FlatResourceScopeV9, flat_resource_scope_v9},
    hash::{Digest, object_id},
    merkle3::{LayoutV3, tensor_commitment_v3},
    merkle5::flat_material_bounds_v5,
    seg::SegmentedCommitmentsV1,
    seg_encoder::EncoderBindingV1,
    trace::{ParamCommitmentsV1, trace_streaming_v1},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{
    DType, ParamSource, Prim, Tensor,
    program::{Ref, TirProgramV1},
};
use std::{cell::Cell, collections::BTreeMap};

pub const RESOURCE_DIMENSIONS_V10: usize = 21;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ResourceAxisV10 {
    Macs = 0,
    ScalarSteps,
    IndexSteps,
    ValidatedElements,
    CopiedReferenceBytes,
    ParameterHashBytes,
    TraceHashBytes,
    ParameterCalls,
    ReferenceTensorPayload,
    ContainerAllocatorAllowance,
    StoredEvidenceBytes,
    DeliveryBytes,
    ReplyCalls,
    Attempts,
    CourtWork,
    CourtBytes,
    CourtScratchBytes,
    MetadataWork,
    RawParameterStorage,
    ParameterTreeScratch,
    OwnedPeakBytes,
}
impl ResourceAxisV10 {
    pub const ALL: [Self; RESOURCE_DIMENSIONS_V10] = [
        Self::Macs,
        Self::ScalarSteps,
        Self::IndexSteps,
        Self::ValidatedElements,
        Self::CopiedReferenceBytes,
        Self::ParameterHashBytes,
        Self::TraceHashBytes,
        Self::ParameterCalls,
        Self::ReferenceTensorPayload,
        Self::ContainerAllocatorAllowance,
        Self::StoredEvidenceBytes,
        Self::DeliveryBytes,
        Self::ReplyCalls,
        Self::Attempts,
        Self::CourtWork,
        Self::CourtBytes,
        Self::CourtScratchBytes,
        Self::MetadataWork,
        Self::RawParameterStorage,
        Self::ParameterTreeScratch,
        Self::OwnedPeakBytes,
    ];
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct ResourceQuantitiesV10(pub [u128; RESOURCE_DIMENSIONS_V10]);
impl Default for ResourceQuantitiesV10 {
    fn default() -> Self {
        Self([0; RESOURCE_DIMENSIONS_V10])
    }
}
impl ResourceQuantitiesV10 {
    pub fn get(&self, axis: ResourceAxisV10) -> u128 {
        self.0[axis as usize]
    }
    pub fn set(&mut self, axis: ResourceAxisV10, value: u128) {
        self.0[axis as usize] = value;
    }
    fn add_axis(&mut self, axis: ResourceAxisV10, value: u128) -> Result<(), String> {
        self.0[axis as usize] = add(self.get(axis), value)?;
        Ok(())
    }
    pub fn checked_add(&self, other: &Self) -> Result<Self, String> {
        let mut out = Self::default();
        for axis in ResourceAxisV10::ALL {
            out.set(axis, add(self.get(axis), other.get(axis))?);
        }
        Ok(out)
    }
    pub fn within(&self, cap: &Self) -> Result<(), ResourceAxisV10> {
        for axis in ResourceAxisV10::ALL {
            if self.get(axis) > cap.get(axis) {
                return Err(axis);
            }
        }
        Ok(())
    }
}
fn add(a: u128, b: u128) -> Result<u128, String> {
    a.checked_add(b).ok_or_else(|| "reference resource addition overflow".into())
}
fn mul(a: u128, b: u128) -> Result<u128, String> {
    a.checked_mul(b).ok_or_else(|| "reference resource multiplication overflow".into())
}
fn elements(shape: &[usize]) -> Result<u128, String> {
    shape.iter().try_fold(1u128, |n, d| mul(n, *d as u128))
}
fn v3_hash(shape: &[usize], dtype: DType) -> Result<(u128, u128), String> {
    let l = LayoutV3::try_of(shape).ok_or("invalid v3 resource shape")?;
    let leaves = add(l.leaves(0) as u128, l.leaves(1) as u128)?;
    Ok((add(add(mul(l.len as u128, 2 * dtype.width() as u128)?, mul(leaves, 512)?)?, 1024)?, add(mul(leaves, 192)?, 131072)?))
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PrimitiveResourcesV10 {
    pub occurrence: u16,
    pub node: u16,
    pub prim_tag: u8,
    pub output_elements: u128,
    pub input_elements: u128,
    pub macs: u128,
    pub scalar_steps: u128,
    pub index_steps: u128,
    pub validated_elements: u128,
    pub copied_reference_bytes: u128,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatResourceVectorV10 {
    pub version: u16,
    pub program: Digest,
    pub parameters: Digest,
    pub court_scope: Digest,
    pub positions: u32,
    pub encoder: Option<(u16, u32)>,
    pub primitives: Vec<PrimitiveResourcesV10>,
    pub quantities: ResourceQuantitiesV10,
}
impl FlatResourceVectorV10 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/reference-resource-vector/v10", self)
    }
}

/// Fixed resource records contain no tensor values. Bound the complete frame
/// before hashing or allocating the primitive vector. The count comes from an
/// independently authenticated native class header, never a carrier's length.
pub const FLAT_RESOURCE_VECTOR_MAX_BYTES_V10: usize = 8 << 20;
pub const FLAT_RESOURCE_PRIMITIVE_BYTES_V10: usize = 2 + 2 + 1 + 7 * 16;
const FLAT_RESOURCE_FIXED_MAX_BYTES_V10: usize = 2 + 3 * 64 + 4 + 1 + 6 + 4 + RESOURCE_DIMENSIONS_V10 * 16;

pub fn flat_resource_vector_max_bytes_v10(primitives: usize) -> Option<usize> {
    if primitives as u128 > crate::flat_scope9::MAX_SCOPE_RELATION_VISITS_V9 {
        return None;
    }
    let bytes = primitives.checked_mul(FLAT_RESOURCE_PRIMITIVE_BYTES_V10)?.checked_add(FLAT_RESOURCE_FIXED_MAX_BYTES_V10)?;
    (bytes <= FLAT_RESOURCE_VECTOR_MAX_BYTES_V10).then_some(bytes)
}

/// Additional native fee for the bounded resource row: raw copy/root hashing,
/// fixed scalar decoding and coordinate checks. This is separate from the
/// candidate9 complete-scope restore tariff and must be paid before that row is
/// copied/decoded. It is not a resource derivation or an eligibility grant.
pub fn flat_resource_vector_restore_work_v10(primitives: usize) -> Option<u64> {
    u64::try_from(flat_resource_vector_max_bytes_v10(primitives)?)
        .ok()?
        .checked_mul(64)?
        .checked_add(u64::try_from(primitives).ok()?.checked_mul(512)?)?
        .checked_add(65536)
}

/// Decode only an original native-anchored admitted resource record. Public
/// import still independently derives every quantity with derive_flat_resources.
/// An expected root supplied by the same untrusted body is not an authority.
pub fn decode_rooted_flat_resources_v10(
    bytes: &[u8],
    expected_root: Digest,
    primitives: usize,
) -> Result<FlatResourceVectorV10, String> {
    let max = flat_resource_vector_max_bytes_v10(primitives).ok_or("resource frame count exceeds fixed byte cap")?;
    if bytes.len() > max || bytes.len() < FLAT_RESOURCE_FIXED_MAX_BYTES_V10 - 6 {
        return Err("resource frame exceeds its authenticated primitive-count bound".into());
    }
    // Exact canonical Borsh bytes hash directly: no duplicate serialization or
    // primitive allocation before independent native-root authentication.
    if crate::hash::id(b"misaka-palw/kernel/reference-resource-vector/v10", bytes) != expected_root {
        return Err("resource frame differs from its independent native root".into());
    }
    fn read<T: BorshDeserialize>(r: &mut std::io::Cursor<&[u8]>) -> Result<T, String> {
        T::deserialize_reader(r).map_err(|_| "truncated fixed resource frame".into())
    }
    let mut r = std::io::Cursor::new(bytes);
    let version = read::<u16>(&mut r)?;
    let program = read::<Digest>(&mut r)?;
    let parameters = read::<Digest>(&mut r)?;
    let court_scope = read::<Digest>(&mut r)?;
    let positions = read::<u32>(&mut r)?;
    let encoder = match read::<u8>(&mut r)? {
        0 => None,
        1 => Some((read::<u16>(&mut r)?, read::<u32>(&mut r)?)),
        _ => return Err("noncanonical resource encoder tag".into()),
    };
    let count = read::<u32>(&mut r)? as usize;
    let rest = count
        .checked_mul(FLAT_RESOURCE_PRIMITIVE_BYTES_V10)
        .and_then(|n| n.checked_add(RESOURCE_DIMENSIONS_V10 * 16))
        .ok_or("resource primitive frame overflow")?;
    if version != 10 || positions == 0 || count != primitives || bytes.len() - r.position() as usize != rest {
        return Err("resource primitive count or exact fixed frame differs from its native header".into());
    }
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        rows.push(PrimitiveResourcesV10 {
            occurrence: read(&mut r)?,
            node: read(&mut r)?,
            prim_tag: read(&mut r)?,
            output_elements: read(&mut r)?,
            input_elements: read(&mut r)?,
            macs: read(&mut r)?,
            scalar_steps: read(&mut r)?,
            index_steps: read(&mut r)?,
            validated_elements: read(&mut r)?,
            copied_reference_bytes: read(&mut r)?,
        });
    }
    let mut quantities = ResourceQuantitiesV10::default();
    for axis in ResourceAxisV10::ALL {
        quantities.set(axis, read(&mut r)?);
    }
    Ok(FlatResourceVectorV10 { version, program, parameters, court_scope, positions, encoder, primitives: rows, quantities })
}
pub struct FlatResourcePlanV10 {
    pub vector: FlatResourceVectorV10,
    pub court: FlatResourceScopeV9,
}
/// Derive all twenty-one dimensions from a canonical typed program, exact extent
/// and exact parameter namespace. The independently derived full v9 scope
/// binds the program/court set; a standalone point-price root is insufficient.
pub fn derive_flat_resources_v10(
    program: &TirProgramV1,
    pc: &ParamCommitmentsV1,
    positions: u32,
    encoder: Option<&EncoderBindingV1>,
) -> Result<FlatResourcePlanV10, String> {
    crate::flat_court4::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64).map_err(|e| e.to_string())?;
    misaka_palw_tir::validate::validate(program).map_err(|e| e.to_string())?;
    let real = encoder.map(|e| e.first_input as usize).unwrap_or(program.params.len());
    if real > program.params.len() {
        return Err("resource encoder namespace exceeds declarations".into());
    }
    let count = program.params[..real]
        .iter()
        .try_fold(0usize, |n, p| n.checked_add(if p.per_layer { program.schedule.layers.len() } else { 1 }))
        .ok_or("resource parameter count overflow")?;
    if count != pc.by_instance.len() {
        return Err("resource parameter namespace count mismatch".into());
    }
    for (j, p) in program.params[..real].iter().enumerate() {
        if p.per_layer {
            for l in 0..program.schedule.layers.len() {
                if !pc.by_instance.contains_key(&(j as u16, Some(l as u16))) {
                    return Err("resource per-layer parameter missing".into());
                }
            }
        } else if !pc.by_instance.contains_key(&(j as u16, None)) {
            return Err("resource global parameter missing".into());
        }
    }
    let court = flat_resource_scope_v9(program, positions, encoder).map_err(|e| format!("{e:?}"))?;
    let pr = PreparedV1::without_scope(program, encoder)?;
    let p = positions - 1;
    let mut q = ResourceQuantitiesV10::default();
    let mut primitives = Vec::new();
    let (mut maximum_inputs, mut maximum_scratch, mut history_payload, mut history_headers, mut tree_scratch) =
        (0u128, 0u128, 0u128, 0u128, 0u128);
    for (s, (b, _)) in pr.w.occurrences.iter().enumerate() {
        for (n, node) in program.blocks[*b as usize].nodes.iter().enumerate() {
            let sh = pr.shapes(p, s as u16, n as u16);
            let out = elements(&sh.out)?;
            let input = sh.inputs.iter().try_fold(0u128, |a, s| add(a, elements(s)?))?;
            // Typed operands plus the committed output value scan.
            let mut validated = add(input, out)?;
            let mut copied = mul(add(input, out)?, 16)?;
            let (mut macs, mut scalar) = (0u128, mul(out, 16)?);
            match node.prim {
                Prim::MatMul => {
                    macs = mul(out, sh.inputs[0][sh.inputs[0].len() - 1] as u128)?;
                    scalar = mul(macs, 16)?;
                }
                Prim::ReduceSum { .. } | Prim::ReduceMax { .. } => scalar = mul(input, 16)?,
                Prim::TopK { axis, k } => {
                    let h = sh.inputs[0][axis as usize] as u128;
                    let rows = input / h;
                    let k = k as u128;
                    // Heap/partition selection and sorted selected ids, separately
                    // from canonical PWU's N*K price and full typed-input scans.
                    let bits = 128 - k.leading_zeros() as u128;
                    scalar = mul(rows, mul(add(h, k)?, add(mul(bits, 4)?, 16)?)?)?;
                    maximum_scratch = maximum_scratch.max(mul(k, 64)?);
                }
                Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => scalar = mul(out, 512)?,
                Prim::HistAppend { .. } => {
                    validated = add(validated, out)?;
                    copied = add(copied, mul(out, 32)?)?;
                    history_payload = add(history_payload, mul(out, 16)?)?;
                    history_headers = add(history_headers, sh.hist.ok_or("missing history geometry")?.0 as u128)?;
                }
                Prim::StateWrite { .. } => validated = add(validated, out)?,
                _ => {}
            }
            // Resolved parameter images are decoded/checked, then checked again by
            // typed trace resolution, in addition to eval_primitive's input scan.
            for r in &node.inputs {
                if let Ref::Param(i) = r {
                    let d = &program.params[*i as usize];
                    let shape: Vec<_> = d.shape.iter().map(|d| *d as usize).collect();
                    let cells = elements(&shape)?;
                    validated = add(validated, mul(cells, 2)?)?;
                    if encoder.is_none_or(|e| *i < e.first_input) {
                        let (hash, scratch) = v3_hash(&shape, d.dtype)?;
                        q.add_axis(ResourceAxisV10::ParameterCalls, positions as u128)?;
                        q.add_axis(ResourceAxisV10::ParameterHashBytes, mul(hash, positions as u128)?)?;
                        tree_scratch = tree_scratch.max(scratch);
                    }
                }
            }
            // Row-major coordinate maps, gather indices and rank<=4 temporary
            // coordinates. This bounds reference index traffic, not a device kernel.
            let indices = add(mul(add(out, input)?, add(64, node.inputs.len() as u128 * 16)?)?, mul(macs, 16)?)?;
            maximum_inputs = maximum_inputs.max(mul(input, 16)?);
            maximum_scratch = maximum_scratch.max(add(mul(out, 40)?, 4096)?);
            let values = if out >= 64 {
                flat_material_bounds_v5(node.out.dtype, &sh.out.iter().map(|d| *d as u64).collect::<Vec<_>>())
                    .map(|m| m.hash_input_bytes)
                    .ok_or("invalid resource material")?
            } else {
                v3_hash(&sh.out, node.out.dtype)?.0
            };
            // Earlier H can still be a small dual-root commitment.
            let hash = values.max(2 * 63 * node.out.dtype.width() as u128 + 126 * 512 + 1024);
            q.add_axis(ResourceAxisV10::TraceHashBytes, mul(hash, positions as u128)?)?;
            for (axis, value) in [
                (ResourceAxisV10::Macs, macs),
                (ResourceAxisV10::ScalarSteps, scalar),
                (ResourceAxisV10::IndexSteps, indices),
                (ResourceAxisV10::ValidatedElements, validated),
                (ResourceAxisV10::CopiedReferenceBytes, copied),
            ] {
                q.add_axis(axis, mul(value, positions as u128)?)?;
            }
            primitives.push(PrimitiveResourcesV10 {
                occurrence: s as u16,
                node: n as u16,
                prim_tag: node.prim.tag(),
                output_elements: out,
                input_elements: input,
                macs,
                scalar_steps: scalar,
                index_steps: indices,
                validated_elements: validated,
                copied_reference_bytes: copied,
            });
        }
    }
    let nodes = pr.node_count as u128;
    let positions128 = positions as u128;
    // Two reference positions, resolved operands, history rows/prior clones and
    // primitive maps/results. The mapped provider's external RSS is not here.
    q.set(
        ResourceAxisV10::ReferenceTensorPayload,
        add(
            add(mul(court.reference_tensor_bytes_per_position, 2)?, maximum_inputs)?,
            add(mul(history_payload, 2)?, maximum_scratch)?,
        )?,
    );
    q.set(ResourceAxisV10::ParameterTreeScratch, tree_scratch);
    let tensor_headers = mul(add(mul(nodes, 2)?, mul(history_headers, 2)?)?, 128)?;
    let metadata = add(mul(program.encode().len() as u128, 64)?, mul(add(nodes, pr.w.occurrences.len() as u128)?, 1024)?)?;
    q.set(ResourceAxisV10::ContainerAllocatorAllowance, add(add(tensor_headers, metadata)?, 16384)?);
    // Commitment vectors and position roots are retained by this consumer;
    // serving all values requires a separate bounded store/DA consumer.
    let stored = add(
        court.node_commitments_bytes_per_claim,
        mul(add(mul(pr.w.occurrences.len() as u128, positions128)?, positions128)?, 128)?,
    )?;
    q.set(ResourceAxisV10::StoredEvidenceBytes, add(stored, mul(positions128, 192)?)?);
    q.add_axis(ResourceAxisV10::TraceHashBytes, mul(mul(nodes, positions128)?, 512)?)?;
    // One full-value delivery envelope per node/position. This producer does
    // not invoke a network provider; enforcing retention, requested tiles and
    // transport retries remains the serving consumer's separate obligation.
    let reply_calls = mul(nodes, positions128)?;
    q.set(ResourceAxisV10::ReplyCalls, reply_calls);
    q.set(ResourceAxisV10::DeliveryBytes, add(court.trace_value_bytes_per_claim, mul(reply_calls, 16 << 10)?)?);
    q.set(ResourceAxisV10::Attempts, 1);
    // The actual mixed court API rechecks the complete scope, so reserve its
    // metadata work/RAM too. The pure bound helper is consumed only after this
    // factory independently derives the full program-bound scope above.
    let complete_court = complete_scope_api_bounds_v9(&court).map_err(|e| format!("{e:?}"))?;
    q.set(ResourceAxisV10::CourtWork, complete_court.work as u128);
    q.set(ResourceAxisV10::CourtBytes, complete_court.filing_bytes as u128);
    q.set(ResourceAxisV10::CourtScratchBytes, complete_court.scratch_bytes as u128);
    q.set(ResourceAxisV10::MetadataWork, add(court.metadata_derivation_work, mul(nodes, 16 * 1024)?)?);
    let mut raw = 0u128;
    for (j, _) in pc.by_instance.keys() {
        let d = &program.params[*j as usize];
        let cells = d.shape.iter().try_fold(1u128, |n, d| mul(n, *d as u128))?;
        raw = add(raw, mul(cells, d.dtype.width() as u128)?)?;
    }
    q.set(ResourceAxisV10::RawParameterStorage, raw);
    let owned = [
        ResourceAxisV10::ReferenceTensorPayload,
        ResourceAxisV10::ContainerAllocatorAllowance,
        ResourceAxisV10::StoredEvidenceBytes,
        ResourceAxisV10::ParameterTreeScratch,
        ResourceAxisV10::CourtScratchBytes,
    ]
    .into_iter()
    .try_fold(0u128, |n, a| add(n, q.get(a)))?;
    // Commitment tree roots temporarily clone at most one position's leaves.
    q.set(ResourceAxisV10::OwnedPeakBytes, add(owned, mul(nodes, 192)?)?);
    let vector = FlatResourceVectorV10 {
        version: 10,
        program: object_id(b"misaka-palw/kernel/reference-resource-program/v10", program),
        parameters: pc.root(),
        court_scope: court.root(),
        positions,
        encoder: encoder.map(|e| (e.first_input, e.l)),
        primitives,
        quantities: q,
    };
    Ok(FlatResourcePlanV10 { vector, court })
}

/// All four consumers compare the same independently derived quantities.
/// Reservations are monotone spent allowances, including failed attempts.
/// These local counters are not native policy, coins or reward-credit tables.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct ResourceCeilingsV10 {
    pub plan: ResourceQuantitiesV10,
    pub block: ResourceQuantitiesV10,
    pub bond: ResourceQuantitiesV10,
    pub network: ResourceQuantitiesV10,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct ResourceJobBindingV10 {
    pub actor: Digest,
    pub job: Digest,
    pub vector: Digest,
    pub tokens: Digest,
}
impl ResourceJobBindingV10 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/reference-resource-job/v10", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceJobReservationV10 {
    pub binding: ResourceJobBindingV10,
    pub spent: ResourceQuantitiesV10,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceReservationsV10 {
    // The ceilings are fixed once for this accounting interval. Retrying with a
    // new caller-supplied cap cannot increase an admitted job/bond/network cap.
    ceilings: ResourceCeilingsV10,
    block: ResourceQuantitiesV10,
    network: ResourceQuantitiesV10,
    bonds: BTreeMap<Digest, ResourceQuantitiesV10>,
    jobs: BTreeMap<Digest, ResourceJobReservationV10>,
}
impl ResourceReservationsV10 {
    pub fn new(ceilings: ResourceCeilingsV10) -> Self {
        Self {
            ceilings,
            block: ResourceQuantitiesV10::default(),
            network: ResourceQuantitiesV10::default(),
            bonds: BTreeMap::new(),
            jobs: BTreeMap::new(),
        }
    }
    pub fn ceilings_root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/reference-resource-ceilings/v10", &self.ceilings)
    }
    pub fn block_spent(&self) -> &ResourceQuantitiesV10 {
        &self.block
    }
    pub fn network_spent(&self) -> &ResourceQuantitiesV10 {
        &self.network
    }
    pub fn bond_spent(&self, actor: &Digest) -> Option<&ResourceQuantitiesV10> {
        self.bonds.get(actor)
    }
    pub fn job_spent(&self, job: &Digest) -> Option<&ResourceJobReservationV10> {
        self.jobs.get(job)
    }
    // Only the independently deriving consumer reserves. A public raw vector is
    // reviewable data and cannot itself authorize an underpriced execution.
    fn reserve(&mut self, binding: ResourceJobBindingV10, vector: &FlatResourceVectorV10) -> Result<(), String> {
        if vector.version != 10 || vector.root() != binding.vector {
            return Err("wrong resource reservation vector/version".into());
        }
        let need = &vector.quantities;
        let old = match self.jobs.get(&binding.job) {
            Some(old) if old.binding != binding => return Err("resource job binding cannot change across retries".into()),
            Some(old) => old.spent.clone(),
            None => ResourceQuantitiesV10::default(),
        };
        let job_next = old.checked_add(need)?;
        let block = self.block.checked_add(need)?;
        let network = self.network.checked_add(need)?;
        let bond = self.bonds.get(&binding.actor).cloned().unwrap_or_default().checked_add(need)?;
        for (name, value, limit) in [
            ("plan", &job_next, &self.ceilings.plan),
            ("block", &block, &self.ceilings.block),
            ("bond", &bond, &self.ceilings.bond),
            ("network", &network, &self.ceilings.network),
        ] {
            value.within(limit).map_err(|a| format!("{name} resource allowance exceeded: {a:?}"))?;
        }
        // Atomic: a refusal neither partially updates another budget nor
        // creates a phantom actor/job. A successful callback failure spends it.
        self.bonds.insert(binding.actor, bond);
        self.jobs.insert(binding.job, ResourceJobReservationV10 { binding, spent: job_next });
        self.block = block;
        self.network = network;
        Ok(())
    }
}
/// Exact borrowed raw-width metadata and image. A provider may map an artifact;
/// this decoder checks the declaration/length before allocating reference data.
pub struct RawParameterImageV10<'a> {
    pub dtype: DType,
    pub shape: &'a [u32],
    pub bytes: &'a [u8],
}
pub trait RawParameterProviderV10 {
    fn image(&self, index: u16, layer: Option<u16>) -> Option<RawParameterImageV10<'_>>;
}
/// Root-only producer/reference consumer. The exact parameter commitment is
/// authenticated on every callback; every retry is reserved before callbacks.
/// It does not retain selected values or install a native serving/Final path.
pub fn commit_streaming_bounded_v10(
    program: &TirProgramV1,
    pc: &ParamCommitmentsV1,
    tokens: &[u32],
    declared: &FlatResourceVectorV10,
    provider: &dyn RawParameterProviderV10,
    actor: Digest,
    job: Digest,
    reservations: &mut ResourceReservationsV10,
) -> Result<SegmentedCommitmentsV1, String> {
    let positions = u32::try_from(tokens.len()).map_err(|_| "reference job length overflow")?;
    if tokens.iter().any(|t| *t >= program.token_bound) {
        return Err("reference job token outside bound".into());
    }
    let plan = derive_flat_resources_v10(program, pc, positions, None)?;
    if plan.vector != *declared {
        return Err("reference resource vector differs from independent derivation".into());
    }
    let binding = ResourceJobBindingV10 {
        actor,
        job,
        vector: plan.vector.root(),
        tokens: object_id(b"misaka-palw/kernel/reference-resource-tokens/v10", &tokens),
    };
    reservations.reserve(binding, &plan.vector)?;
    struct Params<'a> {
        program: &'a TirProgramV1,
        pc: &'a ParamCommitmentsV1,
        provider: &'a dyn RawParameterProviderV10,
        remaining: Cell<u128>,
    }
    impl ParamSource for Params<'_> {
        fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
            let remaining = self.remaining.get().checked_sub(1)?;
            self.remaining.set(remaining);
            let d = self.program.params.get(index as usize)?;
            if d.per_layer != layer.is_some() || layer.is_some_and(|l| l as usize >= self.program.schedule.layers.len()) {
                return None;
            }
            let raw = self.provider.image(index, layer)?;
            let cells = d.shape.iter().try_fold(1usize, |n, d| n.checked_mul(*d as usize))?;
            if raw.dtype != d.dtype || raw.shape != d.shape.as_slice() || cells.checked_mul(d.dtype.width())? != raw.bytes.len() {
                return None;
            }
            let shape: Vec<_> = d.shape.iter().map(|d| *d as usize).collect();
            let tensor = Tensor::from_le_bytes(d.dtype, &shape, raw.bytes).ok()?;
            (self.pc.by_instance.get(&(index, layer)) == Some(&tensor_commitment_v3(&tensor))).then_some(tensor)
        }
    }
    let params = Params { program, pc, provider, remaining: Cell::new(plan.vector.quantities.get(ResourceAxisV10::ParameterCalls)) };
    let mut commitments = Vec::with_capacity(tokens.len());
    trace_streaming_v1(program, &params, tokens, &mut |_, values| {
        let mut position = Vec::with_capacity(values.len());
        for occurrence in values {
            let mut row = Vec::with_capacity(occurrence.len());
            for value in occurrence {
                row.push(
                    flat_profile_commitment_v5(value)
                        .map_err(|e| misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Operand, e.to_string()))?,
                );
            }
            position.push(row);
        }
        commitments.push(position);
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    Ok(SegmentedCommitmentsV1::new(commitments))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        flat_court4::tests::{primitive_fixture, state_fixture},
        trace::{TraceV1, trace_v1},
    };
    use misaka_palw_tir::MapParams;
    use std::collections::BTreeSet;

    struct Raw {
        images: BTreeMap<(u16, Option<u16>), (DType, Vec<u32>, Vec<u8>)>,
        calls: Cell<u128>,
    }
    impl Raw {
        fn of(params: &MapParams) -> Self {
            Self {
                images: params
                    .tensors
                    .iter()
                    .map(|(key, t)| (*key, (t.dtype, t.shape.iter().map(|d| *d as u32).collect(), t.to_le_bytes())))
                    .collect(),
                calls: Cell::new(0),
            }
        }
    }
    impl RawParameterProviderV10 for Raw {
        fn image(&self, index: u16, layer: Option<u16>) -> Option<RawParameterImageV10<'_>> {
            self.calls.set(self.calls.get() + 1);
            let (dtype, shape, bytes) = self.images.get(&(index, layer))?;
            Some(RawParameterImageV10 { dtype: *dtype, shape, bytes })
        }
    }
    fn scale(q: &ResourceQuantitiesV10, n: u128) -> ResourceQuantitiesV10 {
        ResourceQuantitiesV10(q.0.map(|v| v.checked_mul(n).unwrap()))
    }
    fn caps(q: &ResourceQuantitiesV10, n: u128) -> ResourceCeilingsV10 {
        ResourceCeilingsV10 { plan: scale(q, n), block: scale(q, n), bond: scale(q, n), network: scale(q, n) }
    }
    fn reference(t: &TraceV1) -> SegmentedCommitmentsV1 {
        SegmentedCommitmentsV1::new(
            t.values
                .iter()
                .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                .collect(),
        )
    }
    #[test]
    fn bounded_reference_matches_all_twenty_five_primitives_and_state_across_positions() {
        let mut covered = BTreeSet::new();
        for tag in 0..23 {
            let (p, params, n) = primitive_fixture(tag);
            let pc = ParamCommitmentsV1::of_v3(&params);
            let raw = Raw::of(&params);
            let tokens = [1, 2, 3];
            let plan = derive_flat_resources_v10(&p, &pc, 3, None).unwrap();
            assert_eq!(p.blocks[0].nodes[n as usize].prim.tag(), tag);
            covered.extend(plan.vector.primitives.iter().map(|p| p.prim_tag));
            let mut state = ResourceReservationsV10::new(caps(&plan.vector.quantities, 1));
            let actual = commit_streaming_bounded_v10(&p, &pc, &tokens, &plan.vector, &raw, [1; 64], [tag; 64], &mut state).unwrap();
            let expected = reference(&trace_v1(&p, &params, &tokens).unwrap());
            assert_eq!(actual, expected, "primitive {tag}");
            assert_eq!(raw.calls.get(), plan.vector.quantities.get(ResourceAxisV10::ParameterCalls));
            assert_eq!(state.block_spent(), &plan.vector.quantities);
            assert!(
                plan.vector.quantities.get(ResourceAxisV10::OwnedPeakBytes)
                    >= plan.vector.quantities.get(ResourceAxisV10::ReferenceTensorPayload)
            );
        }
        let (p, params, _, _) = state_fixture();
        let pc = ParamCommitmentsV1::of_v3(&params);
        let raw = Raw::of(&params);
        let tokens = [1, 2, 3, 1];
        let plan = derive_flat_resources_v10(&p, &pc, 4, None).unwrap();
        covered.extend(plan.vector.primitives.iter().map(|p| p.prim_tag));
        let mut state = ResourceReservationsV10::new(caps(&plan.vector.quantities, 1));
        let actual = commit_streaming_bounded_v10(&p, &pc, &tokens, &plan.vector, &raw, [1; 64], [30; 64], &mut state).unwrap();
        assert_eq!(actual, reference(&trace_v1(&p, &params, &tokens).unwrap()));
        assert_eq!(covered, (0..25).collect());
    }
    #[test]
    fn every_resource_dimension_is_refused_before_parameter_callback_for_all_four_ceiling_scopes() {
        let (p, params, _) = primitive_fixture(11);
        let pc = ParamCommitmentsV1::of_v3(&params);
        let plan = derive_flat_resources_v10(&p, &pc, 1, None).unwrap();
        let raw = Raw::of(&params);
        for axis in ResourceAxisV10::ALL {
            let need = plan.vector.quantities.get(axis);
            assert!(need > 0, "fixture must exercise {axis:?}");
            for consumer in 0..4 {
                let mut cap = caps(&plan.vector.quantities, 1);
                let limit = match consumer {
                    0 => &mut cap.plan,
                    1 => &mut cap.block,
                    2 => &mut cap.bond,
                    _ => &mut cap.network,
                };
                limit.set(axis, need - 1);
                let mut state = ResourceReservationsV10::new(cap);
                let before = state.clone();
                let err = commit_streaming_bounded_v10(&p, &pc, &[1], &plan.vector, &raw, [1; 64], [1; 64], &mut state).unwrap_err();
                assert!(err.contains("resource allowance exceeded"), "{axis:?}/{consumer}: {err}");
                assert_eq!(state, before);
                assert_eq!(raw.calls.get(), 0);
            }
        }
    }
    #[test]
    fn aggregate_plan_block_bond_and_network_refusals_are_atomic_and_do_not_reset_for_new_actor_or_job() {
        let (p, params, _) = primitive_fixture(11);
        let pc = ParamCommitmentsV1::of_v3(&params);
        let plan = derive_flat_resources_v10(&p, &pc, 1, None).unwrap();
        for consumer in 0..4 {
            let raw = Raw::of(&params);
            let mut cap = caps(&plan.vector.quantities, 3);
            match consumer {
                0 => cap.plan = plan.vector.quantities.clone(),
                1 => cap.block = plan.vector.quantities.clone(),
                2 => cap.bond = plan.vector.quantities.clone(),
                _ => cap.network = plan.vector.quantities.clone(),
            };
            let mut state = ResourceReservationsV10::new(cap);
            let ceiling_root = state.ceilings_root();
            commit_streaming_bounded_v10(&p, &pc, &[1], &plan.vector, &raw, [1; 64], [1; 64], &mut state).unwrap();
            let before = state.clone();
            let calls = raw.calls.get();
            let actor = if consumer == 1 || consumer == 3 { [2; 64] } else { [1; 64] };
            let job = if consumer == 0 { [1; 64] } else { [2; 64] };
            assert!(commit_streaming_bounded_v10(&p, &pc, &[1], &plan.vector, &raw, actor, job, &mut state).is_err());
            assert_eq!(raw.calls.get(), calls);
            assert_eq!(state, before);
            assert_eq!(state.ceilings_root(), ceiling_root);
        }
    }
    #[test]
    fn malformed_raw_width_shape_dtype_or_authentication_spends_attempt_and_retry_is_prepaid() {
        let (p, params, _) = primitive_fixture(7);
        let pc = ParamCommitmentsV1::of_v3(&params);
        let plan = derive_flat_resources_v10(&p, &pc, 1, None).unwrap();
        for bad in 0..5 {
            let mut raw = Raw::of(&params);
            let key = (0, None);
            let image = raw.images.get_mut(&key).unwrap();
            match bad {
                0 => image.0 = DType::I32,
                1 => image.1 = vec![u32::MAX],
                2 => image.2.push(0),
                3 => image.2[0] ^= 1,
                _ => {
                    raw.images.remove(&key);
                }
            }
            let mut state = ResourceReservationsV10::new(caps(&plan.vector.quantities, 1));
            assert!(commit_streaming_bounded_v10(&p, &pc, &[1], &plan.vector, &raw, [1; 64], [1; 64], &mut state).is_err());
            assert_eq!(raw.calls.get(), 1);
            assert_eq!(state.job_spent(&[1; 64]).unwrap().spent.get(ResourceAxisV10::Attempts), 1);
            let spent = state.clone();
            assert!(commit_streaming_bounded_v10(&p, &pc, &[1], &plan.vector, &raw, [1; 64], [1; 64], &mut state).is_err());
            assert_eq!(raw.calls.get(), 1);
            assert_eq!(state, spent);
        }
    }
    #[test]
    fn public_vector_tampering_never_authorizes_underpriced_source_work_or_borrowed_court_scope() {
        let (p, params, _) = primitive_fixture(11);
        let pc = ParamCommitmentsV1::of_v3(&params);
        let plan = derive_flat_resources_v10(&p, &pc, 1, None).unwrap();
        let raw = Raw::of(&params);
        let mut changes = Vec::new();
        for axis in ResourceAxisV10::ALL {
            let mut v = plan.vector.clone();
            v.quantities.set(axis, 0);
            changes.push(v);
        }
        for field in 0..8 {
            let mut v = plan.vector.clone();
            match field {
                0 => v.version = 9,
                1 => v.program[0] ^= 1,
                2 => v.parameters[0] ^= 1,
                3 => v.court_scope[0] ^= 1,
                4 => v.positions = 2,
                5 => v.encoder = Some((0, 1)),
                6 => v.primitives[0].prim_tag = 22,
                _ => v.primitives.clear(),
            }
            changes.push(v);
        }
        for changed in changes {
            let mut state = ResourceReservationsV10::new(caps(&plan.vector.quantities, 2));
            let before = state.clone();
            assert!(
                commit_streaming_bounded_v10(&p, &pc, &[1], &changed, &raw, [1; 64], [1; 64], &mut state)
                    .unwrap_err()
                    .contains("independent derivation")
            );
            assert_eq!(state, before);
            assert_eq!(raw.calls.get(), 0);
        }
    }
    #[test]
    fn same_job_cannot_borrow_a_new_actor_input_extent_parameter_root_or_vector_across_retries() {
        let (p, params, _) = primitive_fixture(7);
        let pc = ParamCommitmentsV1::of_v3(&params);
        let plan = derive_flat_resources_v10(&p, &pc, 1, None).unwrap();
        let mut state = ResourceReservationsV10::new(caps(&plan.vector.quantities, 100));
        let raw = Raw::of(&params);
        commit_streaming_bounded_v10(&p, &pc, &[1], &plan.vector, &raw, [1; 64], [1; 64], &mut state).unwrap();
        let before = state.clone();
        let calls = raw.calls.get();
        for (actor, tokens) in [([2; 64], vec![1]), ([1; 64], vec![2]), ([1; 64], vec![1, 2])] {
            let altered = derive_flat_resources_v10(&p, &pc, tokens.len() as u32, None).unwrap();
            assert!(
                commit_streaming_bounded_v10(&p, &pc, &tokens, &altered.vector, &raw, actor, [1; 64], &mut state)
                    .unwrap_err()
                    .contains("binding cannot change")
            );
            assert_eq!(state, before);
            assert_eq!(raw.calls.get(), calls);
        }
        let mut other = params.clone();
        other.tensors.get_mut(&(0, None)).unwrap().data[0] += 1;
        let other_pc = ParamCommitmentsV1::of_v3(&other);
        let other_plan = derive_flat_resources_v10(&p, &other_pc, 1, None).unwrap();
        let other_raw = Raw::of(&other);
        assert!(
            commit_streaming_bounded_v10(&p, &other_pc, &[1], &other_plan.vector, &other_raw, [1; 64], [1; 64], &mut state)
                .unwrap_err()
                .contains("binding cannot change")
        );
        assert_eq!(state, before);
        assert_eq!(other_raw.calls.get(), 0);
        // An honest repeat with the identical subject pays another full attempt.
        commit_streaming_bounded_v10(&p, &pc, &[1], &plan.vector, &raw, [1; 64], [1; 64], &mut state).unwrap();
        assert_eq!(state.job_spent(&[1; 64]).unwrap().spent.get(ResourceAxisV10::Attempts), 2);
    }
    #[test]
    fn checked_resource_aggregation_cannot_wrap_and_exact_namespaces_and_extents_are_required() {
        let mut q = ResourceQuantitiesV10::default();
        q.set(ResourceAxisV10::ScalarSteps, u128::MAX);
        let mut one = ResourceQuantitiesV10::default();
        one.set(ResourceAxisV10::ScalarSteps, 1);
        assert!(q.checked_add(&one).is_err());
        assert!(mul(u128::MAX, 2).is_err());
        let (p, params, _) = primitive_fixture(7);
        let pc = ParamCommitmentsV1::of_v3(&params);
        assert!(derive_flat_resources_v10(&p, &pc, 0, None).is_err());
        assert!(derive_flat_resources_v10(&p, &pc, p.history_bound + 1, None).is_err());
        let mut wrong = pc.clone();
        wrong.by_instance.insert((0, Some(0)), [0; 64]);
        assert!(derive_flat_resources_v10(&p, &wrong, 1, None).is_err());
        wrong.by_instance.remove(&(0, None));
        assert!(derive_flat_resources_v10(&p, &wrong, 1, None).is_err());
        let a = derive_flat_resources_v10(&p, &pc, 1, None).unwrap();
        let b = derive_flat_resources_v10(&p, &pc, 2, None).unwrap();
        assert_ne!(a.vector.root(), b.vector.root());
        assert_eq!(b.vector.quantities.get(ResourceAxisV10::Macs), a.vector.quantities.get(ResourceAxisV10::Macs) * 2);
        assert_eq!(
            b.vector.quantities.get(ResourceAxisV10::ParameterHashBytes),
            a.vector.quantities.get(ResourceAxisV10::ParameterHashBytes) * 2
        );
        assert_eq!(b.vector.quantities.get(ResourceAxisV10::Attempts), 1);
    }
    #[test]
    fn actual_qwen_metadata_prices_source_scans_hashes_reference_memory_and_unrolled_nodes_without_loading_weights() {
        let p = TirProgramV1::decode_canonical(include_bytes!("../../docs/design/palw/tir/evidence/qwen25-real-pilot-program.tir"))
            .unwrap();
        let pc: ParamCommitmentsV1 =
            borsh::from_slice(include_bytes!("../../docs/design/palw/tir/evidence/qwen25-real-params-v3-commitments.borsh")).unwrap();
        let plan = derive_flat_resources_v10(&p, &pc, 32, None).unwrap();
        assert_eq!(plan.vector.primitives.len(), 7658);
        assert_eq!(pc.by_instance.len(), 1636);
        assert_eq!(plan.vector.parameters, pc.root());
        assert_eq!(plan.vector.court_scope, plan.court.root());
        assert!(plan.vector.quantities.get(ResourceAxisV10::Macs) > 1 << 30);
        assert!(
            plan.vector.quantities.get(ResourceAxisV10::ValidatedElements)
                > plan.vector.quantities.get(ResourceAxisV10::ParameterCalls)
        );
        assert!(plan.vector.quantities.get(ResourceAxisV10::RawParameterStorage) > 1 << 30);
        assert!(
            plan.vector.quantities.get(ResourceAxisV10::ParameterHashBytes)
                > plan.vector.quantities.get(ResourceAxisV10::RawParameterStorage)
        );
        assert!(
            plan.vector.quantities.get(ResourceAxisV10::OwnedPeakBytes)
                > plan.vector.quantities.get(ResourceAxisV10::ReferenceTensorPayload)
        );
        println!(
            "QWEN_RESOURCE10_METADATA_ONLY nodes={} positions={} quantities={:?} vector={} source_weights_loaded=false reference_executed=false native_admission=false final=false full_rfc02=false",
            plan.vector.primitives.len(),
            plan.vector.positions,
            plan.vector.quantities.0,
            crate::hash::hex(&plan.vector.root())
        );
    }
}
