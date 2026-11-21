//! RFC-0066 dense Qwen3 lowering. All numeric operands come from the
//! descriptor's conversion recipe. These builders only emit ordinary TIR.
mod program;
mod weights;
pub use weights::{INVENTORY, visit_fixed_scale_weights};
pub use program::{Qf1Program, fixed_scale_program};

use crate::{LowerError, Result};
use misaka_palw_tir::{DType, Ref, TensorType, builder::BlockBuilder, library::Narrowing};

pub struct AttentionInputParams {
    /// Input, Q and K gains. The two 128-lane gains broadcast over heads.
    pub gains: [Ref; 3],
    /// Input epsilon, and separate Q and K epsilons in squared-code Q24.
    pub eps: [Ref; 3],
    /// Q24 unit times the channel gain, converted to each site's output codes.
    pub gain_narrow: [Narrowing; 3],
    /// Q, K and V weights [out,4096] and their per-output narrowing.
    pub projections: [(Ref, Narrowing); 3],
}

pub struct AttentionInput {
    pub input_norm: Ref,
    pub q: Ref,
    pub k: Ref,
    pub v: Ref,
    pub q_norm: Ref,
    pub k_norm: Ref,
}

/// Lower the first operations of any of the 36 layers. Q and K normalization
/// precede rotary position encoding; V stays in its projection's code scale.
pub fn attention_input(b: &mut BlockBuilder<'_>, residual: Ref, p: &AttentionInputParams) -> Result<AttentionInput> {
    // Check geometry before adding any nodes to the caller's block.
    if b.ty(residual) != TensorType::fixed(DType::I16, &[4096]) {
        return Err(LowerError::not_lowerable("QF1 residual must be i16[4096]"));
    }
    for (gain, width) in p.gains.into_iter().zip([4096, 128, 128]) {
        if b.ty(gain) != TensorType::fixed(DType::I32, &[width]) {
            return Err(LowerError::not_lowerable("QF1 integer gain has the wrong width or dtype"));
        }
    }
    for ((weights, _), outputs) in p.projections.iter().zip([4096, 1024, 1024]) {
        if b.ty(*weights) != TensorType::fixed(DType::I8, &[outputs, 4096]) {
            return Err(LowerError::not_lowerable("QF1 Q/K/V weights have the wrong shape or dtype"));
        }
    }
    let input_norm = b.rms_gain_codes(residual, p.eps[0], p.gains[0], &p.gain_narrow[0]);
    let [q, k, v] = p.projections.each_ref().map(|(weight, n)| b.a16_matmul(*weight, input_norm, n, false));
    let q_heads = b.reshape_fixed(q, &[32, 128]);
    let q_norm = b.rms_gain_codes(q_heads, p.eps[1], p.gains[1], &p.gain_narrow[1]);
    let k_heads = b.reshape_fixed(k, &[8, 128]);
    let k_norm = b.rms_gain_codes(k_heads, p.eps[2], p.gains[2], &p.gain_narrow[2]);
    Ok(AttentionInput { input_norm, q, k, v, q_norm, k_norm })
}

/// RFC-0066 block operands following the Q/K per-head normalizations.
pub struct DecoderBlockParams {
    pub rope_cos: Ref,
    pub rope_sin: Ref,
    pub key_history: u16,
    pub value_history: u16,
    pub score: Narrowing,
    pub value: Narrowing,
    pub output: (Ref, Narrowing),
    pub post_eps: Ref,
    pub post_gain: Ref,
    pub post_narrow: Narrowing,
    pub gate: (Ref, Narrowing),
    pub up: (Ref, Narrowing),
    pub product: Narrowing,
    pub down: (Ref, Narrowing),
}

pub struct DecoderBlockOutput {
    pub query: Ref,
    pub key: Ref,
    pub attention: Ref,
    pub after_attention: Ref,
    pub product: Ref,
    pub residual: Ref,
}

/// Emit rotary position encoding, inclusive causal history, grouped attention,
/// and the pre-normalized gated MLP. The final residual supplies POST_BLOCK.
pub fn decoder_block_output(
    b: &mut BlockBuilder<'_>,
    residual: Ref,
    input: &AttentionInput,
    p: &DecoderBlockParams,
) -> Result<DecoderBlockOutput> {
    use misaka_palw_tir::{library::attn::AttnCfg, program::INPUT_POS};

    for (weight, shape) in [(p.output.0, [4096, 4096]), (p.gate.0, [12288, 4096]), (p.up.0, [12288, 4096]), (p.down.0, [4096, 12288])]
    {
        if b.ty(weight) != TensorType::fixed(DType::I8, &shape) {
            return Err(LowerError::not_lowerable("QF1 output or MLP projection has the wrong shape or dtype"));
        }
    }
    if b.ty(p.post_gain) != TensorType::fixed(DType::I32, &[4096])
        || [p.rope_cos, p.rope_sin].into_iter().any(|t| b.ty(t) != TensorType::fixed(DType::I32, &[8192, 64]))
    {
        return Err(LowerError::not_lowerable("QF1 post-attention gain or rotary table geometry"));
    }
    // Admission ranges over the TIR history bound, which exceeds the QF1
    // context. In the registered context every legal position passes unchanged.
    let rope_pos = b.clamp(Ref::Input(INPUT_POS), 0, 8191, DType::Idx);
    let cos = b.gather(p.rope_cos, rope_pos, 0, 0);
    let sin = b.gather(p.rope_sin, rope_pos, 0, 0);
    let query = b.rope_half(input.q_norm, cos, sin, -32767, 32767, DType::I16);
    let key = b.rope_half(input.k_norm, cos, sin, -32767, 32767, DType::I16);
    let key_row = b.reshape_fixed(key, &[1024]);
    let keys = b.hist_append(p.key_history, key_row);
    let values = b.hist_append(p.value_history, input.v);
    let attention = b.attention(
        query,
        keys,
        values,
        &AttnCfg {
            heads: 32,
            kv_heads: 8,
            head_dim: 128,
            score: p.score,
            value: p.value,
            softcap: None,
            sink: None,
            alibi: None,
            up_bits: 0,
        },
    );
    let output = b.a16_matmul(p.output.0, attention, &p.output.1, false);
    let sum = b.add(residual, output, DType::I32);
    let after_attention = b.clamp(sum, -32767, 32767, DType::I16);
    let norm = b.rms_gain_codes(after_attention, p.post_eps, p.post_gain, &p.post_narrow);
    let gate = b.a16_matmul(p.gate.0, norm, &p.gate.1, true);
    let up = b.a16_matmul(p.up.0, norm, &p.up.1, false);
    let activation = b.silu(gate);
    let product = b.mul(activation, up, DType::I64);
    let product = b.narrow_codes(product, &p.product);
    let down = b.a16_matmul(p.down.0, product, &p.down.1, false);
    let sum = b.add(after_attention, down, DType::I32);
    let residual = b.clamp(sum, -32767, 32767, DType::I16);
    Ok(DecoderBlockOutput { query, key, attention, after_attention, product, residual })
}
