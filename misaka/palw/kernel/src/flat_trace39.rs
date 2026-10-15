//! Producer and independent reference replay over the SAME canonical39 body.
//! Every original node (including wide non-commit intermediates) is C_v3 bound.
//! Native accepts only the compact claim roots; this module mints no coins.
use crate::{
    flat_job39::{ActualInputResolverV39, CheckedJobMaximumV39, RequestStopV39},
    flat_point39::FlatClaimV39,
    flat_prepared39::PreparedV39,
    flat_source39::CheckedSourceV39,
    hash::{Digest, finish, keyed},
};
use borsh::BorshSerialize;
use misaka_palw_tir::{
    Tensor,
    interp::RunState,
    interp_v39::{InterpreterV39, ParamSourceV39},
};
pub trait TraceSinkV39 {
    /// A producer may stream typed bytes/leaf files; observers must not promote
    /// this position's provisional output until position_finished succeeds.
    fn node(&mut self, position: u32, occurrence: u16, node: u16, value: &Tensor, commitment: Digest) -> Result<(), String>;
    fn position_finished(&mut self, position: u32, root: Digest, output: &Tensor) -> Result<(), String>;
}
pub struct IgnoreTraceV39;
impl TraceSinkV39 for IgnoreTraceV39 {
    fn node(&mut self, _: u32, _: u16, _: u16, _: &Tensor, _: Digest) -> Result<(), String> {
        Ok(())
    }
    fn position_finished(&mut self, _: u32, _: Digest, _: &Tensor) -> Result<(), String> {
        Ok(())
    }
}
struct PositionFrontier {
    levels: [Option<Digest>; 64],
    count: u64,
}
fn pair(a: &Digest, b: &Digest) -> Digest {
    let mut h = keyed(crate::seg::SEG_POS_NODE_DOMAIN_V1);
    h.update(a).update(b);
    finish(h)
}
impl PositionFrontier {
    fn new() -> Self {
        Self { levels: [None; 64], count: 0 }
    }
    fn push(&mut self, mut x: Digest) -> Result<(), String> {
        for p in &mut self.levels {
            match p.take() {
                None => {
                    *p = Some(x);
                    self.count += 1;
                    return Ok(());
                }
                Some(a) => x = pair(&a, &x),
            }
        }
        Err("trace39 position frontier overflow".into())
    }
    fn finish(self, p: u32, expected: u64) -> Result<Digest, String> {
        if self.count != expected {
            return Err("trace39 all original nodes not observed exactly once".into());
        }
        let mut root = None;
        for x in self.levels.into_iter().flatten() {
            root = Some(match root {
                None => x,
                Some(r) => pair(&x, &r),
            });
        }
        let root = root.ok_or("trace39 empty position")?;
        let mut h = keyed(crate::seg::SEG_POS_ROOT_DOMAIN_V1);
        h.update(&p.to_le_bytes()).update(&expected.to_le_bytes()).update(&root);
        Ok(finish(h))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct TraceSummaryV39 {
    pub claim: FlatClaimV39,
    pub position_roots: Vec<Digest>,
    pub output_roots: Vec<Digest>,
}
/// Cold producer/reference replay. All generated IDs are checked against the
/// actual logits at their ORIGINAL prediction positions and firstEOS/max stop.
pub fn trace_streamed_v39(
    source: &CheckedSourceV39,
    maximum: &CheckedJobMaximumV39,
    inputs: &ActualInputResolverV39<'_>,
    params: &dyn ParamSourceV39,
    sink: &mut dyn TraceSinkV39,
) -> Result<TraceSummaryV39, String> {
    trace_impl_v39(source, maximum, inputs, params, sink, true)
}
/// Independent replay computes the arithmetic under the original claimed fed
/// IDs. Decode correctness is a separate public Greedy relation, never waived.
pub fn replay_streamed_v39(
    source: &CheckedSourceV39,
    maximum: &CheckedJobMaximumV39,
    inputs: &ActualInputResolverV39<'_>,
    params: &dyn ParamSourceV39,
    sink: &mut dyn TraceSinkV39,
) -> Result<TraceSummaryV39, String> {
    trace_impl_v39(source, maximum, inputs, params, sink, false)
}
fn trace_impl_v39(
    source: &CheckedSourceV39,
    maximum: &CheckedJobMaximumV39,
    inputs: &ActualInputResolverV39<'_>,
    params: &dyn ParamSourceV39,
    sink: &mut dyn TraceSinkV39,
    require_decode: bool,
) -> Result<TraceSummaryV39, String> {
    if inputs.source() != source.root() || inputs.maximum() != maximum.root() {
        return Err("trace39 original source/maximum/input derivation differs".into());
    }
    let actual = u32::try_from(inputs.tokens().len()).map_err(|_| "trace39 actual input length")?;
    let prepared = PreparedV39::new(source, actual)?;
    let evaluator = InterpreterV39::new(source.program()).map_err(|e| e.to_string())?;
    let mut state = RunState::default();
    let mut positions = Vec::with_capacity(actual as usize);
    let mut outputs = Vec::new();
    for (p, token) in inputs.tokens().iter().copied().enumerate() {
        let mut frontier = PositionFrontier::new();
        let mut next = 0u64;
        let output = evaluator
            .step_with_trace_observer(params, inputs, &mut state, token, &mut |point| {
                let occurrence = prepared.occurrences().iter().position(|x| *x == (point.block, point.layer)).ok_or_else(|| {
                    misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Operand, "trace39 original occurrence absent")
                })?;
                let index = prepared
                    .index(occurrence as u16, point.node)
                    .map_err(|e| misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Operand, e))?;
                if index != next || point.slot as u64 != index {
                    return Err(misaka_palw_tir::TirError::new(
                        misaka_palw_tir::TirErrorKind::Operand,
                        "trace39 duplicate/reordered original node",
                    ));
                }
                next += 1;
                let commitment =
                    crate::flat_material39::tensor_commitment_streamed_v39(point.value.dtype, &point.value.shape, |offset, count| {
                        let first = usize::try_from(offset).map_err(|_| "trace39 tensor range offset")?;
                        let end = first.checked_add(count).ok_or("trace39 tensor range overflow")?;
                        point.value.data.get(first..end).map(|x| x.to_vec()).ok_or("trace39 exact tensor range".into())
                    })
                    .map_err(|e| misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Operand, e))?;
                frontier
                    .push(crate::seg::node_leaf_v1(occurrence as u16, point.node, &commitment))
                    .map_err(|e| misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Operand, e))?;
                sink.node(p as u32, occurrence as u16, point.node, point.value, commitment)
                    .map_err(|e| misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Operand, e))
            })
            .map_err(|e| e.to_string())?;
        if require_decode {
            if let RequestStopV39::Decode { prompt, .. } = inputs.stop() {
                if p + 1 >= prompt.len() {
                    let generation = p + 1 - prompt.len();
                    let expected = *inputs.generated().get(generation).ok_or("trace39 actual generated prediction missing")?;
                    if output.data.len() != source.program().token_bound as usize {
                        return Err("trace39 actual logits do not match original vocabulary".into());
                    }
                    let best = output
                        .data
                        .iter()
                        .enumerate()
                        .max_by(|(i, a), (j, b)| a.cmp(b).then_with(|| j.cmp(i)))
                        .map(|(i, _)| i as u32)
                        .ok_or("trace39 no logits")?;
                    if best != expected {
                        return Err("trace39 actual generation differs from original greedy logits/EOS".into());
                    }
                }
            }
        }
        let root = frontier.finish(p as u32, prepared.node_count())?;
        sink.position_finished(p as u32, root, &output)?;
        positions.push(root);
        if !matches!(source.program().output, misaka_palw_tir::program_v2::OutputDecl::Final { .. }) || p + 1 == actual as usize {
            outputs.push(crate::flat_material39::tensor_commitment_streamed_v39(output.dtype, &output.shape, |offset, count| {
                let at = usize::try_from(offset).map_err(|_| "trace39 output offset")?;
                let end = at.checked_add(count).ok_or("trace39 output range overflow")?;
                output.data.get(at..end).map(|v| v.to_vec()).ok_or("trace39 exact output range".into())
            })?);
        }
    }
    let actual_resources = crate::flat_resources39::derive_flat_resources_v39(source, actual)?;
    let claim = FlatClaimV39 {
        version: 39,
        source: source.root(),
        maximum: maximum.root(),
        generated: crate::hash::object_id(b"misaka-palw/kernel/actual-generated/v39", &inputs.generated()),
        maximum_scope: maximum.resources().scope().root(),
        actual_scope: actual_resources.scope().root(),
        input_binding: inputs.root(),
        positions: actual,
        segment_roots: crate::seg::segment_roots_of_position_roots_v1(&positions),
    };
    Ok(TraceSummaryV39 { claim, position_roots: positions, output_roots: outputs })
}
