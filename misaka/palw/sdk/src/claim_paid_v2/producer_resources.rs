//! Deterministic workspace bound for the finite private Program producer.
//! Matches trace_stage_v1 (whole retained trace), eval.rs, TensorWire encoding
//! and Merkle roots. No protocol economic/security/resource approval is inferred.
use super::*;
use misaka_palw_kernel::{job::KernelJobV1, ledger::ClassRowV1, public::TensorWireV1};
use misaka_palw_tir::{
    DType, Prim, TensorType,
    program::{Ref, StateKind},
};
use std::collections::BTreeMap;
const HEADER: u128 = 512; // Tensor/Vec/BTree entry + rank<=12 dimensions, separate maps
fn fail() -> Refusal {
    Refusal::new("BOOTSTRAP_RESOURCE", "private producer resource arithmetic overflow")
}
fn add(a: u128, b: u128) -> Result<u128, Refusal> {
    a.checked_add(b).ok_or_else(fail)
}
fn mul(a: u128, b: u128) -> Result<u128, Refusal> {
    a.checked_mul(b).ok_or_else(fail)
}
fn tensor(elements: u128, bytes: u128) -> Result<u128, Refusal> {
    add(mul(elements, bytes)?, HEADER)
}
fn input_type(class: &ClassRowV1, block: usize, input: &Ref) -> TensorType {
    let p = &class.program;
    match *input {
        Ref::Node(i) => p.blocks[block].nodes[i as usize].out.clone(),
        Ref::CarryIn(i) => p.blocks[block].carry_in[i as usize].clone(),
        Ref::Param(i) => TensorType::fixed(p.params[i as usize].dtype, &p.params[i as usize].shape),
        Ref::Const(i) => TensorType::fixed(p.consts[i as usize].dtype, &p.consts[i as usize].shape),
        Ref::State(i) => TensorType::fixed(p.states[i as usize].dtype, &p.states[i as usize].shape),
        Ref::Input(_) => TensorType::fixed(DType::Idx, &[]),
    }
}
/// Temporary bytes in addition to cloned operands and retained output. Vec
/// growth in fallible collect is allowed at up to twice the final data count.
/// Output Vecs are priced separately at 32 bytes/element for i128 allocation.
/// All current Prim variants are explicit: a new variant requires this audit.
fn temporary_bytes(prim: &Prim, out: u128) -> Result<u128, Refusal> {
    let bytes_per_output = match prim {
        // Row/shape index vectors have rank<=12 and are consumed one at a time.
        Prim::Reshape
        | Prim::Cast
        | Prim::Transpose { .. }
        | Prim::Slice { .. }
        | Prim::Concat { .. }
        | Prim::Iota { .. }
        | Prim::Gather { .. }
        | Prim::ReduceSum { .. }
        | Prim::ReduceMax { .. }
        | Prim::Clamp { .. }
        | Prim::Log2Floor
        | Prim::IntExp
        | Prim::IntRsqrt
        | Prim::IntLn
        | Prim::StateWrite { .. } => 0,
        Prim::Broadcast => 16, // one usize map, including doubling
        Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => 32, // two maps
        Prim::Select => 48,    // three maps
        Prim::MatMul => 32,    // two batch maps, batch count <= output elements
        Prim::TopK { .. } => 48, // <=k (Reverse<i128>,usize) heap plus k-index result;
        // >64 branch holds at most 2k usize, no score clones
        Prim::HistAppend { .. } => 32, // final concat buffer and prior row copying
    };
    // shape/stride/unravel vectors and fixed per-call validation scratch;
    // primitive arithmetic algorithms use fixed iteration scalar state.
    add(mul(out, bytes_per_output)?, 4096)
}
#[derive(Clone, Copy, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PrivateProgramResourceBoundV3 {
    pub workspace_bytes: u128,
    pub material_bytes: u128,
    pub execution_work: u128,
}
pub fn bound_program_work_v3(
    class: &ClassRowV1,
    job: &KernelJobV1,
    parameters: &BTreeMap<(u16, Option<u16>), TensorWireV1>,
) -> Result<PrivateProgramResourceBoundV3, Refusal> {
    job.well_formed(class.program.token_bound, class.plan.max_positions).map_err(|e| Refusal::new("BOOTSTRAP_JOB", e))?;
    let positions = add(job.prompt.len() as u128, u128::from(job.max_new_tokens))?.checked_sub(1).ok_or_else(fail)?;
    let info = misaka_palw_tir::validate::validate(&class.program).map_err(|e| Refusal::new("BOOTSTRAP_RESOURCE", e.to_string()))?;
    let mut position_heap = 0;
    let mut maximum_input = 0;
    let mut maximum_temporary = 0;
    let mut maximum_elements = 1;
    let mut nodes = 0;
    for (block, _) in class.program.occurrences() {
        let bi = block as usize;
        let h = (info.blocks[bi].window.unwrap_or(1) as u64).min(positions as u64).max(1);
        for node in &class.program.blocks[bi].nodes {
            let output = u128::from(node.out.elements_at(h));
            maximum_elements = maximum_elements.max(output);
            position_heap = add(position_heap, tensor(output, 32)?)?;
            nodes = add(nodes, 1)?;
            let mut inputs = 0;
            for reference in &node.inputs {
                let elements = u128::from(input_type(class, bi, reference).elements_at(h));
                maximum_elements = maximum_elements.max(elements);
                inputs = add(inputs, tensor(elements, 16)?)?;
            }
            if let Prim::HistAppend { state } = node.prim {
                let state = &class.program.states[state as usize];
                let rows = match state.kind {
                    StateKind::Hist { window } => u128::from(window).min(positions).saturating_sub(1),
                    StateKind::Fixed { .. } => 0,
                };
                let elements = state.shape.iter().try_fold(1u128, |n, d| n.checked_mul(u128::from(*d))).ok_or_else(fail)?;
                // hist_prior_sources returns a Vec of SourceV1 then clones each
                // row into a Vec<Tensor>; both coexist during collection.
                inputs = add(inputs, mul(rows, add(tensor(elements, 16)?, 128)?)?)?;
            }
            maximum_input = maximum_input.max(inputs);
            maximum_temporary = maximum_temporary.max(temporary_bytes(&node.prim, output)?);
        }
    }
    let trace_heap = mul(position_heap, positions)?;
    let mut decoded_artifact = 0;
    for wire in parameters.values() {
        let elements = wire.shape.iter().try_fold(1u128, |n, d| n.checked_mul(u128::from(*d))).ok_or_else(fail)?;
        maximum_elements = maximum_elements.max(elements);
        decoded_artifact = add(decoded_artifact, tensor(elements, 16)?)?;
    }
    let mut constant_scratch = 0;
    for constant in &class.program.consts {
        let elements = constant.shape.iter().try_fold(1u128, |n, d| n.checked_mul(u128::from(*d))).ok_or_else(fail)?;
        constant_scratch = constant_scratch.max(tensor(elements, 16)?);
    }
    // trace_stage_v1 resolves Fixed state from the retained prior output and
    // Hist rows from retained committed appenders; it creates no hidden state
    // checkpoint or history cache. Whole trace + explicit prior clones above
    // cover them. Greedy loop drops each trace before the next iteration.
    let artifact_wire = borsh::object_length(parameters).map_err(|e| Refusal::new("BOOTSTRAP_RESOURCE", e.to_string()))? as u128;
    // Merkle row/column leaves are collected sequentially. Allow doubled leaf
    // Vec capacity + next level concurrently (<=3*64*N), plus one copied leaf.
    let merkle_scratch = add(mul(maximum_elements, 224)?, 4096)?;
    let commitment_rows = mul(mul(nodes, positions)?, add(64, HEADER)?)?;
    let program_metadata = add(class.program_bytes.len() as u128, class.plan.encoded_len() as u128)?;
    // TensorWire::of material lives beside original trace and decoded params;
    // work serialization can retain that material plus a growing Vec (<=2x).
    // Four whole trace allowances: resident i128 trace + material tensors +
    // two serialized material capacities. No other complete trace is live.
    let material = add(trace_heap, artifact_wire)?;
    let mut workspace = mul(trace_heap, 4)?;
    for extra in [
        decoded_artifact,
        mul(artifact_wire, 3)?,
        maximum_input,
        maximum_temporary,
        constant_scratch,
        merkle_scratch,
        commitment_rows,
        mul(program_metadata, 4)?,
        64 * 1024,
    ] {
        workspace = add(workspace, extra)?;
    }
    let actual_plan = misaka_palw_kernel::plan::plan_for_tir_program_v1(
        &class.descriptor,
        &class.program,
        class.plan.program_root,
        positions as u32,
    )
    .map_err(|e| Refusal::new("BOOTSTRAP_RESOURCE", e.to_string()))?;
    let (work, _, _) = misaka_palw_kernel::plan::reexecution_bounds_v1(&class.program, &actual_plan);
    let execution_work = mul(work, add(u128::from(job.max_new_tokens), 1)?)?;
    if workspace > 256 * 1024 * 1024 || material > 32 * 1024 * 1024 || execution_work > 100_000_000 {
        return Err(Refusal::new(
            "BOOTSTRAP_RESOURCE",
            "private producer exceeds 256 MiB workspace, 32 MiB material, or 100M work hard cap",
        ));
    }
    Ok(PrivateProgramResourceBoundV3 { workspace_bytes: workspace, material_bytes: material, execution_work })
}
