//! Request composer39: the composition is accepted by the same native rule
//! (`check_component_invocation_v39`), and nothing absent is invented.
//! Unbuilt source for the consolidated validation phase.
use misaka_palw_kernel::{
    flat_invocation39::*, flat_material39::*, flat_pipeline39::PipelineV39, flat_source39::*, trace::ParamCommitmentsV1,
};
use misaka_palw_tir::{
    ConstDecl, DType, Prim, Ref, Schedule,
    pipeline::{StageDecl, TripRule},
    program_v2::OutputDecl,
    program_v39::*,
};

fn program() -> TirProgramV39 {
    let out = TensorTypeV39::scalar(DType::I16);
    TirProgramV39 {
        version: 39,
        profile: SourceProfileV39::full_source(),
        prim_set_id: PRIM_SET_ID_V39,
        token_bound: 16,
        history_bound: 32,
        inputs: vec![],
        params: vec![],
        consts: vec![ConstDecl { dtype: DType::I16, shape: vec![], data: 5i16.to_le_bytes().to_vec() }],
        states: vec![],
        blocks: vec![
            BlockV39 {
                name: "pre".into(),
                carry_in: vec![],
                nodes: vec![NodeV39 { prim: NodePrimV39::Frozen(Prim::Cast), inputs: vec![Ref::Const(0)], out: out.clone(), commit: true }],
                carry_out: vec![0],
            },
            BlockV39 {
                name: "post".into(),
                carry_in: vec![out.clone()],
                nodes: vec![NodeV39 { prim: NodePrimV39::Frozen(Prim::Cast), inputs: vec![Ref::CarryIn(0)], out, commit: true }],
                carry_out: vec![],
            },
        ],
        schedule: Schedule { pre: 0, layers: vec![], post: 1 },
        output: OutputDecl::Final { node: 0 },
    }
}
fn source(p: &TirProgramV39) -> CheckedSourceV39 {
    FlatSourceWireV39 {
        version: 39,
        program_bytes: p.encode(),
        inventory_root: derive_inventory_root_v39(p, [9; 64], &[]).unwrap(),
        real_parameters: ParamCommitmentsV1::default(),
        original_context_max: 32,
        original_task_metadata: [9; 64],
        original_inventory: vec![],
    }
    .check()
    .unwrap()
}
fn request() -> MasterPublicRequestV39 {
    let mut nonce = [0u8; 64];
    nonce[..8].copy_from_slice(&7u64.to_le_bytes());
    MasterPublicRequestV39 {
        version: 39,
        complete_source: [1; 64],
        requester: [2; 64],
        requester_operator: [3; 64],
        first_nonce: nonce,
        producers: vec![MasterProducerV39 { producer: [4; 64], operator: [5; 64], reward: 10 }],
        prompt: vec![],
        negative: vec![],
        source: vec![],
        key: vec![],
        steps: 0,
        scalars: vec![],
        images: vec![],
        seed: [6; 32],
        requested_new_tokens: 0,
        eos: vec![],
        finalized: vec![],
    }
}

#[test]
fn a_fixed_stage_composes_into_exactly_what_the_native_rule_accepts() {
    let p = program();
    let s = source(&p);
    let pipeline = PipelineV39 {
        version: 39,
        stages: vec![StageDecl { name: "only".into(), program: 0, trip: TripRule::Fixed { n: 1 }, max_trip: 1, tokens: None, bind: vec![] }],
        output_stage: 0,
    }
    .check(&[p])
    .unwrap();
    let r = request();
    let composed = compose_component_request_v39([8; 64], &r, &pipeline, &s, 0, &[]).unwrap();
    assert_eq!(composed.maximum_positions, 1);
    assert_eq!(composed.nonce, r.stage_nonce(0).unwrap(), "the per-stage nonce is the request's, never chosen");
    assert!(composed.inputs.is_empty());
    let (checked, invocation) = compose_and_check_component_v39([8; 64], &r, &pipeline, &s, 0, &[], &[]).unwrap();
    assert_eq!(invocation.maximum(), checked.root());
    assert!(compose_component_request_v39([8; 64], &r, &pipeline, &s, 1, &[]).is_err(), "no stage beyond the Master");
    assert!(compose_component_request_v39([0; 64], &r, &pipeline, &s, 0, &[]).is_err(), "a Master identity is required");
    let stray = MaterialDescriptorV39 {
        version: 39,
        source: s.root(),
        master: [8; 64],
        requester: r.requester,
        requester_operator: r.requester_operator,
        nonce: composed.nonce,
        input: 0,
        dtype: DType::I16,
        shape: vec![],
        tensor: [1; 64],
        page_root: [1; 64],
        origin: MaterialOriginV39::External { original_task: [9; 64], raw_source: [1; 64] },
    };
    assert!(composed.finish(&[stray]).is_err(), "a descriptor the composition does not need is refused");
}
