//! Private conversion operands for a complete real QF1 decoder block.
use super::{WIDTH, parameter};
use misaka_palw_tir::{DType, Ref, builder::ProgramBuilder, library::Narrowing};
use misaka_palw_tir_exec::Buf;
use misaka_palw_tir_lower::{qf1::DecoderBlockParams, quant::mul_shift, rope::RopeFreqs};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn read(dir: &Path, name: &str, suffix: &str, expected: &serde_json::Value) -> Vec<u8> {
    let bytes = fs::read(dir.join(format!("{name}.{suffix}"))).unwrap();
    assert_eq!(faster_hex::hex_string(&Sha256::digest(&bytes)), expected.as_str().unwrap());
    bytes
}

pub(super) fn operands(
    p: &mut ProgramBuilder,
    inputs: &mut Vec<Buf>,
    dir: &Path,
    eps: Ref,
    gain_scale: Narrowing,
) -> DecoderBlockParams {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/evidence/palw/mas1-qf1-native-2026-11-21/integer-weights.json"
    )))
    .unwrap();
    let mut projection = |name: &str, rows: usize, cols: usize, shift: i32| {
        let source = format!("model.layers.0.{name}.weight");
        let entry = &manifest["tensors"][&source];
        assert_eq!(entry["shape"], serde_json::json!([rows, cols]));
        let raw = read(dir, &source, "codes", &entry["codes_sha256"]);
        let peaks = read(dir, &source, "row_absmax.bf16", &entry["absmax_bf16le_sha256"]);
        assert_eq!(raw.len(), rows * cols);
        assert_eq!(peaks.len(), rows * 2);
        let (m, s): (Vec<_>, Vec<_>) = peaks
            .chunks_exact(2)
            .map(|bytes| {
                let hi = u16::from_le_bytes(bytes.try_into().unwrap()) as u32;
                let peak = f64::from(f32::from_bits(hi << 16));
                let scale = if peak == 0.0 { 1.0 } else { peak / 127.0 };
                mul_shift(scale * 2f64.powi(shift))
            })
            .unzip();
        let w = parameter(p, inputs, name, &[rows as u32, cols as u32], Buf::I8(raw.into_iter().map(|x| x as i8).collect()));
        let mult = parameter(p, inputs, &format!("{name}.multiplier"), &[rows as u32], Buf::I64(m));
        let shr = parameter(p, inputs, &format!("{name}.shift"), &[rows as u32], Buf::I8(s));
        (w, Narrowing::new(mult, shr, None))
    };
    // Q10 -> Q15 for both residual additions; gate -> Q24, up and product -> Q10.
    let output = projection("self_attn.o_proj", WIDTH, WIDTH, 5);
    let gate = projection("mlp.gate_proj", 12288, WIDTH, 14);
    let up = projection("mlp.up_proj", 12288, WIDTH, 0);
    let down = projection("mlp.down_proj", WIDTH, 12288, 5);
    let name = "model.layers.0.post_attention_layernorm.weight";
    let raw = read(dir, name, "codes", &manifest["tensors"][name]["codes_sha256"]);
    assert_eq!(raw.len(), WIDTH * 4);
    let post_gain = parameter(
        p,
        inputs,
        "post_attention_layernorm",
        &[4096],
        Buf::I32(raw.chunks_exact(4).map(|v| i32::from_le_bytes(v.try_into().unwrap())).collect()),
    );
    let rope = RopeFreqs::plain(1_000_000.0, 128);
    let mut cos = Vec::with_capacity(8192 * 64);
    let mut sin = Vec::with_capacity(8192 * 64);
    let q24 = |x: f32| (f64::from(x) * ((1 << 24) as f64)).round() as i32;
    for position in 0..8192 {
        let (c, s) = rope.cos_sin(position);
        cos.extend(c.into_iter().map(q24));
        sin.extend(s.into_iter().map(q24));
    }
    let rope_cos = parameter(p, inputs, "rope.cos", &[8192, 64], Buf::I32(cos));
    let rope_sin = parameter(p, inputs, "rope.sin", &[8192, 64], Buf::I32(sin));
    let key_history = p.hist_state("key", DType::I16, &[1024], 8192, true);
    let value_history = p.hist_state("value", DType::I16, &[1024], 8192, true);
    let mut narrow = |m: i64, s: i8| Narrowing::new(p.scalar(DType::I64, m as i128), p.scalar(DType::I32, s as i128), None);
    let (m, s) = mul_shift(16.0 / 128_f64.sqrt());
    let score = narrow(m, s);
    let q24_to_code = narrow(1, 24);
    DecoderBlockParams {
        rope_cos,
        rope_sin,
        key_history,
        value_history,
        score,
        value: q24_to_code,
        output,
        post_eps: eps,
        post_gain,
        post_narrow: gain_scale,
        gate,
        up,
        product: q24_to_code,
        down,
    }
}
