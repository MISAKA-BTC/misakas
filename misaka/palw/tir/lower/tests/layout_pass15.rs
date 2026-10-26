//! Exact value and overflow equivalence, never source-fidelity or Final credit.
mod common;
use misaka_palw_tir::{
    DType, Interpreter, Prim, Ref, RunState, Tensor, builder::ProgramBuilder, eval::eval_primitive, program::HISTORY_BOUND_V1_SMALL,
};
use misaka_palw_tir_lower::{
    layout_pass15::*,
    lower::{IntParams, IntTensor},
};

fn transformed(a: &Tensor, b: &Tensor, dtype: DType, out: &[usize]) -> Result<Tensor, misaka_palw_tir::TirError> {
    let mut perm = (0..b.shape.len() as u8).collect::<Vec<_>>();
    let r = perm.len();
    perm.swap(r - 2, r - 1);
    let btshape = perm.iter().map(|i| b.shape[*i as usize]).collect::<Vec<_>>();
    let bt = eval_primitive(&Prim::Transpose { perm }, std::slice::from_ref(b), b.dtype, &btshape)?;
    let mut brshape = btshape;
    brshape.insert(r - 2, 1);
    let br = eval_primitive(&Prim::Reshape, &[bt], b.dtype, &brshape)?;
    let mut arshape = a.shape.clone();
    arshape.push(1);
    let ar = eval_primitive(&Prim::Reshape, std::slice::from_ref(a), a.dtype, &arshape)?;
    let mut mmshape = out.to_vec();
    mmshape.push(1);
    let mm = eval_primitive(&Prim::MatMul, &[br, ar], dtype, &mmshape)?;
    eval_primitive(&Prim::Reshape, &[mm], dtype, out)
}

#[test]
fn exact_products_with_grouped_and_broadcast_batches_keep_all_values() {
    for k in [1, 2, 31, 65, 2048] {
        for (a_shape, b_shape, out) in [
            (vec![7, k], vec![k, 4], vec![7, 4]),
            (vec![2, 7, k], vec![2, k, 4], vec![2, 7, 4]),
            (vec![1, 7, k], vec![2, k, 4], vec![2, 7, 4]),
            (vec![2, 7, k], vec![1, k, 4], vec![2, 7, 4]),
            (vec![7, k], vec![2, k, 4], vec![2, 7, 4]),
            (vec![2, 7, k], vec![k, 4], vec![2, 7, 4]),
        ] {
            let data = |s: &Vec<usize>, seed: i128| (0..s.iter().product()).map(|i| ((i as i128 * seed) % 61) - 30).collect();
            let a = Tensor::new(DType::I16, a_shape.clone(), data(&a_shape, 17)).unwrap();
            let b = Tensor::new(DType::I32, b_shape.clone(), data(&b_shape, 29)).unwrap();
            let expected = eval_primitive(&Prim::MatMul, &[a.clone(), b.clone()], DType::I64, &out).unwrap();
            assert_eq!(transformed(&a, &b, DType::I64, &out).unwrap(), expected);
        }
    }
}
#[test]
fn positive_and_negative_partial_sum_overflow_is_not_hidden_by_layout() {
    for sign in [-1, 1] {
        let a = Tensor::new(DType::I64, vec![2, 3], vec![sign * (i64::MAX as i128); 6]).unwrap();
        let b = Tensor::new(DType::I16, vec![3, 2], vec![2; 6]).unwrap();
        let old = eval_primitive(&Prim::MatMul, &[a.clone(), b.clone()], DType::I64, &[2, 2]).unwrap_err();
        let new = transformed(&a, &b, DType::I64, &[2, 2]).unwrap_err();
        assert_eq!(format!("{:?}", old.kind), format!("{:?}", new.kind));
    }
}
fn history_program() -> misaka_palw_tir::TirProgramV1 {
    let mut b = ProgramBuilder::new(6, HISTORY_BOUND_V1_SMALL);
    let a = b.param("q", DType::I16, &[2], false);
    let v = b.param("v", DType::I16, &[3], false);
    let hs = b.hist_state("qh", DType::I16, &[2], 512, false);
    let vs = b.hist_state("vh", DType::I16, &[3], 512, false);
    let mut pre = b.block("pre", vec![]);
    let a = pre.cast(a, DType::I16);
    let v = pre.cast(v, DType::I16);
    let ah = pre.hist_append(hs, a);
    let ah = pre.transpose(ah, &[1, 0]);
    let bh = pre.hist_append(vs, v);
    let mm = pre.matmul(ah, bh, DType::I64);
    let carried = pre.cast(mm, DType::I32);
    let pre = pre.finish(&[carried]);
    let mut post = b.block("post", vec![misaka_palw_tir::TensorType::fixed(DType::I32, &[2, 3])]);
    let logits = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    b.finish(pre, vec![], post, logits)
}
#[test]
fn canonical_program_refs_states_and_logits_are_remapped_without_changing_answers() {
    let old = history_program();
    let result = transpose_history_contractions_v15(&old).unwrap();
    let new = &result.program;
    assert_eq!(result.rewrites.len(), 1);
    assert_eq!(new.blocks[old.schedule.pre as usize].nodes.len(), old.blocks[old.schedule.pre as usize].nodes.len() + 4);
    assert_ne!(old.encode(), new.encode());
    assert_eq!(old.params, new.params);
    assert_eq!(old.states, new.states);
    assert_eq!(old.schedule, new.schedule);
    let mut params = IntParams::default();
    params.tensors.insert((0, None), IntTensor::i16(vec![2], vec![1, -2]));
    params.tensors.insert((1, None), IntTensor::i16(vec![3], vec![2, 3, -1]));
    let (old, new) = (Interpreter::new(&old).unwrap(), Interpreter::new(new).unwrap());
    let (mut a, mut b) = (RunState::default(), RunState::default());
    for p in 0..65 {
        assert_eq!(old.step(&params, &mut a, p % 6).unwrap().logits, new.step(&params, &mut b, p % 6).unwrap().logits);
    }
    common::three_ways(&result.program, &params, &[vec![1; 65]]).unwrap();
}
