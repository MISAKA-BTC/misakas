//! Test-only native dynamic dependency fixture; no production profile values.
use super::fixture::{Fixture, fixture_with};
/// A real native history-dissection fixture whose reduction controls BOTH a
/// Select branch and a Gather row. Only the tail piece of each possible row is
/// needed. The unrelated committed parameter is deliberately outside this cone.
pub(super) fn dynamic_fixture(parameter_free: bool, per_layer: bool, fixed_replay: bool) -> (Fixture, u16) {
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType};
    let mut pb = ProgramBuilder::new(4, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
    let columns = if parameter_free { 1 } else { 8193 };
    let shape = [4, columns];
    let (left, right) = if parameter_free {
        let a = pb.konst(DType::I32, &shape, &vec![7; 4 * columns as usize]);
        (a, a)
    } else {
        (pb.param("left", DType::I32, &shape, per_layer), pb.param("right", DType::I32, &shape, per_layer))
    };
    let unrelated = pb.param("unrelated", DType::I8, &[1], per_layer);
    let replay = if fixed_replay { Some(pb.param("writer", DType::I8, &[1], per_layer)) } else { None };
    let history = pb.hist_state("history", DType::I8, &[1], 8, true);
    let state = if fixed_replay { Some(pb.fixed_state("replay", DType::I32, &[1], -128, 127, true)) } else { None };
    let carry = vec![TensorType::fixed(DType::I32, &[1])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let t = b.cast(Ref::Input(0), DType::I32);
        let t = b.reshape_fixed(t, &[1]);
        b.finish(&[t])
    };
    let (layer, target) = {
        let mut b = pb.block("history", carry.clone());
        let row = b.clamp(Ref::CarryIn(0), 0, 3, DType::I32);
        let row = b.cast(row, DType::I8);
        let h = b.hist_append(history, row);
        let total = b.reduce_sum(h, 0, DType::I32);
        let mut value = b.reshape_fixed(total, &[]);
        if let Some(state) = state {
            let s = b.reshape_fixed(Ref::State(state), &[]);
            value = b.add(value, s, DType::I32);
        }
        let selected = b.select(value, left, right, DType::I32);
        let clamped = b.clamp(value, 0, 3, DType::I32);
        let ix = b.cast(clamped, DType::Idx);
        let ix = b.reshape_fixed(ix, &[1]);
        let gathered = b.gather(selected, ix, 0, 0);
        let tail = b.slice(gathered, 1, columns - 1, 1);
        let out = b.reshape_fixed(tail, &[1]);
        b.commit(out);
        let Ref::Node(target) = out else { unreachable!() };
        // This other committed cone must not contaminate a potential cover.
        let other = b.cast(unrelated, DType::I32);
        b.commit(other);
        if let (Some(state), Some(replay)) = (state, replay) {
            b.state_write(state, replay);
        }
        (b.finish(&[out]), target)
    };
    let post = {
        let mut b = pb.block("post", carry);
        let out = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.commit(out);
        b.finish(&[])
    };
    let p = pb.finish(pre, if per_layer { vec![layer, layer] } else { vec![layer] }, post, 0);
    let mut params = MapParams::default();
    for (j, d) in p.params.iter().enumerate() {
        let layers = if d.per_layer { vec![Some(0), Some(1)] } else { vec![None] };
        for layer in layers {
            let n = d.shape.iter().map(|n| *n as usize).product();
            params.tensors.insert(
                (j as u16, layer),
                Tensor {
                    dtype: d.dtype,
                    shape: d.shape.iter().map(|n| *n as usize).collect(),
                    data: vec![if d.name == "writer" { 1 } else { 7 }; n],
                },
            );
        }
    }
    (fixture_with("dynamic-parameter-cover".into(), p, params, vec![0, 1, 2, 3], 4, 2), target)
}
