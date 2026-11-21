use crate::{quant::mul_shift, rope::RopeFreqs};
use misaka_palw_tir::DType;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    error::Error,
    io::{BufRead, Read},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub const INVENTORY: &str = "b05a9dfc493f131e7bd2ca86dd8712cecee93e017bb0959faaa4427abfe59830";
#[derive(Deserialize)]
struct Record {
    dtype: String,
    shape: Vec<usize>,
    codes_sha256: String,
    absmax_bf16le_sha256: Option<String>,
}
#[derive(Deserialize)]
struct Manifest {
    tensors: BTreeMap<String, Record>,
}

fn read_verified(input: &mut impl Read, len: usize, expected: &str) -> Result<Vec<u8>> {
    let mut bytes = vec![0; len];
    input.read_exact(&mut bytes)?;
    if format!("{:x}", Sha256::digest(&bytes)) != expected {
        return Err("converted tensor digest mismatch".into());
    }
    Ok(bytes)
}

/// Read the pinned QF1 integer stream in upstream shard order and deliver the canonical
/// little-endian bytes for every graph parameter, including conversion scales and RoPE.
/// Callers must only publish the result after the upstream completion marker and EOF.
pub fn visit_fixed_scale_weights(
    input: &mut impl BufRead,
    inventory: &[u8],
    mut put: impl FnMut(&str, Option<u16>, Vec<u8>) -> Result<()>,
) -> Result<()> {
    if format!("{:x}", Sha256::digest(inventory)) != INVENTORY {
        return Err("wrong integer inventory".into());
    }
    let mut records = serde_json::from_slice::<Manifest>(inventory)?.tensors;
    let mut name = String::new();
    while !records.is_empty() {
        name.clear();
        input.take(256).read_line(&mut name)?;
        let name = name.trim_end_matches('\n');
        let record = records.remove(name).ok_or("missing, duplicate or unexpected tensor")?;
        let (base, layer) = if let Some(s) = name.strip_prefix("model.layers.") {
            let (l, n) = s.split_once('.').ok_or("malformed layer")?;
            (n, Some(l.parse::<u16>()?))
        } else {
            (name, None)
        };
        let dtype = match record.dtype.as_str() {
            "i1" => DType::I8,
            "<i2" => DType::I16,
            "<i4" => DType::I32,
            _ => return Err("tensor type".into()),
        };
        let n = record.shape.iter().product::<usize>();
        let raw = read_verified(input, n * dtype.width(), &record.codes_sha256)?;
        put(base, layer, raw)?;
        if let Some(digest) = record.absmax_bf16le_sha256 {
            let raw = read_verified(input, record.shape[0] * 2, &digest)?;
            let ratio = match base {
                "model.embed_tokens.weight" => 32768.,
                "lm_head.weight" => 4.,
                "self_attn.q_proj.weight" | "self_attn.k_proj.weight" | "self_attn.v_proj.weight" | "mlp.up_proj.weight" => 1.,
                "self_attn.o_proj.weight" | "mlp.down_proj.weight" => 32.,
                "mlp.gate_proj.weight" => 16384.,
                _ => return Err("missing conversion scale".into()),
            };
            let rail = if dtype == DType::I16 { 32767. } else { 127. };
            let (m, s): (Vec<_>, Vec<_>) = raw
                .chunks_exact(2)
                .map(|b| {
                    let peak = f64::from(f32::from_bits(u32::from(u16::from_le_bytes(b.try_into().unwrap())) << 16));
                    mul_shift((if peak == 0. { 1. } else { peak / rail }) * ratio)
                })
                .unzip();
            put(&format!("{base}.multiplier"), layer, m.into_iter().flat_map(i64::to_le_bytes).collect())?;
            put(&format!("{base}.shift"), layer, s.into_iter().map(|v| v as u8).collect())?;
        }
    }
    let mut end = [0; 4];
    input.read_exact(&mut end)?;
    if end != *b"END\n" || !input.fill_buf()?.is_empty() {
        return Err("incomplete upstream verification or extra bytes".into());
    }
    let rope = RopeFreqs::plain(1_000_000., 128);
    let (mut cos, mut sin) = (Vec::with_capacity(8192 * 64), Vec::with_capacity(8192 * 64));
    for pos in 0..8192 {
        let (c, s) = rope.cos_sin(pos);
        let q = |v: f32| (f64::from(v) * 16777216.).round() as i32;
        cos.extend(c.into_iter().map(q));
        sin.extend(s.into_iter().map(q));
    }
    put("rope.cos", None, cos.into_iter().flat_map(i32::to_le_bytes).collect())?;
    put("rope.sin", None, sin.into_iter().flat_map(i32::to_le_bytes).collect())?;
    Ok(())
}
