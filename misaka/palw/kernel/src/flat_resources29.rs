//! Maximum and actual 21-axis quotes for the independently checked court25.
//! Frozen10 formulas are copied explicitly under a new identity. No source
//! weights, state layouts, old scope roots or historical grants are resized.
use crate::{
    element::PreparedV1,
    flat_resources10::{PrimitiveResourcesV10, ResourceAxisV10, ResourceQuantitiesV10},
    flat_scope25::FlatResourceScopeV25,
    hash::{Digest, object_id},
    merkle3::LayoutV3,
    merkle5::flat_material_bounds_v5,
    seg_encoder::EncoderBindingV1,
    trace::ParamCommitmentsV1,
};
use borsh::BorshSerialize;
use misaka_palw_tir::{
    DType, Prim,
    program::{Ref, TirProgramV1},
};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatResourceVectorV29 {
    pub version: u16,
    pub program: Digest,
    pub parameters: Digest,
    pub court_scope: Digest,
    pub positions: u32,
    pub encoder: Option<(u16, u32)>,
    pub primitives: Vec<PrimitiveResourcesV10>,
    pub quantities: ResourceQuantitiesV10,
}
impl FlatResourceVectorV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/original-maximum-reference-resource/v29", self)
    }
}
pub struct FlatResourcePlanV29 {
    pub vector: FlatResourceVectorV29,
    pub court: FlatResourceScopeV25,
}
fn axis_add(q: &mut ResourceQuantitiesV10, a: ResourceAxisV10, v: u128) -> Result<(), String> {
    q.set(a, add(q.get(a), v)?);
    Ok(())
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
pub fn derive_flat_resources_v29(
    program: &TirProgramV1,
    pc: &ParamCommitmentsV1,
    positions: u32,
    encoder: Option<&EncoderBindingV1>,
) -> Result<FlatResourcePlanV29, String> {
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
    let checked =
        crate::flat_scope25::derive_checked_flat_resource_scope_v25(program, positions, encoder).map_err(|e| format!("{e:?}"))?;
    let court = checked.scope().clone();
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
                        axis_add(&mut q, ResourceAxisV10::ParameterCalls, positions as u128)?;
                        axis_add(&mut q, ResourceAxisV10::ParameterHashBytes, mul(hash, positions as u128)?)?;
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
            axis_add(&mut q, ResourceAxisV10::TraceHashBytes, mul(hash, positions as u128)?)?;
            for (axis, value) in [
                (ResourceAxisV10::Macs, macs),
                (ResourceAxisV10::ScalarSteps, scalar),
                (ResourceAxisV10::IndexSteps, indices),
                (ResourceAxisV10::ValidatedElements, validated),
                (ResourceAxisV10::CopiedReferenceBytes, copied),
            ] {
                axis_add(&mut q, axis, mul(value, positions as u128)?)?;
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
    axis_add(&mut q, ResourceAxisV10::TraceHashBytes, mul(mul(nodes, positions128)?, 512)?)?;
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
    let complete_court = crate::flat_court25::complete_scope_api_bounds_v25(&court).map_err(|e| format!("{e:?}"))?;
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
    let vector = FlatResourceVectorV29 {
        version: 29,
        program: object_id(b"misaka-palw/kernel/reference-resource-program/v29", program),
        parameters: pc.root(),
        court_scope: court.root(),
        positions,
        encoder: encoder.map(|e| (e.first_input, e.l)),
        primitives,
        quantities: q,
    };
    Ok(FlatResourcePlanV29 { vector, court })
}
