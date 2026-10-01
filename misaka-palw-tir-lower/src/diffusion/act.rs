//! **Activations on codes**, two forms.
//!
//! * **[`lower_act_codes`] — composed (what the image classes use).** SiLU (the modulation's, the embedders' and the
//!   VAE's) and GELU-tanh (the feed-forward's) written out of the library's Q24 forms (`BlockBuilder::silu`,
//!   `gelu_tanh_q24`: `IntExp`, `IntRecip` and exact arithmetic) between two narrowings — codes to Q24 and back. No
//!   param: a cone that reads an activation reads only its input leaf. The sites that are lossy: the two narrowings
//!   and the library form's own rounding.
//! * **[`QAct`] — `ACT_TABLE_V1`, a 65,536-entry table of `i16` codes** (PALW-EX-5: a transcendental at registration
//!   is data), one `Gather`. Exact on the grid, two nodes — and a cone that reads it opens one tile of the table PER
//!   DISTINCT LOOKUP: measured on the fixture, a 64-lane activation leaf's cone close was 751 KB (about 11.7 KB a
//!   lookup), far over the one-carrier bound, so no committed tensor of a class may depend on a table lookup per lane.
//!   Kept for the tests and for classes whose activation is read by no committed cone.

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::{DType, Ref};

use super::sink::ParamSink;
use super::stream::lower_requant;
use super::tables::{act_table_i16, gelu_tanh, silu};
use crate::quant::mul_shift;

/// The activations the image profile needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Silu,
    GeluTanh,
}

/// **An activation on `i16` codes**: `x` at `in_scale` to codes at `out_scale`, through the library's Q24 form.
pub fn lower_act_codes(b: &mut BlockBuilder<'_>, x: Ref, act: Act, in_scale: f64, out_scale: f64) -> Ref {
    let q = lower_requant(b, x, mul_shift(in_scale * (1u64 << 24) as f64), i32::MIN as i64, i32::MAX as i64, DType::I32);
    let y = match act {
        Act::Silu => b.silu(q),
        Act::GeluTanh => b.gelu_tanh_q24(q),
    };
    lower_requant(b, y, mul_shift(1.0 / ((1u64 << 24) as f64 * out_scale)), -32_767, 32_767, DType::I16)
}

/// An activation table with the scales it was built at.
#[derive(Clone, Debug)]
pub struct QAct {
    pub table: Vec<i16>,
    pub in_scale: f64,
    pub out_scale: f64,
}

impl QAct {
    pub fn silu(in_scale: f64, out_scale: f64) -> Self {
        Self { table: act_table_i16(silu, in_scale, out_scale), in_scale, out_scale }
    }

    pub fn gelu_tanh(in_scale: f64, out_scale: f64) -> Self {
        Self { table: act_table_i16(gelu_tanh, in_scale, out_scale), in_scale, out_scale }
    }

    /// Declare the table as the `i16` param `<name>.table` `[65536]`.
    pub fn declare(&self, pb: &mut ProgramBuilder, sink: &mut ParamSink, name: &str) -> Ref {
        sink.put(pb, &format!("{name}.table"), DType::I16, &[65_536], self.table.iter().map(|v| *v as i128).collect())
    }
}

/// `table[x + 32768]` for `i16` codes `x` of any shape.
pub fn lower_act(b: &mut BlockBuilder<'_>, x: Ref, table: Ref) -> Ref {
    b.act_table(x, table)
}

#[cfg(test)]
mod tests {
    use super::super::testkit::{Lcg, run_one_block};
    use super::*;

    #[test]
    fn an_activation_table_is_the_function_on_the_grid() {
        let mut rng = Lcg(5);
        let (sx, sy) = (1.0 / 2048.0, 1.0 / 2048.0);
        let x: Vec<i128> = (0..24).map(|_| rng.range(-20_000, 20_000)).collect();
        for (name, q, f) in
            [("silu", QAct::silu(sx, sy), silu as fn(f64) -> f64), ("gelu", QAct::gelu_tanh(sx, sy), gelu_tanh as fn(f64) -> f64)]
        {
            let x2 = x.clone();
            let y = run_one_block(
                |pb, sink| (sink.put(pb, "x", DType::I16, &[4, 6], x2), q.declare(pb, sink, name)),
                |b, (xs, t)| lower_act(b, xs, t),
            );
            assert_eq!(y.shape, vec![4, 6]);
            for (i, c) in x.iter().enumerate() {
                let want = f(*c as f64 * sx);
                let got = y.data[i] as f64 * sy;
                assert!((got - want).abs() <= 0.5 * sy + 1e-12, "{name}({c}) = {got} vs {want}");
            }
        }
    }

    #[test]
    fn the_composed_activations_are_the_functions_to_the_codes_precision() {
        let mut rng = Lcg(7);
        let (sx, sy) = (1.0 / 2048.0, 1.0 / 2048.0);
        let x: Vec<i128> = (0..48).map(|_| rng.range(-20_000, 20_000)).collect();
        for (act, f) in [(Act::Silu, silu as fn(f64) -> f64), (Act::GeluTanh, gelu_tanh as fn(f64) -> f64)] {
            let x2 = x.clone();
            let y = run_one_block(|pb, sink| sink.put(pb, "x", DType::I16, &[48], x2), |b, xs| lower_act_codes(b, xs, act, sx, sy));
            for (i, c) in x.iter().enumerate() {
                let want = (f(*c as f64 * sx) / sy).clamp(-32_767.0, 32_767.0);
                assert!((y.data[i] as f64 - want).abs() <= 3.0, "{act:?}({c}): code {} vs {want}", y.data[i]);
            }
        }
    }
}
