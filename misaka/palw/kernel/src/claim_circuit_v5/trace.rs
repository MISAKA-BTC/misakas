//! Whole registered TIR-v1 schedules, positions, carries and state are wired
//! into one semantic circuit. This is a circuit constructor, not a proof or a
//! native V5 activation. A future native proof contract must authenticate the
//! returned public binding against its real claim/job and artifact scope.
use super::native_commitment::{NativeTensorCommitmentV5, native_tensor_workspace_bytes_v5};
use super::tensor::{CircuitTensorV5, NativeTensorEndpointV5, constrain_endpoint_v5, emit_primitive_v5};
use super::*;
use crate::{
    ledger::{ClassRowV1, KernelLedgerV1},
    trace::EvidenceV1,
};
use misaka_palw_tir::{
    DType, Dim, Prim, TensorType,
    program::{MAX_CARRY, MAX_NODE_INPUTS, Ref, StateKind, TirProgramV1},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProgramTraceBindingV5 {
    pub class: Digest,
    pub program: Digest,
    pub parameters: Digest,
    pub evidence: Digest,
    pub output: Digest,
    pub public_tokens: Digest,
    pub positions: u32,
}
pub struct ReservedProgramTraceV5 {
    binding: ProgramTraceBindingV5,
    graph: ReservedCircuitV5,
}
impl ReservedProgramTraceV5 {
    pub fn binding(&self) -> &ProgramTraceBindingV5 {
        &self.binding
    }
    pub fn graph(&self) -> &ReservedCircuitV5 {
        &self.graph
    }
}
#[derive(Clone, Copy)]
struct StateValue {
    state: u16,
    layer: Option<u16>,
    position: Option<u32>,
    handle: usize,
}
#[derive(Clone, Copy)]
struct Occurrence {
    block: usize,
    layer: Option<u16>,
}
fn occurrence(program: &TirProgramV1, at: usize) -> Result<Occurrence, String> {
    let count = program.schedule.layers.len().checked_add(2).ok_or("trace schedule overflow")?;
    let (block, layer) = if at == 0 {
        (program.schedule.pre, None)
    } else if at + 1 == count {
        (program.schedule.post, None)
    } else {
        (
            *program.schedule.layers.get(at - 1).ok_or("trace occurrence absent")?,
            Some(u16::try_from(at - 1).map_err(|_| "trace layer overflow")?),
        )
    };
    if block as usize >= program.blocks.len() {
        return Err("trace block absent".into());
    }
    Ok(Occurrence { block: block as usize, layer })
}
struct ResolvedShape {
    dims: [usize; misaka_palw_tir::types::MAX_RANK],
    rank: usize,
}
impl ResolvedShape {
    fn fixed(dims: &[u32]) -> Result<Self, String> {
        if dims.len() > misaka_palw_tir::types::MAX_RANK {
            return Err("trace rank exceeds TIR-v1".into());
        }
        let mut out = Self { dims: [0; misaka_palw_tir::types::MAX_RANK], rank: dims.len() };
        for (i, d) in dims.iter().enumerate() {
            out.dims[i] = *d as usize;
        }
        super::tensor::CircuitShapeV5::new(out.as_slice())?;
        Ok(out)
    }
    fn symbolic(ty: &TensorType, h: usize) -> Result<Self, String> {
        if ty.rank() > misaka_palw_tir::types::MAX_RANK {
            return Err("trace rank exceeds TIR-v1".into());
        }
        let mut out = Self { dims: [0; misaka_palw_tir::types::MAX_RANK], rank: ty.rank() };
        for (i, d) in ty.shape.iter().enumerate() {
            out.dims[i] = match d {
                Dim::Fixed(n) => *n as usize,
                Dim::H => h,
            };
        }
        super::tensor::CircuitShapeV5::new(out.as_slice())?;
        Ok(out)
    }
    fn as_slice(&self) -> &[usize] {
        &self.dims[..self.rank]
    }
}
fn reserved_vec<T>(count: usize) -> Result<Vec<T>, String> {
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|e| e.to_string())?;
    if values.capacity() != count {
        return Err("trace header capacity differs from typed plan".into());
    }
    Ok(values)
}
fn push<T>(values: &mut Vec<T>, value: T) -> Result<usize, String> {
    if values.len() >= values.capacity() {
        return Err("trace exceeds its reserved header count".into());
    }
    let index = values.len();
    values.push(value);
    Ok(index)
}
/// Compare decoded cache data with the class's exact immutable bytes without
/// serializing a second program-sized Vec. Registration owns validation of the
/// class; this constructor never substitutes an unrelated decoded program.
struct ExactBytes<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl std::io::Write for ExactBytes<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self.at.checked_add(bytes.len()).ok_or_else(|| std::io::Error::other("program comparison overflow"))?;
        if self.bytes.get(self.at..end) != Some(bytes) {
            return Err(std::io::Error::other("decoded program cache differs from registered bytes"));
        }
        self.at = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct TraceCounts {
    arena: usize,
    writes: usize,
    appends: usize,
    maximum_prior: usize,
}
fn counts(class: &ClassRowV1, tokens: &[u32], evidence: &EvidenceV1) -> Result<TraceCounts, String> {
    let program = &class.program;
    if tokens.is_empty()
        || tokens.len() > class.plan.max_positions as usize
        || tokens.len() > program.history_bound as usize
        || evidence.commitments.len() != tokens.len()
        || !evidence.inputs.is_empty()
        || program.blocks.len() > misaka_palw_tir::program::MAX_BLOCKS
        || program.schedule.layers.len() > misaka_palw_tir::program::MAX_LAYERS
        || program.states.len() > misaka_palw_tir::program::MAX_STATES
        || class.program_bytes.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES
        || program.params.len() > misaka_palw_tir::program::MAX_PARAMS
    {
        return Err("registered program trace dimensions outside finite native scope".into());
    }
    let mut encoded = ExactBytes { bytes: &class.program_bytes, at: 0 };
    BorshSerialize::serialize(program, &mut encoded).map_err(|e| e.to_string())?;
    if encoded.at != class.program_bytes.len() || crate::public::program_root_v1(&class.program_bytes) != class.plan.program_root {
        return Err("registered program bytes/root mismatch".into());
    }
    let uses_token = program.blocks.iter().flat_map(|b| &b.nodes).any(|n| n.inputs.contains(&Ref::Input(0)));
    if uses_token && tokens.iter().any(|t| *t >= program.token_bound) {
        return Err("trace token outside native bound".into());
    }
    let occurrences = program.schedule.layers.len() + 2;
    let mut nodes = 0usize;
    let mut writes = 0usize;
    let mut appends = 0usize;
    let mut maximum_prior = 0usize;
    let mut initial_fixed = 0usize;
    for state in &program.states {
        ResolvedShape::fixed(&state.shape)?;
        match state.kind {
            StateKind::Fixed { lo, hi } => {
                if lo > 0 || hi < 0 || lo > hi || !state.dtype.contains(lo as i128) || !state.dtype.contains(hi as i128) {
                    return Err("registered Fixed state interval malformed".into());
                }
                initial_fixed = initial_fixed
                    .checked_add(if state.per_layer { program.schedule.layers.len() } else { 1 })
                    .ok_or("fixed instance count overflow")?;
            }
            StateKind::Hist { window } => {
                if window == 0 || window > program.history_bound {
                    return Err("registered history window malformed".into());
                }
                maximum_prior = maximum_prior.max(tokens.len().saturating_sub(1).min(window as usize - 1));
            }
        }
    }
    for at in 0..occurrences {
        let occ = occurrence(program, at)?;
        let block = &program.blocks[occ.block];
        if block.nodes.is_empty()
            || block.nodes.len() > misaka_palw_tir::program::MAX_NODES_PER_BLOCK
            || block.carry_in.len() > MAX_CARRY
            || block.carry_out.len() > MAX_CARRY
        {
            return Err("trace block dimensions outside native scope".into());
        }
        nodes = nodes.checked_add(block.nodes.len()).ok_or("trace node count overflow")?;
        for node in &block.nodes {
            match node.prim {
                Prim::StateWrite { .. } => writes = writes.checked_add(1).ok_or("trace write count overflow")?,
                Prim::HistAppend { .. } => appends = appends.checked_add(1).ok_or("trace append count overflow")?,
                _ => {}
            }
        }
        for position in &evidence.commitments {
            if position.len() != occurrences || position[at].len() != block.nodes.len() {
                return Err("native evidence shape differs from registered schedule".into());
            }
        }
    }
    let arena = class
        .param_commitments
        .by_instance
        .len()
        .checked_add(program.consts.len())
        .and_then(|n| n.checked_add(initial_fixed))
        .and_then(|n| n.checked_add(nodes.checked_add(2)?.checked_mul(tokens.len())?))
        .ok_or("trace arena header count overflow")?;
    Ok(TraceCounts {
        arena,
        writes: writes.checked_mul(tokens.len()).and_then(|n| n.checked_add(initial_fixed)).ok_or("trace write count overflow")?,
        appends: appends.checked_mul(tokens.len()).ok_or("trace append count overflow")?,
        maximum_prior,
    })
}
fn state_key(program: &TirProgramV1, state: u16, layer: Option<u16>) -> Result<Option<u16>, String> {
    let state = program.states.get(state as usize).ok_or("trace state absent")?;
    if state.per_layer != layer.is_some() {
        return Err("state instance differs from registered block role".into());
    }
    Ok(layer)
}
fn emit_trace<'a>(
    builder: &mut CircuitBuilderV5,
    class: &'a ClassRowV1,
    tokens: &[u32],
    evidence: &EvidenceV1,
    plan: &TraceCounts,
    memory: &CircuitMemoryBudgetV5,
) -> Result<(), String> {
    let program = &class.program;
    let suite = match class.descriptor.commitment_suite_id {
        2 => NativeTensorCommitmentV5::DualRootV2,
        3 => NativeTensorCommitmentV5::TiledDualRootV3,
        _ => return Err("registered descriptor's tensor commitment suite is not implemented by this trace compiler".into()),
    };
    let mut arena: Vec<CircuitTensorV5<'a>> = reserved_vec(plan.arena)?;
    let mut writes: Vec<StateValue> = reserved_vec(plan.writes)?;
    let mut appends: Vec<StateValue> = reserved_vec(plan.appends)?;
    for ((j, layer), root) in &class.param_commitments.by_instance {
        let decl = program.params.get(*j as usize).ok_or("native parameter commitment has no declaration")?;
        if decl.per_layer != layer.is_some() || layer.is_some_and(|l| l as usize >= program.schedule.layers.len()) {
            return Err("native parameter layer differs from registered declaration".into());
        }
        let shape = ResolvedShape::fixed(&decl.shape)?;
        let value = CircuitTensorV5::input(builder, decl.dtype, shape.as_slice())?;
        constrain_endpoint_v5(
            builder,
            NativeTensorEndpointV5 { dtype: decl.dtype, shape: shape.as_slice(), suite, root: *root },
            &value,
        )?;
        push(&mut arena, value)?;
    }
    let const_base = arena.len();
    for decl in &program.consts {
        let shape = ResolvedShape::fixed(&decl.shape)?;
        let value = CircuitTensorV5::raw_constant(decl.dtype, shape.as_slice(), &decl.data)?;
        push(&mut arena, value)?;
    }
    for (j, state) in program.states.iter().enumerate() {
        if !matches!(state.kind, StateKind::Fixed { .. }) {
            continue;
        }
        let shape = ResolvedShape::fixed(&state.shape)?;
        let instances = if state.per_layer { program.schedule.layers.len() } else { 1 };
        for at in 0..instances {
            let layer = if state.per_layer { Some(at as u16) } else { None };
            let handle = push(&mut arena, CircuitTensorV5::repeated(state.dtype, shape.as_slice(), 0)?)?;
            push(&mut writes, StateValue { state: j as u16, layer, position: None, handle })?;
        }
    }
    let placeholder = CircuitTensorV5::repeated(DType::I8, &[], 0)?;
    for (position, token) in tokens.iter().enumerate() {
        let position = u32::try_from(position).map_err(|_| "trace position overflow")?;
        let token_handle = push(&mut arena, CircuitTensorV5::repeated(DType::Idx, &[], *token as i128)?)?;
        let position_handle = push(&mut arena, CircuitTensorV5::repeated(DType::Idx, &[], position as i128)?)?;
        let mut carry = [None; MAX_CARRY];
        for at in 0..program.schedule.layers.len() + 2 {
            let occ = occurrence(program, at)?;
            let block = &program.blocks[occ.block];
            let base = arena.len();
            let mut window = None;
            for node in &block.nodes {
                if let Prim::HistAppend { state } = node.prim {
                    let state = program.states.get(state as usize).ok_or("trace history absent")?;
                    let StateKind::Hist { window: w } = state.kind else {
                        return Err("trace append targets Fixed state".into());
                    };
                    if window.is_some_and(|old| old != w) {
                        return Err("trace block mixes history windows".into());
                    }
                    window = Some(w);
                }
            }
            let h = window.map_or(1, |w| (position as usize + 1).min(w as usize));
            for (ni, node) in block.nodes.iter().enumerate() {
                let arity = node.prim.arity();
                if node.inputs.len() > MAX_NODE_INPUTS || node.inputs.len() < arity.0 || node.inputs.len() > arity.1 {
                    return Err("registered trace node arity malformed".into());
                }
                let mut handles = [0usize; MAX_NODE_INPUTS];
                for (i, source) in node.inputs.iter().enumerate() {
                    handles[i] = match *source {
                        Ref::Node(j) => {
                            if j as usize >= ni {
                                return Err("trace node reference is not backward".into());
                            }
                            base + j as usize
                        }
                        Ref::CarryIn(k) => carry.get(k as usize).copied().flatten().ok_or("trace carry absent")?,
                        Ref::Param(j) => {
                            let decl = program.params.get(j as usize).ok_or("trace parameter declaration absent")?;
                            let layer = if decl.per_layer { occ.layer } else { None };
                            if decl.per_layer && layer.is_none() {
                                return Err("per-layer parameter outside a layer".into());
                            }
                            class
                                .param_commitments
                                .by_instance
                                .keys()
                                .position(|key| *key == (j, layer))
                                .ok_or("trace artifact parameter commitment absent")?
                        }
                        Ref::Const(j) => {
                            if j as usize >= program.consts.len() {
                                return Err("trace constant absent".into());
                            }
                            const_base + j as usize
                        }
                        Ref::State(j) => {
                            let layer = state_key(program, j, occ.layer)?;
                            writes
                                .iter()
                                .rev()
                                .find(|v| v.state == j && v.layer == layer && v.position.is_none_or(|p| p < position))
                                .ok_or("trace Fixed entry state absent")?
                                .handle
                        }
                        Ref::Input(0) => token_handle,
                        Ref::Input(1) => position_handle,
                        Ref::Input(_) => return Err("trace unknown public input".into()),
                    };
                }
                // Prior writes of this same position are deliberately invisible.
                // Hist rows are the last min(position,window-1) earlier appends.
                let mut history_key = None;
                let result = {
                    let mut prior: Vec<&CircuitTensorV5<'a>> = reserved_vec(plan.maximum_prior)?;
                    if let Prim::HistAppend { state } = node.prim {
                        let layer = state_key(program, state, occ.layer)?;
                        let StateKind::Hist { window } = program.states[state as usize].kind else {
                            return Err("trace history kind".into());
                        };
                        let want = (position as usize).min(window as usize - 1);
                        let first = position.saturating_sub(want as u32);
                        for row in appends
                            .iter()
                            .filter(|v| v.state == state && v.layer == layer && v.position.is_some_and(|p| p >= first && p < position))
                        {
                            let value = arena.get(row.handle).ok_or("trace history handle invalid")?;
                            push(&mut prior, value)?;
                        }
                        if prior.len() != want {
                            return Err("trace history continuity has missing or duplicate positions".into());
                        }
                        history_key = Some((state, layer));
                    }
                    let shape = ResolvedShape::symbolic(&node.out, h)?;
                    let refs: [&CircuitTensorV5<'a>; MAX_NODE_INPUTS] =
                        std::array::from_fn(|i| if i < node.inputs.len() { &arena[handles[i]] } else { &placeholder });
                    let result = emit_primitive_v5(
                        builder,
                        &node.prim,
                        &refs[..node.inputs.len()],
                        node.out.dtype,
                        shape.as_slice(),
                        &program.states,
                        &prior,
                        memory,
                    )?;
                    let root = *evidence.at(position, at as u16, ni as u16).ok_or("trace native commitment absent")?;
                    constrain_endpoint_v5(
                        builder,
                        NativeTensorEndpointV5 { dtype: node.out.dtype, shape: shape.as_slice(), suite, root },
                        &result,
                    )?;
                    // This lexical scope drops borrowed history/operand references
                    // before arena mutation. The result owns wire IDs, not pointers.
                    result
                };
                let handle = push(&mut arena, result)?;
                match node.prim {
                    Prim::StateWrite { state } => {
                        let layer = state_key(program, state, occ.layer)?;
                        if writes.iter().any(|v| v.state == state && v.layer == layer && v.position == Some(position)) {
                            return Err("trace writes one state instance twice in a position".into());
                        }
                        push(&mut writes, StateValue { state, layer, position: Some(position), handle })?;
                    }
                    Prim::HistAppend { .. } => {
                        let (state, layer) = history_key.ok_or("trace history instance absent")?;
                        if appends.iter().any(|v| v.state == state && v.layer == layer && v.position == Some(position)) {
                            return Err("trace appends one state instance twice in a position".into());
                        }
                        push(&mut appends, StateValue { state, layer, position: Some(position), handle: handles[0] })?;
                    }
                    _ => {}
                }
            }
            carry = [None; MAX_CARRY];
            for (i, node) in block.carry_out.iter().enumerate() {
                if *node as usize >= block.nodes.len() {
                    return Err("trace carry output absent".into());
                }
                carry[i] = Some(base + *node as usize);
            }
        }
    }
    if arena.len() != plan.arena || writes.len() != plan.writes || appends.len() != plan.appends {
        return Err("whole trace header use differs from preflight".into());
    }
    Ok(())
}
/// Compile all native node commitments for a registered single-program class.
/// Parameters are witness inputs in the exact registered BTreeMap order; all
/// tokens and positions are public constants. Positions begin at zero, so no
/// caller-selected initial recurrence/history witness is accepted.
pub fn compile_registered_program_trace_v5(
    ledger: &KernelLedgerV1,
    class: &Digest,
    tokens: &[u32],
    evidence: &EvidenceV1,
    limits: CircuitLimitsV5,
    memory: CircuitMemoryBudgetV5,
) -> Result<ReservedProgramTraceV5, String> {
    let row = ledger.classes.get(class).ok_or("registered trace class absent")?;
    let plan = counts(row, tokens, evidence)?;
    let metadata = plan
        .arena
        .checked_mul(std::mem::size_of::<CircuitTensorV5<'_>>())
        .and_then(|n| n.checked_add(plan.writes.checked_add(plan.appends)?.checked_mul(std::mem::size_of::<StateValue>())?))
        .and_then(|n| n.checked_add(plan.maximum_prior.checked_mul(std::mem::size_of::<&CircuitTensorV5<'_>>())?))
        .ok_or("whole trace metadata reservation overflow")?;
    let _metadata = memory.reserve(metadata as u64)?;
    let _hash = memory.reserve(native_tensor_workspace_bytes_v5())?;
    let mut counting = CircuitBuilderV5::preflight(limits)?;
    emit_trace(&mut counting, row, tokens, evidence, &plan, &memory)?;
    let cost = counting.preflight_cost()?;
    let mut builder = CircuitBuilderV5::new_planned(limits, cost, memory.clone())?;
    emit_trace(&mut builder, row, tokens, evidence, &plan, &memory)?;
    let graph = builder.finish_reserved()?;
    let mut token_hash = crate::hash::keyed(b"MISAKA/PALW/CIRCUIT-PUBLIC-TOKENS/V5");
    token_hash.update(&(tokens.len() as u64).to_le_bytes());
    for token in tokens {
        token_hash.update(&token.to_le_bytes());
    }
    let binding = ProgramTraceBindingV5 {
        class: *class,
        program: row.plan.program_root,
        parameters: row.param_commitments.root(),
        evidence: evidence.root(),
        output: evidence.output_root(&row.program),
        public_tokens: crate::hash::finish(token_hash),
        positions: u32::try_from(tokens.len()).map_err(|_| "trace positions overflow")?,
    };
    Ok(ReservedProgramTraceV5 { binding, graph })
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::{MapParams, builder::ProgramBuilder};
    fn fixture() -> (ClassRowV1, Vec<u32>, EvidenceV1) {
        let mut p = ProgramBuilder::new(10, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
        let state = p.fixed_state("counter", DType::I8, &[], -10, 10, false);
        let hist = p.hist_state("history", DType::I8, &[], 2, false);
        let one = p.scalar(DType::I8, 1);
        let mut pre = p.block("pre", vec![]);
        let value = pre.add(Ref::State(state), one, DType::I8);
        pre.commit(value);
        pre.state_write(state, value);
        pre.hist_append(hist, value);
        let pre = pre.finish(&[value]);
        let mut post = p.block("post", vec![TensorType::scalar(DType::I8)]);
        // The post block must read the previous position's entry state even
        // though the pre block has already calculated this position's write.
        let value = post.add(Ref::CarryIn(0), Ref::State(state), DType::I8);
        post.commit(value);
        let Ref::Node(logits) = value else { unreachable!() };
        let post = post.finish(&[]);
        let program = p.finish(pre, vec![], post, logits);
        misaka_palw_tir::validate::validate(&program).unwrap();
        let descriptor = crate::descriptor::k2_tir_v2_descriptor();
        let bytes = program.encode();
        let plan = crate::plan::plan_for_tir_program_v1(&descriptor, &program, crate::public::program_root_v1(&bytes), 2).unwrap();
        let bounds = crate::gate::class_prosecution_bounds_v1(
            &descriptor,
            &plan,
            &program,
            &crate::public::ProfileMaterialV1::kernel_route(true),
            &crate::gate::ProsecutionPolicyV1 {
                court_deadline_daa: 20,
                max_sessions_per_claim: 2,
                max_public_bytes: 1 << 40,
                max_verifier_ram: 1 << 36,
                max_retained_state: 1 << 32,
            },
        )
        .unwrap();
        let tokens = vec![1, 2];
        let trace = crate::trace::trace_v1(&program, &MapParams::default(), &tokens).unwrap();
        assert_eq!(trace.values[0][1][logits as usize].data, vec![1]);
        assert_eq!(trace.values[1][1][logits as usize].data, vec![3]);
        let evidence = trace.evidence();
        (
            ClassRowV1 {
                descriptor,
                program_bytes: bytes,
                program,
                plan,
                param_commitments: Default::default(),
                network_domain: [9; 64],
                ruleset_digest: [8; 64],
                bounds,
                liability_daa: 100,
                court_scope: None,
            },
            tokens,
            evidence,
        )
    }
    fn limits() -> CircuitLimitsV5 {
        CircuitLimitsV5 {
            maximum_gates: 40_000_000,
            maximum_input_bits: 1_000_000,
            maximum_assertions: 1_000_000,
            maximum_graph_bytes: 800_000_000,
            maximum_witness_bytes: 40_000_000,
        }
    }
    fn compile(
        row: &ClassRowV1,
        tokens: &[u32],
        evidence: &EvidenceV1,
        memory: CircuitMemoryBudgetV5,
    ) -> Result<ReservedCircuitV5, String> {
        let count = counts(row, tokens, evidence)?;
        // Test helper mirrors the production aggregate reservation; bypassing
        // ledger lookup here tests the pure relation, not native admission.
        let bytes = count.arena * std::mem::size_of::<CircuitTensorV5<'_>>()
            + (count.writes + count.appends) * std::mem::size_of::<StateValue>()
            + count.maximum_prior * std::mem::size_of::<&CircuitTensorV5<'_>>();
        let _metadata = memory.reserve(bytes as u64)?;
        let _hash = memory.reserve(native_tensor_workspace_bytes_v5())?;
        let mut counter = CircuitBuilderV5::preflight(limits())?;
        emit_trace(&mut counter, row, tokens, evidence, &count, &memory)?;
        let plan = counter.preflight_cost()?;
        let mut builder = CircuitBuilderV5::new_planned(limits(), plan, memory.clone())?;
        emit_trace(&mut builder, row, tokens, evidence, &count, &memory)?;
        builder.finish_reserved()
    }
    #[test]
    fn full_positions_wire_entry_state_history_and_carries_to_native_roots() {
        let (row, tokens, evidence) = fixture();
        let memory = CircuitMemoryBudgetV5::new(900_000_000);
        let graph = compile(&row, &tokens, &evidence, memory.clone()).unwrap();
        graph.evaluate(&[], limits()).unwrap();
        drop(graph);
        assert_eq!(memory.held(), 0);
        let mut wrong = evidence;
        wrong.commitments[1][1][row.program.logits as usize] =
            crate::merkle::tensor_commitment_v2(&misaka_palw_tir::Tensor::scalar(DType::I8, 4).unwrap());
        let graph = compile(&row, &tokens, &wrong, memory.clone()).unwrap();
        assert!(graph.evaluate(&[], limits()).is_err());
        drop(graph);
        assert_eq!(memory.held(), 0);
    }
    #[test]
    fn trace_refuses_cache_substitution_evidence_shape_and_metadata_allocation() {
        let (mut row, tokens, evidence) = fixture();
        let memory = CircuitMemoryBudgetV5::new(0);
        assert!(compile(&row, &tokens, &evidence, memory.clone()).is_err());
        assert_eq!(memory.held(), 0);
        row.program.logits_scheme_id[0] ^= 1;
        assert!(counts(&row, &tokens, &evidence).is_err());
        let (row, tokens, mut evidence) = fixture();
        evidence.commitments[1].pop();
        assert!(counts(&row, &tokens, &evidence).is_err());
    }
}
