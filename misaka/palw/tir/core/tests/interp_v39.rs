//! Unexecuted source regressions. These fixtures are not checkpoint or Final evidence.
use misaka_palw_tir::{
    ConeEnv, DType, Prim, Ref, RunState, Schedule, StateDecl, StateKind, Tensor, TirError, TirErrorKind, TirResult,
    interp_v2::MapInputs,
    interp_v39::{InterpreterV39, ParamDescriptorV39, ParamSourceV39},
    program_v2::{InputDecl, InputSource, OutputDecl},
    program_v39::*,
};
use std::{cell::RefCell, collections::BTreeMap};
struct Empty;
impl ParamSourceV39 for Empty {
    fn descriptor(&self, _: u16, _: Option<u16>) -> Option<ParamDescriptorV39> {
        None
    }
    fn read_range(&self, _: u16, _: Option<u16>, _: u64, _: usize) -> TirResult<Vec<i128>> {
        panic!("no parameter is declared")
    }
}
fn scalar(v: i128) -> Tensor {
    Tensor::scalar(DType::I32, v).unwrap()
}
fn node(prim: Prim, inputs: Vec<Ref>, dtype: DType, shape: &[u32], commit: bool) -> NodeV39 {
    NodeV39 { prim: NodePrimV39::Frozen(prim), inputs, out: TensorTypeV39::fixed(dtype, shape), commit }
}
fn selected_history() -> TirProgramV39 {
    let scalar = TensorTypeV39::scalar(DType::I32);
    TirProgramV39 {
        version: 39,
        profile: SourceProfileV39::full_source(),
        prim_set_id: PRIM_SET_ID_V39,
        token_bound: 1,
        history_bound: 4,
        inputs: vec![InputDecl {
            name: "score".into(),
            dtype: DType::I32,
            shape: vec![],
            source: InputSource::External { lo: -100, hi: 100 },
        }],
        params: vec![],
        consts: vec![],
        states: vec![StateDecl {
            name: "scores".into(),
            kind: StateKind::Hist { window: 4 },
            dtype: DType::I32,
            shape: vec![],
            per_layer: true,
        }],
        blocks: vec![
            BlockV39 {
                name: "pre".into(),
                carry_in: vec![],
                nodes: vec![node(Prim::Cast, vec![Ref::Input(2)], DType::I32, &[], true)],
                carry_out: vec![0],
            },
            BlockV39 {
                name: "history".into(),
                carry_in: vec![scalar.clone()],
                nodes: vec![
                    NodeV39 {
                        prim: NodePrimV39::Frozen(Prim::HistAppend { state: 0 }),
                        inputs: vec![Ref::CarryIn(0)],
                        out: TensorTypeV39 { dtype: DType::I32, shape: vec![DimV39::H] },
                        commit: true,
                    },
                    NodeV39 {
                        prim: NodePrimV39::HistorySelect { axis: 0, cap: 2 },
                        inputs: vec![Ref::Node(0)],
                        out: TensorTypeV39 { dtype: DType::Idx, shape: vec![DimV39::SelectedH(2)] },
                        commit: true,
                    },
                    NodeV39 {
                        prim: NodePrimV39::Frozen(Prim::Gather { axis: 0, batch_dims: 0 }),
                        inputs: vec![Ref::Node(0), Ref::Node(1)],
                        out: TensorTypeV39 { dtype: DType::I32, shape: vec![DimV39::SelectedH(2)] },
                        commit: true,
                    },
                    node(Prim::ReduceSum { axis: 0 }, vec![Ref::Node(2)], DType::I64, &[1], false),
                    node(Prim::Reshape, vec![Ref::Node(3)], DType::I64, &[], false),
                    node(Prim::Cast, vec![Ref::Node(4)], DType::I32, &[], true),
                ],
                carry_out: vec![5],
            },
            BlockV39 {
                name: "post".into(),
                carry_in: vec![scalar],
                nodes: vec![node(Prim::Cast, vec![Ref::CarryIn(0)], DType::I32, &[], true)],
                carry_out: vec![],
            },
        ],
        schedule: Schedule { pre: 0, layers: vec![1], post: 2 },
        output: OutputDecl::Rows { node: 0 },
    }
}
#[test]
fn history_selection_executes_short_history_cap_ties_and_cone_replay() {
    let p = selected_history();
    let interpreter = InterpreterV39::new(&p).unwrap();
    let mut inputs = MapInputs::default();
    for (pos, value) in [5, 7, 7, -1].into_iter().enumerate() {
        inputs.at.insert((0, pos as u32), scalar(value));
    }
    let mut state = RunState::default();
    let mut selections = Vec::new();
    let mut outputs = Vec::new();
    for _ in 0..4 {
        let step = interpreter.step(&Empty, &inputs, &mut state, 0).unwrap();
        outputs.push(step.output.data[0]);
        selections.push(step.commits.iter().find(|c| c.block == 1 && c.node == 1).unwrap().value.data.clone());
    }
    assert_eq!(outputs, vec![5, 12, 14, 14]);
    assert_eq!(
        selections,
        vec![vec![0], vec![0, 1], vec![1, 2], vec![1, 2]],
        "equal scores prefer lower indices; result indices are ascending"
    );
    let mut env = ConeEnv {
        pos: 2,
        carry_in: BTreeMap::from([(0, scalar(7))]),
        hist_prior: BTreeMap::from([(0, vec![scalar(5), scalar(7)])]),
        ..Default::default()
    };
    assert_eq!(interpreter.eval_cone(1, Some(0), 1, &Empty, &inputs, &env).unwrap().data, vec![1, 2]);
    env.supplied.insert(1, Tensor::new(DType::Idx, vec![2], vec![0, 2]).unwrap());
    assert_eq!(interpreter.eval_cone(1, Some(0), 1, &Empty, &inputs, &env).unwrap_err().kind, TirErrorKind::Malformed);
}
#[test]
fn failed_input_or_observer_never_changes_history_or_position() {
    let p = selected_history();
    let interpreter = InterpreterV39::new(&p).unwrap();
    let mut inputs = MapInputs::default();
    inputs.constant.insert(0, scalar(5));
    let mut state = RunState::default();
    interpreter.step(&Empty, &inputs, &mut state, 0).unwrap();
    let before = state.clone();
    inputs.at.insert((0, 1), scalar(101));
    assert!(interpreter.step(&Empty, &inputs, &mut state, 0).is_err());
    assert_eq!(state, before);
    inputs.at.remove(&(0, 1));
    assert!(
        interpreter
            .step_with_observer(&Empty, &inputs, &mut state, 0, &mut |c| {
                if c.block == 2 { Err(TirError::new(TirErrorKind::Missing, "observer storage failed")) } else { Ok(()) }
            })
            .is_err()
    );
    assert_eq!(state, before, "earlier history effects are staged until the whole position succeeds");
}
struct HugeEmbedding {
    calls: RefCell<Vec<(u64, usize)>>,
    wrong_descriptor: bool,
    bad_value: bool,
}
impl ParamSourceV39 for HugeEmbedding {
    fn descriptor(&self, _: u16, _: Option<u16>) -> Option<ParamDescriptorV39> {
        Some(ParamDescriptorV39 { dtype: DType::I8, shape: vec![if self.wrong_descriptor { 248319 } else { 248320 }, 2048] })
    }
    fn read_range(&self, index: u16, layer: Option<u16>, offset: u64, n: usize) -> TirResult<Vec<i128>> {
        assert_eq!((index, layer), (0, None));
        assert!(n <= 2048, "must not materialize the full embedding");
        self.calls.borrow_mut().push((offset, n));
        Ok(vec![if self.bad_value { 128 } else { 7 }; n])
    }
}
fn embedding_program() -> TirProgramV39 {
    let mut p = selected_history();
    p.inputs.clear();
    p.states.clear();
    p.token_bound = 248320;
    p.params =
        vec![misaka_palw_tir::ParamDecl { name: "embedding".into(), dtype: DType::I8, shape: vec![248320, 2048], per_layer: false }];
    p.blocks = vec![
        BlockV39 {
            name: "pre".into(),
            carry_in: vec![],
            nodes: vec![node(Prim::Gather { axis: 0, batch_dims: 0 }, vec![Ref::Param(0), Ref::Input(0)], DType::I8, &[2048], true)],
            carry_out: vec![0],
        },
        BlockV39 {
            name: "post".into(),
            carry_in: vec![TensorTypeV39::fixed(DType::I8, &[2048])],
            nodes: vec![node(Prim::Cast, vec![Ref::CarryIn(0)], DType::I8, &[2048], true)],
            carry_out: vec![],
        },
    ];
    p.schedule = Schedule { pre: 0, layers: vec![], post: 1 };
    p
}
#[test]
fn large_parameter_reads_only_the_selected_authenticated_ranges() {
    let p = embedding_program();
    let interpreter = InterpreterV39::new(&p).unwrap();
    let source = HugeEmbedding { calls: RefCell::new(vec![]), wrong_descriptor: false, bad_value: false };
    let out = interpreter.step(&source, &MapInputs::default(), &mut RunState::default(), 12345).unwrap();
    assert_eq!(out.output.data, vec![7; 2048]);
    assert_eq!(*source.calls.borrow(), vec![(12345 * 2048, 2048)]);
    let bad = HugeEmbedding { calls: RefCell::new(vec![]), wrong_descriptor: true, bad_value: false };
    assert!(interpreter.step(&bad, &MapInputs::default(), &mut RunState::default(), 0).is_err());
    assert!(bad.calls.borrow().is_empty());
    let bad = HugeEmbedding { calls: RefCell::new(vec![]), wrong_descriptor: false, bad_value: true };
    assert!(interpreter.step(&bad, &MapInputs::default(), &mut RunState::default(), 0).is_err());
}
