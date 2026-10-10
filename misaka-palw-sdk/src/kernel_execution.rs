//! A generic native producer of v3 node/position/segment roots. The artifact is canonical TIR,
//! with no ModelSpec or official compiler registry. Values are visited only after a successful
//! whole position; no prefix of a failed step is a published result. This is local preparation,
//! not chain registration, source-model fidelity, model authentication or kernel activation.
use misaka_palw_kernel::descriptor::KernelDescriptorV1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::merkle3_stream::{streamed_commitment_shape_v3, tensor_commitment_streamed_v3};
use misaka_palw_kernel::plan::VerificationPlanV1;
use misaka_palw_kernel::seg::{claim_root_v1, position_root_of_v1, segment_roots_of_position_roots_v1};
use misaka_palw_kernel::trace::WiringV1;
use misaka_palw_tir::Tensor;
use misaka_palw_tir_exec::node::TirArtifactV1;
use misaka_palw_tir_exec::{NodeValue, StepSink, TirExecutor};

#[derive(Clone, Copy, Debug)]
pub struct NativeTraceLimitsV3 {
    pub tensor_hash_workspace_bytes: u64,
    /// Current node commitments, position/segment root folding and optional decoded capture.
    /// Excludes executor buffers, mapped weights, caller-retained visits and process overhead.
    pub trace_workspace_bytes: u64,
}

pub struct NativePositionV3 {
    pub position: u32,
    pub commitments: Vec<Vec<Digest>>,
    pub position_root: Digest,
    /// Optional PRIVATE producer capture. It must not be served as public material without
    /// applying the descriptor's disclosure rules (withheld model-dependent values included).
    pub values: Option<Vec<Vec<Tensor>>>,
}

#[derive(Clone, Debug)]
pub struct NativeTraceV3 {
    pub program_root: Digest,
    pub plan_root: Digest,
    pub position_roots: Vec<Digest>,
    pub segment_roots: Vec<Digest>,
    pub claim_root: Digest,
    pub trace_workspace_bound: u64,
    pub max_tensor_hash_workspace: u64,
}

/// Off-chain reproduction data for a delivery court. Contains public roots and at most two logits
/// leaves, never the private captured position values or raw model weights. This is not a
/// consensus object or proof of registration/activation; consumers must bind it to chain facts.
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct DecodeCourtBundleV3 {
    pub version: u16,
    pub inventory_root: Digest,
    pub program_bytes: Vec<u8>,
    pub plan: VerificationPlanV1,
    pub params: misaka_palw_kernel::trace::ParamCommitmentsV1,
    pub position_roots: Vec<Digest>,
    pub tokens: Vec<u32>,
    pub honest_generated: Vec<u32>,
    pub wrong_generated: Vec<u32>,
    pub fault: misaka_palw_kernel::element::SegFaultV1,
}

/// Run the exact supplied fed-token list from initial state, within the exact supplied plan.
/// All shape/hash/capture bounds and tokens are checked before execution. `capture` determines
/// which positions the callback receives decoded; caller retention has its own memory budget.
/// A sink hashing failure discards the current position, after the native step returns.
pub fn native_trace_v3(
    artifact: &TirArtifactV1,
    descriptor: &KernelDescriptorV1,
    plan: &VerificationPlanV1,
    tokens: &[u32],
    limits: NativeTraceLimitsV3,
    capture: &dyn Fn(u32) -> bool,
    visit: &mut dyn FnMut(NativePositionV3) -> Result<(), String>,
) -> Result<NativeTraceV3, String> {
    if !misaka_palw_kernel::descriptor::is_segmented_v1(descriptor) || descriptor.memory_model_id != 1 {
        return Err("native decoder trace requires segmented commitments and decoder memory semantics".into());
    }
    descriptor.well_formed().map_err(|e| format!("descriptor: {e:?}"))?;
    let program = &artifact.container().program;
    let count = u32::try_from(tokens.len()).map_err(|_| "position count overflows")?;
    if count == 0 || count > plan.max_positions || plan.max_positions > program.history_bound {
        return Err("fed positions outside the exact plan/program bound".into());
    }
    if tokens.iter().any(|t| *t >= program.token_bound) {
        return Err("fed token outside the program vocabulary".into());
    }
    let program_root = misaka_palw_kernel::public::program_root_v1(&program.encode());
    let expected = misaka_palw_kernel::plan::plan_for_tir_program_v1(descriptor, program, program_root, plan.max_positions)
        .map_err(|(_, why)| why)?;
    if *plan != expected {
        return Err("not the exact derived plan of this artifact and descriptor".into());
    }
    // Reuse the node's prosecution/carrier/block ceilings. Dormant kernels may be prepared
    // locally, but no other static admission refusal becomes permission to execute this route.
    let outcome = crate::preflight::kernel::node_program_outcome(
        &misaka_palw_kernel::descriptor::builtin_schedule_v1(),
        descriptor,
        program,
        program_root,
        plan.max_positions,
        0,
    );
    if !matches!(
        outcome,
        misaka_palw_kernel::outcome::RegistrationOutcomeV1::EligibleAt { .. }
            | misaka_palw_kernel::outcome::RegistrationOutcomeV1::KernelNotActive { .. }
    ) {
        return Err(format!("static node admission refuses native trace: {outcome}"));
    }
    let wiring = WiringV1::new(program).map_err(|e| e.to_string())?;
    // Evaluate capture policy once per position, before executing any of them.
    let mut captures = Vec::new();
    let root_bytes = (count as u64).checked_mul(256).ok_or("root workspace overflow")?;
    if root_bytes > limits.trace_workspace_bytes {
        return Err("position-root workspace exceeds trace limit".into());
    }
    captures.try_reserve_exact(count as usize).map_err(|e| e.to_string())?;
    captures.extend((0..count).map(capture));
    let keep_any = captures.iter().any(|v| *v);
    let mut trace_bound = root_bytes.checked_add(1 << 16).ok_or("trace workspace overflow")?;
    let mut hash_bound = 0;
    for (s, (b, _)) in wiring.occurrences.iter().enumerate() {
        let h = wiring.h(s as u16, count - 1);
        for node in &program.blocks[*b as usize].nodes {
            let shape = node.out.resolve(h);
            let bound = streamed_commitment_shape_v3(node.out.dtype, &shape)?;
            hash_bound = hash_bound.max(bound.workspace_bytes);
            // Four root arrays cover live commitments, position leaf folding and summaries.
            let bytes = if keep_any {
                let len = bound.tensor_bytes / node.out.dtype.width() as u64;
                len.checked_mul(16).and_then(|n| n.checked_add(1024)).ok_or("capture workspace overflow")?
            } else {
                512
            };
            trace_bound = trace_bound.checked_add(bytes).ok_or("trace workspace overflow")?;
        }
    }
    if trace_bound > limits.trace_workspace_bytes || hash_bound > limits.tensor_hash_workspace_bytes {
        return Err(format!("native trace workspace {trace_bound}, tensor hash workspace {hash_bound} exceed limits"));
    }
    struct Sink<'a> {
        program: &'a misaka_palw_tir::TirProgramV1,
        occurrences: &'a [(u8, Option<u16>)],
        position: u32,
        commitments: Vec<Vec<Digest>>,
        values: Option<Vec<Vec<Tensor>>>,
        histories: Vec<usize>,
        next: (usize, usize),
        limit: u64,
        error: Option<String>,
    }
    impl StepSink for Sink<'_> {
        fn every_node(&self) -> bool {
            true
        }
        fn node(&mut self, v: &NodeValue<'_>) {
            if self.error.is_some() {
                return;
            }
            let (s, n) = self.next;
            let result = (|| -> Result<(), String> {
                let &(b, l) = self.occurrences.get(s).ok_or("extra native node")?;
                let node = self.program.blocks[b as usize].nodes.get(n).ok_or("extra native node")?;
                if (v.block, v.layer, v.node, v.dtype) != (b, l, n as u16, node.out.dtype) {
                    return Err(format!(
                        "native node {:?} differs from expected {:?}",
                        (v.block, v.layer, v.node, v.dtype),
                        (b, l, n, node.out.dtype)
                    ));
                }
                let shape = node.out.resolve(self.histories[s]);
                if v.shape != shape || shape.iter().product::<usize>() != v.data.len() {
                    return Err("native value type/length differs from the declared position type".into());
                }
                let width = v.dtype.width();
                let (root, _) = tensor_commitment_streamed_v3(v.dtype, v.shape, self.limit, &mut |offset, out| {
                    if offset % width as u64 != 0 || out.len() % width != 0 {
                        return Err("unaligned native byte range".into());
                    }
                    let first = offset as usize / width;
                    let end = first.checked_add(out.len() / width).ok_or("native range overflow")?;
                    if end > v.data.len() {
                        return Err("native value length differs from shape".into());
                    }
                    for (i, bytes) in out.chunks_exact_mut(width).enumerate() {
                        // The native planner may store an output in a wider buffer. Its
                        // mathematical value must fit the declared wire dtype before narrowing.
                        let value = v.data.get(first + i);
                        if !v.dtype.contains(value) {
                            return Err("native value exceeds declared dtype".into());
                        }
                        bytes.copy_from_slice(&value.to_le_bytes()[..width]);
                    }
                    Ok(())
                })?;
                self.commitments[s].push(root);
                if let Some(values) = &mut self.values {
                    values[s].push(v.to_tensor());
                }
                self.next = if n + 1 == self.program.blocks[b as usize].nodes.len() { (s + 1, 0) } else { (s, n + 1) };
                Ok(())
            })();
            if let Err(e) = result {
                self.error = Some(format!("position {}: {e}", self.position));
            }
        }
    }
    let mut position_roots = Vec::new();
    position_roots.try_reserve_exact(count as usize).map_err(|e| e.to_string())?;
    let mut exec = TirExecutor::new(artifact.plan(), artifact.params()).map_err(|e| e.to_string())?;
    for (p, token) in tokens.iter().copied().enumerate() {
        let rows = || wiring.occurrences.iter().map(|(b, _)| Vec::with_capacity(program.blocks[*b as usize].nodes.len())).collect();
        let mut sink = Sink {
            program,
            occurrences: &wiring.occurrences,
            position: p as u32,
            commitments: rows(),
            values: captures[p].then(|| {
                wiring.occurrences.iter().map(|(b, _)| Vec::with_capacity(program.blocks[*b as usize].nodes.len())).collect()
            }),
            histories: (0..wiring.occurrences.len()).map(|s| wiring.h(s as u16, p as u32)).collect(),
            next: (0, 0),
            limit: limits.tensor_hash_workspace_bytes,
            error: None,
        };
        exec.step(token, &mut sink).map_err(|e| format!("position {p}: {e}"))?;
        if let Some(e) = sink.error {
            return Err(e);
        }
        if sink.next != (wiring.occurrences.len(), 0) {
            return Err("native step omitted nodes".into());
        }
        let root = position_root_of_v1(p as u32, &sink.commitments);
        visit(NativePositionV3 { position: p as u32, commitments: sink.commitments, position_root: root, values: sink.values })?;
        position_roots.push(root);
    }
    let segment_roots = segment_roots_of_position_roots_v1(&position_roots);
    let claim_root = claim_root_v1(count, &segment_roots);
    Ok(NativeTraceV3 {
        program_root,
        plan_root: plan.root(),
        position_roots,
        segment_roots,
        claim_root,
        trace_workspace_bound: trace_bound,
        max_tensor_hash_workspace: hash_bound,
    })
}

/// Build a delivery-lie witness from the selecting logits and their authenticated node opening.
/// It opens only the delivered/rival leaves, needs no producer state or parameter tensor, and
/// self-grades through the existing terminal court. Correct delivery returns `None`; wrong or
/// copied node/material bindings refuse. Acquisition/disclosure rules remain the caller's.
pub fn decode_fault_from_logits_v3(
    context: &misaka_palw_kernel::element::SegClaimContextV1<'_>,
    opening: &misaka_palw_kernel::seg::NodeOpeningV1,
    logits: &Tensor,
    index: u32,
) -> Result<Option<misaka_palw_kernel::element::SegFaultV1>, String> {
    use misaka_palw_kernel::element::{OperandOpeningV1, SegDecodeFaultV1, SegFaultV1, leaves_covering_v1, verify_seg_fault_v1};
    let delivered = *context.generated.get(index as usize).ok_or("no delivered id")?;
    let position = context.prompt_len.checked_sub(1).and_then(|p| p.checked_add(index)).ok_or("no selecting position")?;
    let wiring = WiringV1::new(context.program).map_err(|e| e.to_string())?;
    let post = (wiring.occurrences.len() - 1) as u16;
    let nodes: usize = wiring.occurrences.iter().map(|(b, _)| context.program.blocks[*b as usize].nodes.len()).sum();
    let offset: usize = wiring.occurrences[..post as usize].iter().map(|(b, _)| context.program.blocks[*b as usize].nodes.len()).sum();
    let node = wiring.node(post, context.program.logits);
    if context.encoder.is_some()
        || (opening.position, opening.occurrence, opening.node) != (position, post, context.program.logits)
        || position >= context.positions
        || !opening.authenticates(
            context.segment_roots,
            context.positions,
            (offset + context.program.logits as usize) as u64,
            nodes as u64,
        )
        || logits.shape.iter().try_fold(1usize, |n, d| n.checked_mul(*d)) != Some(logits.data.len())
        || logits.data.iter().any(|v| !logits.dtype.contains(*v))
        || logits.dtype != node.out.dtype
        || logits.shape != node.out.resolve(wiring.h(post, position))
        || opening.commitment != misaka_palw_kernel::merkle3::tensor_commitment_v3(logits)
    {
        return Err("the selecting logits do not authenticate against the claim/program".into());
    }
    let rival = context.decode.select(logits).ok_or("logits cannot be decoded")?;
    if rival == delivered {
        return Ok(None);
    }
    let mut reads = std::collections::BTreeSet::from([rival as u64]);
    if (delivered as usize) < logits.len() {
        reads.insert(delivered as u64);
    }
    let fault = SegFaultV1::Decode(SegDecodeFaultV1 {
        index,
        rival,
        logits: OperandOpeningV1 {
            node: Some(opening.clone()),
            leaves: leaves_covering_v1(logits, &misaka_palw_kernel::merkle3::TreesV3::of(logits), &reads),
        },
    });
    verify_seg_fault_v1(context, &fault).map_err(|e| format!("decode witness refused: {e:?}"))?;
    Ok(Some(fault))
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::descriptor::k2_tir_v4_descriptor;
    use misaka_palw_kernel::seg::{SegmentedCommitmentsV1, position_node_opening_v1, position_path_of_roots_v1};
    fn artifact(name: &str, f: &misaka_palw_tir_sketch::fixture::TirSketchFixtureV1) -> (std::path::PathBuf, TirArtifactV1) {
        let path = std::env::temp_dir().join(format!("native-kernel-{name}-{}.palwtir", std::process::id()));
        misaka_palw_tir_artifact::write_container_v1(&path, &f.program, vec![], [0; 64], "independent TIR".into(), &mut |j, l| {
            Ok(f.params.tensors[&(j, l)].to_le_bytes())
        })
        .unwrap();
        let a = TirArtifactV1::open(&path).unwrap();
        (path, a)
    }
    fn limits() -> NativeTraceLimitsV3 {
        NativeTraceLimitsV3 { tensor_hash_workspace_bytes: 1 << 24, trace_workspace_bytes: 1 << 26 }
    }
    #[test]
    fn every_native_node_value_and_root_match_reference_through_sliding_history_and_moe() {
        let f = misaka_palw_tir_sketch::fixture::dense_moe_windowed_v1(7, 2);
        let (path, a) = artifact("roots", &f);
        let d = k2_tir_v4_descriptor();
        let plan = misaka_palw_kernel::plan::plan_for_tir_program_v1(
            &d,
            &f.program,
            misaka_palw_kernel::public::program_root_v1(&f.program.encode()),
            5,
        )
        .unwrap();
        let tokens = [1, 2, 3, 4, 5];
        let reference = misaka_palw_kernel::trace::trace_v1(&f.program, &f.params, &tokens).unwrap();
        let expected = misaka_palw_kernel::seg::seg_commitments_of_trace_v1(&reference);
        let result = native_trace_v3(&a, &d, &plan, &tokens, limits(), &|p| p == 4, &mut |p| {
            assert_eq!(p.commitments, expected.commitments[p.position as usize]);
            assert_eq!(p.position_root, expected.position_root(p.position));
            if p.position == 4 {
                assert_eq!(p.values.unwrap(), reference.values[4]);
            } else {
                assert!(p.values.is_none());
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(result.position_roots, expected.position_roots);
        assert_eq!(result.segment_roots, expected.segment_roots());
        assert_eq!(result.claim_root, expected.claim_root());
        drop(a);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn forged_plans_contexts_tokens_caps_and_callback_failures_do_not_complete_a_trace() {
        let f = misaka_palw_tir_sketch::fixture::wide128_v1(7);
        let (path, a) = artifact("limits", &f);
        let d = k2_tir_v4_descriptor();
        let plan = misaka_palw_kernel::plan::plan_for_tir_program_v1(
            &d,
            &f.program,
            misaka_palw_kernel::public::program_root_v1(&f.program.encode()),
            2,
        )
        .unwrap();
        for tokens in [vec![], vec![1, 2, 3], vec![f.program.token_bound]] {
            assert!(native_trace_v3(&a, &d, &plan, &tokens, limits(), &|_| false, &mut |_| panic!("visit after refusal")).is_err());
        }
        let mut forged = plan.clone();
        forged.program_root[0] ^= 1;
        assert!(native_trace_v3(&a, &d, &forged, &[1], limits(), &|_| false, &mut |_| panic!()).is_err());
        for cap in [
            NativeTraceLimitsV3 { tensor_hash_workspace_bytes: 0, ..limits() },
            NativeTraceLimitsV3 { trace_workspace_bytes: 0, ..limits() },
        ] {
            assert!(native_trace_v3(&a, &d, &plan, &[1], cap, &|_| true, &mut |_| panic!()).is_err());
        }
        let mut visits = 0;
        assert_eq!(
            native_trace_v3(&a, &d, &plan, &[1, 2], limits(), &|_| false, &mut |_| {
                visits += 1;
                Err("disk refused".into())
            })
            .unwrap_err(),
            "disk refused"
        );
        assert_eq!(visits, 1);
        drop(a);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn public_logits_convict_delivery_lies_and_refuse_copied_or_tampered_bindings() {
        let f = misaka_palw_tir_sketch::fixture::dense_moe_v1(7);
        let trace = misaka_palw_kernel::trace::trace_v1(&f.program, &f.params, &[1]).unwrap();
        let commits = SegmentedCommitmentsV1::new(misaka_palw_kernel::seg::seg_commitments_of_trace_v1(&trace).commitments);
        let roots = commits.segment_roots();
        let params = misaka_palw_kernel::trace::ParamCommitmentsV1::of_v3(&f.params);
        let post = trace.values[0].len() - 1;
        let logits = &trace.values[0][post][f.program.logits as usize];
        let best = misaka_palw_kernel::job::DecodeRuleV1::Greedy.select(logits).unwrap();
        let generated = [best];
        let mut c = misaka_palw_kernel::element::SegClaimContextV1 {
            program: &f.program,
            params: &params,
            segment_roots: &roots,
            positions: 1,
            prompt_len: 1,
            prompt_root: misaka_palw_kernel::seg::prompt_root_of_ids_v1(&[1]),
            inline_prompt: Some(&[1]),
            generated: &generated,
            decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
            encoder: None,
        };
        let opening = position_node_opening_v1(
            0,
            &commits.commitments[0],
            position_path_of_roots_v1(&commits.position_roots, 0).unwrap().1,
            post as u16,
            f.program.logits,
        )
        .unwrap();
        assert!(decode_fault_from_logits_v3(&c, &opening, logits, 0).unwrap().is_none());
        let wrong = [(best + 1) % logits.len() as u32];
        c.generated = &wrong;
        let proof = decode_fault_from_logits_v3(&c, &opening, logits, 0).unwrap().unwrap();
        assert!(misaka_palw_kernel::element::verify_seg_fault_v1(&c, &proof).is_ok());
        let mut weaker = proof.clone();
        if let misaka_palw_kernel::element::SegFaultV1::Decode(candidate) = &mut weaker {
            candidate.rival = wrong[0];
        }
        c.generated = &generated;
        assert_eq!(
            misaka_palw_kernel::element::verify_seg_fault_v1(&c, &weaker),
            Err(misaka_palw_kernel::verify::DismissalV1::NoFault)
        );
        c.generated = &wrong;
        let mut bad = opening.clone();
        bad.position = 1;
        assert!(decode_fault_from_logits_v3(&c, &bad, logits, 0).is_err());
        bad = opening.clone();
        bad.commitment[0] ^= 1;
        assert!(decode_fault_from_logits_v3(&c, &bad, logits, 0).is_err());
        let mut altered = logits.clone();
        altered.data[0] ^= 1;
        assert!(decode_fault_from_logits_v3(&c, &opening, &altered, 0).is_err());
        assert!(decode_fault_from_logits_v3(&c, &opening, logits, 1).is_err());
        let out_of_range = [logits.len() as u32];
        c.generated = &out_of_range;
        assert!(decode_fault_from_logits_v3(&c, &opening, logits, 0).unwrap().is_some());
    }
}
