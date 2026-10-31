#![allow(warnings)]
use misaka_palw_kernel::trace::{StageBindingV1, trace_stage_v1, trace_streaming_v1, trace_v1};
use misaka_palw_tir::{DType, Dim, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, program::HISTORY_BOUND_V1_SMALL};

fn selection() -> misaka_palw_tir::TirProgramV1 {
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_SMALL);
    let scores = b.param("scores", DType::I32, &[3], false);
    let mut pre = b.block("pre", vec![]);
    let selected = pre.topk(scores, 0, 1);
    let pre = pre.finish(&[selected]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::Idx, &[1])]);
    let output = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(output);
    let post = post.finish(&[]);
    b.finish(pre, vec![], post, 0)
}
fn wrong() -> Vec<Tensor> {
    vec![
        Tensor::new(DType::I32, vec![8], vec![1; 8]).unwrap(),
        Tensor::new(DType::I16, vec![3], vec![1; 3]).unwrap(),
        Tensor { dtype: DType::I32, shape: vec![3], data: vec![1, 2] },
        Tensor { dtype: DType::I32, shape: vec![3], data: vec![1, 2, i128::MAX] },
        Tensor::new(DType::I32, vec![1, 3], vec![1; 3]).unwrap(),
    ]
}
#[test]
fn buffered_and_streamed_trace_refuse_wrong_parameter_metadata_and_payload() {
    let p = selection();
    for tensor in wrong() {
        let parameters = MapParams { tensors: [((0, None), tensor)].into() };
        assert!(trace_v1(&p, &parameters, &[0]).is_err());
        let mut visits = 0;
        assert!(
            trace_streaming_v1(&p, &parameters, &[0], &mut |_, _| {
                visits += 1;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(visits, 0, "bad parameter is refused before publishing a position");
    }
    let parameters = MapParams { tensors: [((0, None), Tensor::new(DType::I32, vec![3], vec![3, 9, 9]).unwrap())].into() };
    let t = trace_v1(&p, &parameters, &[0, 1]).unwrap();
    assert_eq!(t.values[0][0][0].data, vec![1], "the canonical tie rule is preserved");
    trace_streaming_v1(&p, &parameters, &[0, 1], &mut |at, values| {
        assert_eq!(values, &t.values[at as usize]);
        Ok(())
    })
    .unwrap();
}
#[test]
fn stage_inputs_use_the_same_declared_parameter_type() {
    let p = selection();
    let stage = StageBindingV1 { first_input: 0, post_writers: vec![], inputs: 1 };
    for tensor in wrong() {
        assert!(trace_stage_v1(&p, Some(&stage), &MapParams::default(), &[0], &|_, _| Some(tensor.clone())).is_err());
    }
    let tensor = Tensor::new(DType::I32, vec![3], vec![3, 9, 9]).unwrap();
    let result = trace_stage_v1(&p, Some(&stage), &MapParams::default(), &[0], &|_, _| Some(tensor.clone())).unwrap();
    assert_eq!(result.values[0][0][0].data, vec![1]);
    let no_declared_second = StageBindingV1 { inputs: 2, ..stage };
    assert!(trace_stage_v1(&p, Some(&no_declared_second), &MapParams::default(), &[0], &|_, _| Some(tensor.clone())).is_err());
}
#[test]
fn each_layer_parameter_has_its_own_typed_instance() {
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_SMALL);
    let scores = b.param("scores", DType::I32, &[3], true);
    let mut pre = b.block("pre", vec![]);
    let token = pre.broadcast(Ref::Input(0), &[Dim::Fixed(1)]);
    let pre = pre.finish(&[token]);
    let mut layer = b.block("layer", vec![TensorType::fixed(DType::Idx, &[1])]);
    let selected = layer.topk(scores, 0, 1);
    let layer = layer.finish(&[selected]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::Idx, &[1])]);
    let output = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(output);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![layer; 2], post, 0);
    let right = Tensor::new(DType::I32, vec![3], vec![3, 9, 9]).unwrap();
    let wrong_shape = Tensor::new(DType::I32, vec![8], vec![0; 8]).unwrap();
    for tensors in [
        [((0, Some(0)), right.clone()), ((0, Some(1)), wrong_shape)].into(),
        [((0, Some(0)), right.clone()), ((0, None), right.clone())].into(),
    ] {
        assert!(trace_v1(&p, &MapParams { tensors }, &[0]).is_err());
    }
    let parameters = MapParams { tensors: [((0, Some(0)), right.clone()), ((0, Some(1)), right)].into() };
    assert!(trace_v1(&p, &parameters, &[0]).is_ok());
}
