//! RFC-0066 dense Qwen3 lowering. All numeric operands come from the
//! descriptor's conversion recipe. These builders only emit ordinary TIR.
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
    // Reject an accidentally broadcast scalar gain or a transposed/folded
    // projection before adding any nodes to the caller's block.
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
