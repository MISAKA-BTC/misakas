//! Full fixed-scale experiment from authenticated input ID to greedy output ID.
//! Parameters of each layer are distinct instances even when the block is reused.
use super::{AttentionInputParams, DecoderBlockParams, attention_input, decoder_block_output};
use crate::{LowerError, Result};
use misaka_palw_tir::{
    DType, Ref, TensorType, TirProgramV1,
    builder::ProgramBuilder,
    library::Narrowing,
    program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN},
};

pub struct Qf1Program {
    pub program: TirProgramV1,
    pub final_norm: u16,
}

fn projection(p: &mut ProgramBuilder, name: &str, shape: [u32; 2], per_layer: bool) -> (Ref, Narrowing) {
    let w = p.param(name, DType::I8, &shape, per_layer);
    let m = p.param(&format!("{name}.multiplier"), DType::I64, &[shape[0]], per_layer);
    let s = p.param(&format!("{name}.shift"), DType::I8, &[shape[0]], per_layer);
    (w, Narrowing::new(m, s, None))
}

fn constant_scale(p: &mut ProgramBuilder, m: i64, s: i8) -> Narrowing {
    Narrowing::new(p.scalar(DType::I64, i128::from(m)), p.scalar(DType::I32, i128::from(s)), None)
}

/// The Q15 residual / Q10 normalized code recipe of the isolated conversion.
/// The caller supplies the native tiled logit scheme. This produces canonical
/// program bytes and declares every weight; no eligibility is inferred.
pub fn fixed_scale_program(tiled_logits: [u8; 64]) -> Result<Qf1Program> {
    const VOCAB: u32 = 151936;
    let mut p = ProgramBuilder::new(VOCAB, HISTORY_BOUND_V1_SMALL);
    let embed = p.param("model.embed_tokens.weight", DType::I16, &[VOCAB, 4096], false);
    let em = p.param("model.embed_tokens.weight.multiplier", DType::I64, &[VOCAB], false);
    let es = p.param("model.embed_tokens.weight.shift", DType::I8, &[VOCAB], false);
    let mut pre = p.block("embedding", vec![]);
    let token = Ref::Input(INPUT_TOKEN);
    let row = pre.gather(embed, token, 0, 0);
    let m = pre.gather(em, token, 0, 0);
    let s = pre.gather(es, token, 0, 0);
    let lifted = pre.narrow_codes(row, &Narrowing::new(m, s, None));
    let pre = pre.finish(&[lifted]);
    let gain_narrow = constant_scale(&mut p, 1, 34);
    let eps = |frac: u32| ((1_i128 << (24 + 2 * frac)) + 500_000) / 1_000_000;
    let residual_eps = p.scalar(DType::I64, eps(15));
    let head_eps = p.scalar(DType::I64, eps(10));
    let gains = [("input_layernorm.weight", 4096), ("self_attn.q_norm.weight", 128), ("self_attn.k_norm.weight", 128)]
        .map(|(n, w)| p.param(n, DType::I32, &[w], true));
    let projections = [("self_attn.q_proj.weight", 4096), ("self_attn.k_proj.weight", 1024), ("self_attn.v_proj.weight", 1024)]
        .map(|(n, r)| projection(&mut p, n, [r, 4096], true));
    let qkv = AttentionInputParams { gains, eps: [residual_eps, head_eps, head_eps], gain_narrow: [gain_narrow; 3], projections };
    let (m, s) = crate::quant::mul_shift(16.0 / libm::sqrt(128.0));
    let tail = DecoderBlockParams {
        rope_cos: p.param("rope.cos", DType::I32, &[8192, 64], false),
        rope_sin: p.param("rope.sin", DType::I32, &[8192, 64], false),
        key_history: p.hist_state("key", DType::I16, &[1024], 8192, true),
        value_history: p.hist_state("value", DType::I16, &[1024], 8192, true),
        score: constant_scale(&mut p, m, s),
        value: constant_scale(&mut p, 1, 24),
        output: projection(&mut p, "self_attn.o_proj.weight", [4096, 4096], true),
        post_eps: residual_eps,
        post_gain: p.param("post_attention_layernorm.weight", DType::I32, &[4096], true),
        post_narrow: gain_narrow,
        gate: projection(&mut p, "mlp.gate_proj.weight", [12288, 4096], true),
        up: projection(&mut p, "mlp.up_proj.weight", [12288, 4096], true),
        product: constant_scale(&mut p, 1, 24),
        down: projection(&mut p, "mlp.down_proj.weight", [4096, 12288], true),
    };
    let carry = vec![TensorType::fixed(DType::I16, &[4096])];
    let mut block = p.block("decoder", carry.clone());
    let x = Ref::CarryIn(0);
    let a = attention_input(&mut block, x, &qkv)?;
    let y = decoder_block_output(&mut block, x, &a, &tail)?;
    // Separate the two sides of the history reduction and the large MLP
    // projections so each terminal dispute fits the shipped t12 court.
    for site in [a.q, y.attention, y.after_attention, y.product] {
        block.commit(site);
    }
    let hidden = block.commit(y.residual);
    let decoder = block.finish(&[hidden]);
    let final_gain = p.param("model.norm.weight", DType::I32, &[4096], false);
    let head = projection(&mut p, "lm_head.weight", [VOCAB, 4096], false);
    let mut post = p.block("prediction", carry);
    let norm = post.rms_gain_codes(x, residual_eps, final_gain, &gain_narrow);
    let Ref::Node(final_norm) = post.commit(norm) else { unreachable!() };
    let logits = post.a16_matmul(head.0, norm, &head.1, true);
    let Ref::Node(logits_node) = post.commit(logits) else { unreachable!() };
    post.topk(logits, 0, 1);
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![decoder; 36], post, logits_node);
    program.logits_scheme_id = tiled_logits;
    misaka_palw_tir::validate::validate(&program).map_err(|e| LowerError::not_lowerable(e.to_string()))?;
    misaka_palw_tir::interval::analyze_ranges(&program).map_err(|e| LowerError::not_lowerable(e.to_string()))?;
    Ok(Qf1Program { program, final_norm })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_scale_program_has_in_range_reads_over_the_full_tir_position_domain() {
        let model = fixed_scale_program([0; 64]).unwrap();
        misaka_palw_tir::interval::analyze_ranges(&model.program).unwrap();
        let decoder = &model.program.blocks[usize::from(model.program.schedule.layers[0])];
        let rope: Vec<_> = model
            .program
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| p.name.starts_with("rope."))
            .map(|(i, _)| Ref::Param(i as u16))
            .collect();
        let indices: Vec<_> = decoder
            .nodes
            .iter()
            .filter(|n| matches!(n.prim, misaka_palw_tir::Prim::Gather { .. }) && rope.contains(&n.inputs[0]))
            .map(|n| n.inputs[1])
            .collect();
        assert_eq!(indices.len(), 2);
        assert_eq!(indices[0], indices[1]);
        let Ref::Node(i) = indices[0] else { panic!("RoPE position must be bounded") };
        let position = &decoder.nodes[usize::from(i)];
        assert_eq!(position.inputs.as_slice(), &[Ref::Input(misaka_palw_tir::program::INPUT_POS)]);
        assert_eq!(position.prim, misaka_palw_tir::Prim::Clamp { lo: 0, hi: 8191 });
    }
}
