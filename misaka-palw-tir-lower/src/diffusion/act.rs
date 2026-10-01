//! **`ACT_TABLE_V1`** — an activation as data on the A16 grid (PALW-EX-5: a transcendental at registration is
//! data): SiLU (the modulation's and the VAE's) and GELU-tanh (the feed-forward's) as a 65,536-entry table of `i16`
//! codes, indexed by the input code. One `Gather`; the table is the lossy site (the function rounded onto the
//! output grid), and the input's own rounding is the narrowing before it.

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::{DType, Ref};

use super::sink::ParamSink;
use super::tables::{act_table_i16, gelu_tanh, silu};

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
        let (sx, sy) = (1.0 / 2048.0, 1.0 / 4096.0);
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
}
