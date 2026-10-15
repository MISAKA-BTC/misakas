//! Material verification workspaces, distinct from the inner Borsh decode and
//! from salted-claim replay. All dimensions come from actual decoded values or
//! the selected admitted class; raw frame lengths are never native lane counts.
use super::*;
use misaka_palw_kernel::{
    hash::Digest,
    public::{MaterialResponseV1, PositionResponseV1, TensorWireV1},
};

fn bad(why: &str) -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<PositionResponseV1>("kernel material authentication workspace", why)
}
fn product(a: u64, b: u64) -> Result<u64, NativeCoverageV1> {
    TirNativeRequestsV1::product_v1(a, b)
}
fn add(a: u64, b: u64) -> Result<u64, NativeCoverageV1> {
    a.checked_add(b).ok_or_else(|| NativeCoverageV1::overflow::<PositionResponseV1>("kernel material cardinality"))
}
fn shape_product(mut shape: impl Iterator<Item = u64>) -> Result<u64, NativeCoverageV1> {
    shape.try_fold(1u64, |n, d| product(n, d))
}
fn tensor_wire(q: &mut TirNativeRequestsV1, wire: &TensorWireV1) -> Result<(), NativeCoverageV1> {
    let dtype = misaka_palw_kernel::plan::dtype_of_tag(wire.dtype).ok_or_else(|| bad("unknown material tensor dtype"))?;
    let n = shape_product(wire.shape.iter().copied())?;
    if product(n, dtype.width() as u64)? != wire.bytes.len() as u64 {
        return Err(bad("material tensor dimensions and bytes disagree"));
    }
    tensor(q, wire.shape.len() as u64, n, wire.shape.last().copied(), wire.shape.iter().rev().nth(1).copied(), false)
}
fn tensor(
    q: &mut TirNativeRequestsV1,
    rank: u64,
    n: u64,
    last: Option<u64>,
    penultimate: Option<u64>,
    v3: bool,
) -> Result<(), NativeCoverageV1> {
    let (rows, row_len, cols, col_len) = match (last, penultimate) {
        (_, None) => (1, n, 1, n),
        (Some(last), Some(before)) => (if last == 0 { 0 } else { n / last }, last, if before == 0 { 0 } else { n / before }, before),
        _ => return Err(bad("inconsistent tensor rank")),
    };
    q.exact_v1::<i128>(n)?; // TensorWire/Tensor::from_le_bytes, actual lanes
    q.vec_v1::<usize>(rank)?; // Tensor shape
    q.vec_v1::<usize>(rank)?; // declared dtype/shape comparison
    q.vec_v1::<u64>(rank)?; // Layout::try_of
    if v3 {
        let rows = product(rows, row_len.div_ceil(misaka_palw_kernel::merkle3::TILE_V3))?;
        let cols = product(cols, col_len.div_ceil(misaka_palw_kernel::merkle3::TILE_V3))?;
        q.vec_v1::<Digest>(rows)?;
        q.vec_v1::<Digest>(cols)?; // TreesV3 owned leaves
        q.vec_v1::<Digest>(rows)?;
        q.vec_v1::<Digest>(rows.div_ceil(2))?; // cloned root + replacement
        q.vec_v1::<Digest>(cols)?;
        q.vec_v1::<Digest>(cols.div_ceil(2))?;
    } else {
        q.vec_v1::<i128>(row_len)?;
        q.vec_v1::<i128>(col_len)?;
        q.vec_v1::<Digest>(rows)?;
        q.vec_v1::<Digest>(rows.div_ceil(2))?;
        q.vec_v1::<Digest>(cols)?;
        q.vec_v1::<Digest>(cols.div_ceil(2))?;
    }
    Ok(())
}
fn opening(q: &mut TirNativeRequestsV1, rank: u64, tile: bool) -> Result<(), NativeCoverageV1> {
    q.vec_v1::<usize>(rank)?;
    q.vec_v1::<u64>(rank)?;
    if tile {
        q.vec_v1::<u64>(misaka_palw_kernel::merkle3::TILE_V3)?;
    }
    Ok(())
}
/// Additional classifier work after the caller's exact inner-codec preflight.
/// Served tensor bytes move from the decoded response; its output Vec frames
/// and transient native i128 tensors/Merkle levels are separately requested.
pub fn native_kernel_position_auth_requests_v4(r: &PositionResponseV1) -> Result<u64, NativeCoverageV1> {
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<Vec<Option<TensorWireV1>>>(r.values.len() as u64)?;
    q.vec_v1::<TensorWireV1>(r.inputs.len() as u64)?;
    for values in &r.values {
        q.vec_v1::<Option<TensorWireV1>>(values.len() as u64)?;
    }
    for value in r.values.iter().flatten().chain(&r.inputs) {
        match value {
            MaterialResponseV1::Whole(w) => tensor_wire(&mut q, w)?,
            MaterialResponseV1::Part(p) => opening(&mut q, p.shape.len() as u64, false)?,
            MaterialResponseV1::Omitted => {}
        }
    }
    Ok(q.bytes_v1())
}
/// Parameter opening authentication streams the leaf values already retained
/// by the decoder. It allocates rank projections and, for v3, bounded tile
/// indices; it never loads a full parameter tensor under a row/tile root.
pub fn native_kernel_parameter_auth_requests_v4(
    r: &crate::palw_kernel_parameter_material_v4::PalwKernelParameterRowV4,
) -> Result<u64, NativeCoverageV1> {
    use crate::palw_kernel_parameter_material_v4::PalwKernelParameterOpeningV4 as O;
    let mut q = TirNativeRequestsV1::new();
    match &r.opening {
        O::RowV2(o) => opening(&mut q, o.shape.len() as u64, false)?,
        O::TileV3(o) => opening(&mut q, o.shape.len() as u64, true)?,
    }
    Ok(q.bytes_v1())
}

fn occurrence_block<'a>(
    p: &'a misaka_palw_tir::program::TirProgramV1,
    occurrence: u16,
) -> Result<&'a misaka_palw_tir::program::Block, NativeCoverageV1> {
    let at = occurrence as usize;
    let index = if at == 0 {
        p.schedule.pre
    } else if at <= p.schedule.layers.len() {
        p.schedule.layers[at - 1]
    } else if at == p.schedule.layers.len() + 1 {
        p.schedule.post
    } else {
        return Err(bad("segment occurrence is outside selected program"));
    };
    p.blocks.get(index as usize).ok_or_else(|| bad("segment occurrence names an absent selected block"))
}
/// Authentication-only scratch for one actual decoded part. Wiring/mode/part
/// layout construction is owned separately by the selected-class reservation.
/// The declared H bound is conservative for every node at this actual position.
pub fn native_kernel_segment_auth_requests_v4(
    p: &misaka_palw_tir::program::TirProgramV1,
    position: u32,
    r: &misaka_palw_kernel::seg_da::SegPartResponseV1,
) -> Result<u64, NativeCoverageV1> {
    use misaka_palw_kernel::seg_da::ChunkBodyV1;
    use misaka_palw_tir::types::Dim;
    let mut q = TirNativeRequestsV1::new();
    let h = add(u64::from(position), 1)?.min(u64::from(p.history_bound));
    for c in &r.chunks {
        let block = occurrence_block(p, c.occurrence)?;
        let node = block.nodes.get(c.node as usize).ok_or_else(|| bad("segment node is outside selected program"))?;
        let dim = |d: &Dim| match d {
            Dim::Fixed(n) => u64::from(*n),
            Dim::H => h,
        };
        let n = shape_product(node.out.shape.iter().map(dim))?;
        let last = node.out.shape.last().map(dim);
        let before = node.out.shape.iter().rev().nth(1).map(dim);
        let rank = node.out.shape.len() as u64;
        q.vec_v1::<usize>(rank)?;
        q.vec_v1::<u64>(rank)?;
        q.vec_v1::<Digest>(c.node_siblings.len() as u64)?;
        q.vec_v1::<Digest>(r.position_siblings.len() as u64)?;
        match &c.body {
            ChunkBodyV1::Whole(_) => tensor(&mut q, rank, n, last, before, true)?,
            ChunkBodyV1::Rows(range) => {
                let row_len = last.unwrap_or(n);
                let rows = if rank <= 1 {
                    1
                } else if row_len == 0 {
                    0
                } else {
                    n / row_len
                };
                let leaves = product(rows, row_len.div_ceil(misaka_palw_kernel::merkle3::TILE_V3))?;
                if range.leaves > leaves {
                    return Err(bad("served row range exceeds selected node's declared leaves"));
                }
                q.vec_v1::<u64>(misaka_palw_kernel::merkle3::TILE_V3)?;
                q.vec_v1::<i128>(misaka_palw_kernel::merkle3::TILE_V3)?;
                q.vec_v1::<Digest>(range.leaves)?; // hashes
                q.vec_v1::<Digest>(range.leaves)?; // range_root known
                q.vec_v1::<Digest>(add(range.leaves, 2)?)?; // ext with edge siblings
                q.vec_v1::<Digest>(add(range.leaves, 2)?.div_ceil(2))?; // replacement known
            }
            ChunkBodyV1::Hidden(range) => {
                let leaves = range.leaves.len() as u64;
                q.vec_v1::<Digest>(leaves)?;
                q.vec_v1::<Digest>(add(leaves, 2)?)?;
                q.vec_v1::<Digest>(add(leaves, 2)?.div_ceil(2))?;
            }
            ChunkBodyV1::Withheld => {}
        }
    }
    Ok(q.bytes_v1())
}

fn copy<T: NativeHeapV1 + ?Sized>(q: &mut TirNativeRequestsV1, value: &T, purpose: &str) -> Result<(), NativeCoverageV1> {
    let bound = value.native_heap_v1(purpose);
    q.add_v1(bound.bytes().ok_or(bound)?)
}
fn selected_program(
    q: &mut TirNativeRequestsV1,
    class: &misaka_palw_kernel::ledger::ClassRowV1,
    positions: u64,
    segmented: bool,
) -> Result<(), NativeCoverageV1> {
    let bytes = super::ledger_expansion::standalone_tir_program_requests_v1(&class.program_bytes)
        .map_err(|e| bad(&format!("selected material program analysis: {e}")))?;
    q.add_v1(bytes)?;
    q.add_v1(
        misaka_palw_kernel::scope::native_material_mask_requests_v4(&class.program)
            .ok_or_else(|| bad("actual selected material-mask workspace overflows"))?,
    )?;
    layout(q, &class.program.blocks, &class.program.schedule, class.program.history_bound, positions, segmented)
}

/// Count the actual unrolled occurrence/node memberships without constructing
/// Wiring, masks, declared-value shapes or part arrays in the quote itself.
/// A layout chunk consumes at least one node or row/hiding leaf, so the sum of
/// these concrete leaves bounds ChunkSpec Vecs and part/used/sizes Vecs even
/// when greedy packing splits every leaf. The actual classifier still checks
/// its stronger codec/part limits; this request does not alter those limits.
fn layout(
    q: &mut TirNativeRequestsV1,
    blocks: &[misaka_palw_tir::program::Block],
    schedule: &misaka_palw_tir::program::Schedule,
    history_bound: u32,
    positions: u64,
    segmented: bool,
) -> Result<(), NativeCoverageV1> {
    use misaka_palw_kernel::seg_scope::SegValueModeV1;
    use misaka_palw_tir::types::Dim;
    let occurrences = add(schedule.layers.len() as u64, 2)?;
    let h = positions.max(1).min(u64::from(history_bound));
    q.vec_v1::<(u8, Option<u16>)>(occurrences)?;
    // Derived, model-dependent and withheld masks coexist with the final mode
    // map. The graph analyses' typed cone/range arrays are charged by the
    // actual program preflight above, separately from these returned masks.
    for _ in 0..3 {
        q.vec_v1::<Vec<bool>>(occurrences)?;
    }
    q.vec_v1::<Vec<SegValueModeV1>>(occurrences)?;
    let mut values = 0u64;
    let mut shape_words = 0u64;
    let mut chunks = 0u64;
    for index in std::iter::once(&schedule.pre).chain(schedule.layers.iter()).chain(std::iter::once(&schedule.post)) {
        let block = blocks.get(*index as usize).ok_or_else(|| bad("selected material schedule names absent block"))?;
        let nodes = block.nodes.len() as u64;
        values = add(values, nodes)?;
        for _ in 0..3 {
            q.vec_v1::<bool>(nodes)?;
        }
        q.vec_v1::<SegValueModeV1>(nodes)?;
        for node in &block.nodes {
            shape_words = add(shape_words, node.out.shape.len() as u64)?;
            if segmented {
                let at = |d: &Dim| match d {
                    Dim::Fixed(n) => u64::from(*n),
                    Dim::H => h,
                };
                let count = shape_product(node.out.shape.iter().map(at))?;
                let row_len = node.out.shape.last().map(at).unwrap_or(count);
                let rows = if node.out.shape.len() <= 1 {
                    1
                } else if row_len == 0 {
                    0
                } else {
                    count / row_len
                };
                let row_leaves = product(rows, row_len.div_ceil(misaka_palw_kernel::merkle3::TILE_V3))?;
                let hide_leaves = misaka_palw_kernel::hide::hide_tiles_v1(count);
                chunks = add(chunks, add(1, add(row_leaves, hide_leaves)?)?)?;
            }
        }
    }
    q.exact_v1::<typed::BTreeNodeEnvelopeV1<(u16, Option<u16>), (u16, u16)>>(values)?; // state writers
    q.exact_v1::<typed::BTreeNodeEnvelopeV1<(u16, Option<u16>), (u16, u16)>>(values)?; // history appenders
    q.exact_v1::<typed::BTreeNodeEnvelopeV1<(u8, u16), ()>>(values)?; // derived node set
    if segmented {
        // classify_part holds one Wiring while position_parts constructs its
        // own; both have their own modes/declared-value projections.
        q.vec_v1::<(u8, Option<u16>)>(occurrences)?;
        q.vec_v1::<(u16, u16, misaka_palw_tir::DType, Vec<usize>, SegValueModeV1)>(values)?;
        q.vec_v1::<usize>(shape_words)?;
        // Each inner shape Vec has the pinned minimum capacity independently.
        q.exact_v1::<usize>(product(values, 4)?)?;
        let parts = add(chunks, 1)?;
        q.vec_v1::<Vec<misaka_palw_kernel::seg_da::ChunkSpecV1>>(parts)?;
        q.vec_v1::<misaka_palw_kernel::seg_da::ChunkSpecV1>(chunks)?;
        q.exact_v1::<misaka_palw_kernel::seg_da::ChunkSpecV1>(product(parts, 4)?)?;
        q.vec_v1::<u64>(parts)?; // layout used_by
        q.vec_v1::<u64>(parts)?; // returned part bytes
    }
    Ok(())
}

fn selected_material_requests(ledger: &misaka_palw_kernel::ledger::KernelLedgerV1, claim: &Digest) -> Result<u64, NativeCoverageV1> {
    use misaka_palw_kernel::{
        ledger::ClaimBodyV1,
        spec::{ComponentV1, SpecClaimV1, SpecClassKindV1, StageClaimV1},
    };
    let row = ledger.claims.get(claim).ok_or_else(|| bad("material reservation requires actual admitted claim"))?;
    let mut q = TirNativeRequestsV1::new();
    match &row.body {
        ClaimBodyV1::Program { commitments, .. } => {
            let class = ledger.classes.get(&row.class_binding_id).ok_or_else(|| bad("actual material class absent"))?;
            selected_program(&mut q, class, commitments.len() as u64, false)?;
        }
        ClaimBodyV1::Segmented { evidence, .. } => {
            let class = ledger.classes.get(&row.class_binding_id).ok_or_else(|| bad("actual segmented material class absent"))?;
            // Two independent Wiring analyses coexist in classify_part and
            // position_parts. Their program analysis arrays are separate.
            selected_program(&mut q, class, u64::from(evidence.positions), true)?;
            selected_program(&mut q, class, u64::from(evidence.positions), false)?;
        }
        ClaimBodyV1::Pipeline { stages, .. } => {
            let class =
                ledger.pipeline_classes.get(&row.class_binding_id).ok_or_else(|| bad("actual pipeline material class absent"))?;
            if class.pipeline.stages.len() != stages.len() {
                return Err(bad("actual pipeline material stage count mismatch"));
            }
            for (stage, committed) in class.pipeline.stages.iter().zip(stages) {
                let program = class.programs.get(stage.program as usize).ok_or_else(|| bad("actual material stage program absent"))?;
                let raw = class.program_bytes.get(stage.program as usize).ok_or_else(|| bad("actual material stage codec absent"))?;
                q.add_v1(
                    super::ledger_expansion::standalone_program_requests_v1::<misaka_palw_tir::program_v2::TirProgramV2>(
                        raw,
                        misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64,
                    )
                    .map_err(|e| bad(&e.to_string()))?,
                )?;
                q.add_v1(
                    misaka_palw_kernel::scope::native_material_mask_parts_requests_v4(
                        &program.blocks,
                        &program.schedule,
                        program.states.len(),
                    )
                    .ok_or_else(|| bad("actual pipeline material mask workspace overflows"))?,
                )?;
                copy(&mut q, program, "material stage_view owned V1 program")?;
                copy(&mut q, &program.inputs, "material stage input parameter names/shapes")?;
                q.vec_v1::<misaka_palw_tir::program::ParamDecl>(add(program.params.len() as u64, program.inputs.len() as u64)?)?;
                layout(&mut q, &program.blocks, &program.schedule, program.history_bound, committed.commitments.len() as u64, false)?;
            }
        }
        ClaimBodyV1::Spec(body) => {
            let class = ledger.typed.classes.get(&row.class_binding_id).ok_or_else(|| bad("actual typed material class absent"))?;
            match (&class.kind, &body.claim) {
                (SpecClassKindV1::Memory { rule, root, .. }, SpecClaimV1::Memory(c)) => {
                    let positions = c.steps.iter().map(|s| s.commitments.len() as u64).max().unwrap_or(0);
                    selected_program(&mut q, rule, positions, false)?;
                    q.vec_v1::<Vec<bool>>(1)?;
                    q.vec_v1::<bool>(root.slots.len() as u64)?; // memory entry-state mask
                }
                (SpecClassKindV1::Retrieval { .. }, SpecClaimV1::Retrieval(_)) => {}
                (SpecClassKindV1::Composite { components, .. }, SpecClaimV1::Composite(c)) => {
                    if components.len() != c.stages.len() {
                        return Err(bad("actual typed material component count mismatch"));
                    }
                    for (component, stage) in components.iter().zip(&c.stages) {
                        match (component, stage) {
                            (ComponentV1::Model(model), StageClaimV1::Model { commitments, .. }) => {
                                selected_program(&mut q, model, commitments.len() as u64, false)?
                            }
                            (ComponentV1::Retrieval(_), StageClaimV1::Retrieval { .. }) => {}
                            _ => return Err(bad("actual typed material component kind mismatch")),
                        }
                    }
                }
                _ => return Err(bad("actual typed material class and claim kind mismatch")),
            }
        }
    }
    Ok(q.bytes_v1())
}
impl ReservedKernelLedgerV1 {
    /// Reserve selected-class Wiring, masks, stage views and greedy segmented
    /// layout BEFORE full inventory enumeration/classification. The candidate
    /// retains this lease. Material codecs and their Tensor/Merkle expansion
    /// have distinct typed input reservations; this API does not cover SDK
    /// re-execution, artifact loading, arbitrary proof construction or fees.
    pub fn try_reserve_kernel_material_v4(&mut self, claim: &Digest) -> Result<(), NativeCoverageV1> {
        if self.lease.is_some() || native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1() {
            let requests = selected_material_requests(&self.ledger, claim)?;
            if let Some(ticket) = &mut self.lease {
                ticket.grow_memory_v1(requests)?;
            } else {
                self.lease = reserve_native_requests_v1(requests)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn position_tensor_lanes_follow_actual_dimensions_and_refuse_incoherent_wire() {
        let make = |count: u64| PositionResponseV1 {
            values: vec![vec![MaterialResponseV1::Whole(TensorWireV1 {
                dtype: misaka_palw_tir::DType::I32.tag(),
                shape: vec![count],
                bytes: vec![0; count as usize * 4],
            })]],
            inputs: vec![],
        };
        assert!(
            native_kernel_position_auth_requests_v4(&make(1024)).unwrap() > native_kernel_position_auth_requests_v4(&make(1)).unwrap()
        );
        let mut malformed = make(1);
        if let MaterialResponseV1::Whole(w) = &mut malformed.values[0][0] {
            w.shape[0] = u64::MAX;
        }
        assert!(native_kernel_position_auth_requests_v4(&malformed).is_err());
    }
}
