use misaka_palw_kernel::{flat_inputs12::*, flat_lengths7::*, merkle3::tensor_commitment_v3, trace::ParamCommitmentsV1};
use misaka_palw_tir::{
    DType, MapParams, ParamSource, Ref, Tensor, TensorType,
    builder::ProgramBuilder,
    interp_v2::{InputProvider, InterpreterV2},
    program::HISTORY_BOUND_V1_SMALL,
    program_v2::{InputSource, OutputDecl, TirProgramV2},
};
use std::cell::Cell;

fn fixture() -> (FlatInputTemplateV12, MapParams, ParamCommitmentsV1) {
    let mut b = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let patches = b.param("patches", DType::I32, &[4], false);
    let audio = b.param("audio", DType::I32, &[6], false);
    let offset = b.param("offset", DType::I8, &[1], false);
    let state = b.fixed_state("previous", DType::I32, &[1], 0, 100, false);
    let mut pre = b.block("pre", vec![]);
    let p = pre.reduce_sum(patches, 0, DType::I64);
    let a = pre.reduce_sum(audio, 0, DType::I64);
    let y = pre.add(p, a, DType::I64);
    let y = pre.add(y, offset, DType::I64);
    let y = pre.add(y, Ref::State(state), DType::I64);
    let y = pre.clamp(y, 0, 100, DType::I32);
    let y = pre.state_write(state, y);
    let pre = pre.finish(&[y]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[1])]);
    let y = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    let old = b.finish(pre, vec![], post, 0);
    let program = TirProgramV2::from_v1_lifting_params(
        &old,
        &[(0, InputSource::External { lo: 0, hi: 20 }), (1, InputSource::External { lo: 0, hi: 20 })],
        OutputDecl::Rows { node: 0 },
    )
    .unwrap();
    let source = MapParams { tensors: [((0, None), Tensor::new(DType::I8, vec![1], vec![5]).unwrap())].into_iter().collect() };
    let pc =
        ParamCommitmentsV1 { by_instance: [((0, None), tensor_commitment_v3(&source.tensors[&(0, None)]))].into_iter().collect() };
    let lengths = FlatLengthTemplateV7 {
        version: 7,
        maxima: FlatJobLengthsV7 { token: 0, patch: 4, audio: 6 },
        max_program_bytes: program.v1_view().encode(),
        bindings: vec![
            FlatShapeBindingV7 { slot: FlatShapeSlotV7::Parameter { parameter: 1, axis: 0 }, length: FlatLengthAxisV7::Patch },
            FlatShapeBindingV7 { slot: FlatShapeSlotV7::Parameter { parameter: 2, axis: 0 }, length: FlatLengthAxisV7::Audio },
        ],
    };
    (FlatInputTemplateV12 { program, lengths, max_positions: 3, source_parameters: pc.root() }, source, pc)
}
struct Inputs {
    shape: [Vec<u32>; 2],
    raw: [Vec<u8>; 2],
    calls: Cell<u32>,
    dtype: DType,
}
impl Inputs {
    fn new() -> Self {
        Self {
            shape: [vec![3], vec![5]],
            raw: [
                vec![1i32, 2, 3].into_iter().flat_map(i32::to_le_bytes).collect(),
                vec![1i32; 5].into_iter().flat_map(i32::to_le_bytes).collect(),
            ],
            calls: Cell::new(0),
            dtype: DType::I32,
        }
    }
}
impl RawExternalInputsV12 for Inputs {
    fn input(&self, i: u16) -> Option<RawExternalInputV12<'_>> {
        self.calls.set(self.calls.get() + 1);
        Some(RawExternalInputV12 { dtype: self.dtype, shape: self.shape.get(i as usize)?, bytes: self.raw.get(i as usize)? })
    }
}
fn declaration(a: &FlatInputAdmissionV12) -> FlatInputCommitmentsV12 {
    FlatInputCommitmentsV12 {
        version: 12,
        admission: a.root(),
        job: [9; 64],
        input_roots: [Tensor::new(DType::I32, vec![3], vec![1, 2, 3]).unwrap(), Tensor::new(DType::I32, vec![5], vec![1; 5]).unwrap()]
            .iter()
            .map(tensor_commitment_v3)
            .collect(),
    }
}
fn full_budget() -> FlatInputBudgetV12 {
    FlatInputBudgetV12 { raw_bytes: 1000, reference_bytes: 1000, attempts: 2 }
}

#[test]
fn typed_patch_audio_counts_roots_and_stateful_v2_view_match_independent_arithmetic() {
    let (template, source, pc) = fixture();
    let actual = FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 };
    let admission = template.specialize(actual, 3).unwrap();
    assert_eq!((admission.admission.raw_bytes, admission.admission.reference_bytes), (32, 128));
    let input = Inputs::new();
    let declared = declaration(&admission.admission);
    let got = acquire_authenticated_inputs_v12(&template, &admission.admission, [9; 64], &declared, &pc, &input, &mut full_budget())
        .unwrap();
    assert_eq!(input.calls.get(), 2);
    assert!(got.input(0, 3).is_none());
    assert!(got.view_input(1, Some(0)).is_none());
    let out = InterpreterV2::new(&admission.program).unwrap().run_positions(&source, &got, 3).unwrap();
    assert_eq!(out.iter().map(|s| s.output.data[0]).collect::<Vec<_>>(), vec![16, 32, 48]);
    struct Combined<'a>(&'a MapParams, &'a AuthenticatedInputsV12);
    impl ParamSource for Combined<'_> {
        fn param(&self, j: u16, l: Option<u16>) -> Option<Tensor> {
            if j == 0 { self.0.param(j, l) } else { self.1.view_input(j, l) }
        }
    }
    let view = admission.program.v1_view();
    let full = misaka_palw_kernel::trace::trace_v1(&view, &Combined(&source, &got), &[0; 3]).unwrap();
    for p in 0..3 {
        assert_eq!(full.values[p].last().unwrap()[0].data, vec![16 * (p as i128 + 1)]);
    }
    assert_eq!(got.commitments(), &declared);
    assert_eq!(got.combined_pc().by_instance.len(), 3);
    assert_eq!(got.combined_pc().by_instance[&(1, None)], declared.input_roots[0]);
}

#[test]
fn typed_input_wrong_job_source_scope_or_budget_is_refused_before_provider() {
    let (template, _, pc) = fixture();
    let a = template.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 3).unwrap().admission;
    let input = Inputs::new();
    let expected = declaration(&a);
    for kind in 0..7 {
        let mut budget = full_budget();
        let mut bad_a = a.clone();
        let mut bad_e = expected.clone();
        let mut bad_pc = pc.clone();
        let mut job = [9; 64];
        match kind {
            0 => job = [8; 64],
            1 => bad_a.scope = [7; 64],
            2 => bad_e.input_roots.pop().map(|_| ()).unwrap(),
            3 => {
                bad_pc.by_instance.insert((0, None), [8; 64]);
            }
            4 => budget.raw_bytes = 31,
            5 => budget.reference_bytes = 127,
            _ => budget.attempts = 0,
        }
        let before = (budget.raw_bytes, budget.reference_bytes, budget.attempts);
        assert!(acquire_authenticated_inputs_v12(&template, &bad_a, job, &bad_e, &bad_pc, &input, &mut budget).is_err());
        assert_eq!(input.calls.get(), 0);
        assert_eq!(before, (budget.raw_bytes, budget.reference_bytes, budget.attempts));
    }
}

#[test]
fn typed_input_bad_image_interval_or_commitment_retains_full_attempt() {
    let (template, _, pc) = fixture();
    let a = template.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 3).unwrap().admission;
    for kind in 0..5 {
        let mut input = Inputs::new();
        let mut e = declaration(&a);
        let mut budget = full_budget();
        match kind {
            0 => input.dtype = DType::I16,
            1 => input.shape[0] = vec![4],
            2 => {
                input.raw[0].push(0);
            }
            3 => input.raw[0][..4].copy_from_slice(&21i32.to_le_bytes()),
            _ => e.input_roots[0] = [8; 64],
        }
        assert!(acquire_authenticated_inputs_v12(&template, &a, [9; 64], &e, &pc, &input, &mut budget).is_err());
        assert_eq!(input.calls.get(), 1);
        assert_eq!((budget.raw_bytes, budget.reference_bytes, budget.attempts), (968, 872, 1));
    }
}

#[test]
fn typed_input_maxima_bind_actual_axes_and_cannot_resize_weights_or_hide_media() {
    let (template, _, _) = fixture();
    assert!(template.specialize(FlatJobLengthsV7 { token: 0, patch: 5, audio: 5 }, 1).is_err());
    assert!(template.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 0 }, 1).is_err());
    assert!(template.specialize(FlatJobLengthsV7 { token: 1, patch: 3, audio: 5 }, 1).is_err());
    let (mut bad, _, _) = fixture();
    bad.lengths.bindings.remove(0);
    assert!(bad.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 1).is_err());
    let (mut bad, _, _) = fixture();
    bad.lengths.bindings[0].slot = FlatShapeSlotV7::Parameter { parameter: 0, axis: 0 };
    assert!(bad.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 1).is_err());
    let (mut bad, _, _) = fixture();
    bad.max_positions = 0;
    assert!(bad.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 1).is_err());
}

#[test]
fn typed_media_input_and_prior_state_openings_bind_complete_court() {
    use misaka_palw_kernel::{
        element::SegClaimContextV1,
        flat_court5::{FlatOperandV5, FlatSegContextV5, flat_profile_commitment_v5, flat_trace_profile_id_v5},
        flat_court9::{FlatContextV9, FlatFaultV9, verify_flat_fault_v9},
        flat_point9::{FlatPointFaultV9, PointContextV9, PointOperandKindV9, ordinary_point_court_id_v9, point_dependencies_v9},
        job::DecodeRuleV1,
        merkle::AXIS_ROW,
        merkle3::{LeafOpeningV3, TreesV3},
        seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1},
        trace::{SourceV1, WiringV1},
        verify::DismissalV1,
    };
    let (template, source, pc) = fixture();
    let specialized = template.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 3).unwrap();
    let declared = declaration(&specialized.admission);
    let inputs = acquire_authenticated_inputs_v12(
        &template,
        &specialized.admission,
        [9; 64],
        &declared,
        &pc,
        &Inputs::new(),
        &mut full_budget(),
    )
    .unwrap();
    struct Combined<'a>(&'a MapParams, &'a AuthenticatedInputsV12);
    impl ParamSource for Combined<'_> {
        fn param(&self, j: u16, l: Option<u16>) -> Option<Tensor> {
            if j == 0 { self.0.param(j, l) } else { self.1.view_input(j, l) }
        }
    }
    let params = Combined(&source, &inputs);
    let program = specialized.program.v1_view();
    let prompt = [0; 3];
    let honest = misaka_palw_kernel::trace::trace_v1(&program, &params, &prompt).unwrap();
    let wiring = WiringV1::new(&program).unwrap();
    // Patch/audio reductions, the explicit previous-state read and its write
    // exercise the input PC and state boundary within the same scope.
    for node in [0u16, 1, 4, (program.blocks[0].nodes.len() - 1) as u16] {
        for lie in [false, true] {
            let mut trace = honest.clone();
            if lie {
                trace.values[2][0][node as usize].data[0] += 1;
            }
            let commitments = SegmentedCommitmentsV1::new(
                trace
                    .values
                    .iter()
                    .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                    .collect(),
            );
            let roots = commitments.segment_roots();
            let base = || FlatSegContextV5 {
                profile: flat_trace_profile_id_v5(),
                claim: SegClaimContextV1 {
                    program: &program,
                    params: inputs.combined_pc(),
                    segment_roots: &roots,
                    positions: 3,
                    prompt_len: 3,
                    prompt_root: prompt_root_of_ids_v1(&prompt),
                    inline_prompt: Some(&prompt),
                    generated: &[0],
                    decode: DecodeRuleV1::Greedy,
                    encoder: None,
                },
            };
            let point = PointContextV9 { court_id: ordinary_point_court_id_v9(), base: base() };
            let context = FlatContextV9 { scope: &specialized.scope, base: base() };
            let relation = wiring.node(0, node);
            let sources = (0..relation.inputs.len()).map(|i| wiring.input_source(&prompt, 2, 0, node, i).unwrap()).collect::<Vec<_>>();
            let own = sources
                .iter()
                .map(|s| match s {
                    SourceV1::Param { index, layer } => params.param(*index, *layer).unwrap(),
                    SourceV1::Node { position, occurrence, node } => {
                        honest.values[*position as usize][*occurrence as usize][*node as usize].clone()
                    }
                    _ => panic!("unexpected media/state source"),
                })
                .collect::<Vec<_>>();
            let dependencies = point_dependencies_v9(&point, 2, 0, node, 0, &own).unwrap();
            let operands = own
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    assert_eq!(dependencies.kinds[i], PointOperandKindV9::Row);
                    FlatOperandV5 {
                        node: match sources[i] {
                            SourceV1::Node { position, occurrence, node } => commitments.node_opening(position, occurrence, node),
                            _ => None,
                        },
                        flat: vec![],
                        legacy: dependencies.leaves[i]
                            .iter()
                            .map(|(row, tile)| LeafOpeningV3::of_trees(t, &TreesV3::of(t), AXIS_ROW, *row, *tile).unwrap())
                            .collect(),
                    }
                })
                .collect();
            let output = &trace.values[2][0][node as usize];
            let proof = FlatFaultV9::Ordinary(FlatPointFaultV9 {
                version: 9,
                element: 0,
                output: FlatOperandV5 {
                    node: commitments.node_opening(2, 0, node),
                    flat: vec![],
                    legacy: vec![LeafOpeningV3::of_trees(output, &TreesV3::of(output), AXIS_ROW, 0, 0).unwrap()],
                },
                inputs: operands,
                token: None,
            });
            let proof = FlatFaultV9::from_bytes(&proof.to_bytes().unwrap()).unwrap();
            assert_eq!(verify_flat_fault_v9(&context, &proof).map(|_| ()), if lie { Ok(()) } else { Err(DismissalV1::NoFault) });
            // Another tensor of the same declared shape cannot substitute the
            // exact job input or its authenticated predecessor state value.
            let mut borrowed = proof;
            let FlatFaultV9::Ordinary(ref mut f) = borrowed else { panic!() };
            f.inputs[0].legacy[0].values[0] ^= 1;
            assert!(matches!(verify_flat_fault_v9(&context, &borrowed), Err(DismissalV1::NotAuthentic(_))));
        }
    }
}
