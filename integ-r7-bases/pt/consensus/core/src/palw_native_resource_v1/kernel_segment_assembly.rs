//! Full served-position assembly, separate from raw part ownership, selected
//! class admission and a single part's authentication. No codec/native ratio.
use super::*;
use misaka_palw_kernel::{hash::Digest, seg_da::SegPartResponseV1};
use misaka_palw_tir::{Tensor, program::TirProgramV1, types::Dim};

fn bad(reason: &str) -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<SegPartResponseV1>("actual segmented position assembly", reason)
}
fn add(a: u64, b: u64) -> Result<u64, NativeCoverageV1> {
    a.checked_add(b).ok_or_else(|| NativeCoverageV1::overflow::<SegPartResponseV1>("assembly cardinality"))
}
fn product(a: u64, b: u64) -> Result<u64, NativeCoverageV1> {
    TirNativeRequestsV1::product_v1(a, b)
}

/// Before `seg_da::assemble_position_full_v1`, reserve its retained decoded
/// parts, full value/hidden-hash maps, bytes concatenation, Tensor lanes and
/// concurrent classifier scratch. `parts` must be actual retained raw frames
/// for this selected admitted program; this function does not authenticate them.
/// The caller retains the returned request through the assembled material's
/// lifetime. Raw frames, previous candidate ledger and other positions retain
/// distinct tickets. This is logical construction coverage, not physical RAM.
pub fn native_kernel_segment_assembly_requests_v4(
    program: &TirProgramV1,
    position: u32,
    parts: &[Vec<u8>],
) -> Result<u64, NativeCoverageV1> {
    use misaka_palw_kernel::seg_da::{SEG_MAX_PARTS_V4, SEG_PART_BYTES_V4};
    if position >= program.token_bound || parts.is_empty() || parts.len() > SEG_MAX_PARTS_V4 as usize {
        return Err(bad("position or offered part count is outside the existing program/codec bound"));
    }
    // A typed program is already owned. A temporary canonical serialization is
    // used only to invoke the exact existing analysis preflight. Count and
    // reserve that exact byte Vec BEFORE encoding; never treat it as RAM size.
    let canonical_len = borsh::object_length(program).map_err(|e| bad(&e.to_string()))? as u64;
    if canonical_len > misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64 {
        return Err(bad("actual program exceeds the existing canonical codec bound"));
    }
    let mut initial = TirNativeRequestsV1::new();
    initial.exact_v1::<u8>(canonical_len)?;
    initial.exact_v1::<bool>(parts.len() as u64)?;
    let _quote_workspace = reserve_native_requests_v1(initial.bytes_v1())?;
    let mut canonical = Vec::with_capacity(canonical_len as usize);
    borsh::to_writer(&mut canonical, program).map_err(|e| bad(&e.to_string()))?;
    if canonical.len() as u64 != canonical_len {
        return Err(bad("canonical program changed during assembly quote"));
    }
    let mut q = TirNativeRequestsV1::new();
    // The audited adapter includes four simultaneous typed program/analysis
    // passes. Assembly has at most three Wiring validations live: its own,
    // classify_part's and that classifier's position_parts helper.
    q.add_v1(super::ledger_expansion::standalone_tir_program_requests_v1(&canonical).map_err(|e| bad(&e.to_string()))?)?;
    // Two live greedy layouts: the assembler's and its current classifier's.
    // Each layout helper includes both Wiring frames; this conservatively
    // retains a fourth frame without assuming the two layouts share storage.
    super::kernel_material::assembly_layout_requests_v4(&mut q, program, u64::from(position) + 1)?;
    super::kernel_material::assembly_layout_requests_v4(&mut q, program, u64::from(position) + 1)?;
    let mut decoded_total = 0;
    let mut largest_decode = 0;
    let mut largest_auth = 0;
    let mut largest_position_path = 0;
    let mut seen = vec![false; parts.len()];
    for raw in parts {
        if raw.len() as u64 > SEG_PART_BYTES_V4 {
            return Err(bad("raw part exceeds the existing per-part codec cap"));
        }
        // Unconditional allocation-free exact framing, then one scoped decoder
        // ticket. The quote itself cannot allocate from hostile Vec lengths.
        let shape = preflight_native_value_v1::<SegPartResponseV1>(raw.as_slice(), raw.len() as u64)
            .map_err(|e| bad(&format!("part structural preflight: {e}")))?;
        let requests = shape.logical_requests_v1();
        let _decode_workspace = reserve_native_requests_v1(requests)?;
        let part: SegPartResponseV1 = borsh::from_slice(raw).map_err(|e| bad(&e.to_string()))?;
        let slot = seen.get_mut(part.part as usize).ok_or_else(|| bad("part IDs must cover exactly the offered complete set"))?;
        if std::mem::replace(slot, true) {
            return Err(bad("duplicate part ID does not authorize replacement allocations"));
        }
        decoded_total = add(decoded_total, requests)?;
        largest_decode = largest_decode.max(requests);
        largest_auth = largest_auth.max(native_kernel_segment_auth_requests_v4(program, position, &part)?);
        largest_position_path = largest_position_path.max(part.position_siblings.len() as u64);
    }
    // Every decoded response may coexist in by_part. classify_part owns one
    // additional decoded response while all previous by_part rows are retained.
    q.add_v1(decoded_total)?;
    q.add_v1(largest_decode)?;
    q.add_v1(largest_auth)?;
    q.vec_v1::<Digest>(largest_position_path)?;
    let occurrences = add(program.schedule.layers.len() as u64, 2)?;
    q.exact_v1::<Vec<Option<Vec<u8>>>>(occurrences)?; // bytes
    q.exact_v1::<Vec<Option<Vec<Digest>>>>(occurrences)?; // hidden
    q.exact_v1::<Vec<Digest>>(occurrences)?; // commitments
    q.exact_v1::<Vec<Tensor>>(occurrences)?; // assembled values
    let h = (u64::from(position) + 1).min(u64::from(program.history_bound));
    let mut chunk_upper = 0;
    for index in std::iter::once(&program.schedule.pre).chain(&program.schedule.layers).chain(std::iter::once(&program.schedule.post))
    {
        let block = program.blocks.get(*index as usize).ok_or_else(|| bad("selected schedule names an absent block"))?;
        let nodes = block.nodes.len() as u64;
        q.exact_v1::<Option<Vec<u8>>>(nodes)?;
        q.exact_v1::<Option<Vec<Digest>>>(nodes)?;
        q.exact_v1::<Digest>(nodes)?;
        q.exact_v1::<Tensor>(nodes)?;
        for node in &block.nodes {
            let dim = |d: &Dim| match d {
                Dim::Fixed(n) => u64::from(*n),
                Dim::H => h,
            };
            let elements = node.out.shape.iter().map(dim).try_fold(1u64, product)?;
            let row_len = node.out.shape.last().map(dim).unwrap_or(elements);
            let rows = if node.out.shape.len() <= 1 {
                1
            } else if row_len == 0 {
                0
            } else {
                elements / row_len
            };
            let row_leaves = product(rows, row_len.div_ceil(misaka_palw_kernel::merkle3::TILE_V3))?;
            let hidden_leaves = misaka_palw_kernel::hide::hide_tiles_v1(elements);
            chunk_upper = add(chunk_upper, add(1, add(row_leaves, hidden_leaves)?)?)?;
            // Clear rows concatenate at the declared dtype width before
            // Tensor::from_le_bytes widens to actual i128 lanes. Hidden mode
            // retains tile hashes instead; counting both is a conservative
            // typed bound independent of attacker-supplied chunk variants.
            q.vec_v1::<u8>(product(elements, node.out.dtype.width() as u64)?)?;
            q.exact_v1::<i128>(elements)?;
            q.vec_v1::<Digest>(hidden_leaves)?;
            q.vec_v1::<usize>(node.out.shape.len() as u64)?; // resolved shape
            q.vec_v1::<usize>(node.out.shape.len() as u64)?; // Tensor shape clone
            q.exact_v1::<usize>(1)?; // withheld/hiding placeholder [0]
        }
    }
    // Layout creation is already quoted without reducing its pre-refusal
    // allocations to SEG_MAX_PARTS. by_part is constructed only after that
    // helper succeeds, so its actual returned length has the explicit cap.
    let by_part = add(chunk_upper, 1)?.min(u64::from(SEG_MAX_PARTS_V4));
    q.exact_v1::<Option<SegPartResponseV1>>(by_part)?;
    Ok(q.bytes_v1())
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::{DType, Ref, TensorType, builder::ProgramBuilder, program::HISTORY_BOUND_V1_HELD};

    fn program(width: u32) -> TirProgramV1 {
        let mut b = ProgramBuilder::new(4, HISTORY_BOUND_V1_HELD);
        let weight = b.param("weight", DType::I32, &[width], false);
        let mut pre = b.block("pre", vec![]);
        let value = pre.cast(weight, DType::I32);
        let pre = pre.finish(&[value]);
        let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[width])]);
        post.commit(Ref::CarryIn(0));
        let post = post.finish(&[]);
        b.finish(pre, vec![], post, 0)
    }
    fn empty_part(part: u32) -> Vec<u8> {
        borsh::to_vec(&SegPartResponseV1 { part, position_root: [0; 64], position_siblings: vec![], chunks: vec![] }).unwrap()
    }
    #[test]
    fn assembly_quote_is_typed_and_actual_dimensions_change_its_request() {
        let parts = [empty_part(0)];
        assert!(
            native_kernel_segment_assembly_requests_v4(&program(4096), 0, &parts).unwrap()
                > native_kernel_segment_assembly_requests_v4(&program(1), 0, &parts).unwrap()
        );
    }
    #[test]
    fn quote_refuses_duplicate_or_unframed_parts_before_assembler_decode() {
        let p = program(4);
        assert!(native_kernel_segment_assembly_requests_v4(&p, 0, &[empty_part(0), empty_part(0)]).is_err());
        assert!(native_kernel_segment_assembly_requests_v4(&p, 0, &[empty_part(1)]).is_err());
        let mut truncated = empty_part(0);
        truncated.pop();
        assert!(native_kernel_segment_assembly_requests_v4(&p, 0, &[truncated]).is_err());
        let mut hostile = empty_part(0);
        // part u32 + position root; position_siblings Vec length must not
        // allocate its declared count before the bounded structural walk.
        hostile[68..72].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(native_kernel_segment_assembly_requests_v4(&p, 0, &[hostile]).is_err());
        assert!(native_kernel_segment_assembly_requests_v4(&p, p.token_bound, &[empty_part(0)]).is_err());
    }
}
