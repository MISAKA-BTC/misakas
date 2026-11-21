//! Stream the pinned, untied Qwen3-8B output head through the TIR weight
//! quantizer and integer projection on a fixed, non-mining A16 input.
use misaka_palw_tir::Rounding;
use misaka_palw_tir_exec::{
    kernels::matmul::{dot_i8_i16, dot_scalar},
    scalar::shr_round_i128,
};
use misaka_palw_tir_lower::{
    quant::{mul_shift, quantize_rows},
    weights::{parse_header, widen_floats},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs::File,
    io::{BufReader, Read},
};

const ROWS: usize = 151936;
const WIDTH: usize = 4096;
const PIN: &str = "20c2d6366ab85c90786ccdd829cd2b9e7d30ef3b2ebbb998280e7e4014b542ff";
const FILE_SIZE: u64 = 1_244_659_840;

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os().nth(1).ok_or("expected the pinned lm_head safetensors shard")?;
    let file = File::open(path)?;
    if file.metadata()?.len() != FILE_SIZE {
        return Err("incorrect length for the pinned shard".into());
    }
    let mut input = BufReader::new(file);
    let mut len = [0; 8];
    input.read_exact(&mut len)?;
    let len_value = u64::from_le_bytes(len);
    if len_value > 64 * 1024 {
        return Err("unexpected header length".into());
    }
    let mut header = len.to_vec();
    header.resize(8 + len_value as usize, 0);
    input.read_exact(&mut header[8..])?;
    let (entries, offset) = parse_header(&header, FILE_SIZE)?;
    let head = entries.get("lm_head.weight").ok_or("missing independent output head")?;
    if entries.len() != 1
        || head.dtype != "BF16"
        || head.shape != [ROWS, WIDTH]
        || head.begin != 0
        || head.end != (2 * ROWS * WIDTH) as u64
        || offset + head.end != FILE_SIZE
    {
        return Err("unexpected Qwen3-8B output-head extent or type".into());
    }

    // The probe fixes both activation and logit scales before reading the weights.
    // An accepted integer program will have its own immutable parameter manifest.
    let hidden: Vec<i16> = (0..WIDTH).map(|i| ((i * 73 + 19) % 2047) as i16 - 1023).collect();
    let mut source = Sha256::new();
    source.update(header);
    let mut weight_hash = Sha256::new();
    let mut scale_hash = Sha256::new();
    let mut logits_hash = Sha256::new();
    let mut best = (0, i32::MIN);
    let mut ties = 0;
    let mut minimum = i32::MAX;
    let mut samples = Vec::new();
    for start in (0..ROWS).step_by(256) {
        let count = (ROWS - start).min(256);
        let mut bytes = vec![0; 2 * WIDTH * count];
        input.read_exact(&mut bytes)?;
        source.update(&bytes);
        let weights = widen_floats("BF16", &bytes)?;
        if weights.iter().any(|x| !x.is_finite()) {
            return Err("nonfinite output weight".into());
        }
        let block = quantize_rows(&weights, count, WIDTH, None);
        weight_hash.update(block.codes.iter().map(|x| *x as u8).collect::<Vec<_>>());
        for (i, (codes, scale)) in block.codes.chunks_exact(WIDTH).zip(block.scales).enumerate() {
            let row = start + i;
            let (m, shift) = mul_shift(scale * 4.0);
            let dot = dot_i8_i16(codes, &hidden);
            if dot != dot_scalar(codes, &hidden) {
                return Err("scalar and native dot products differ".into());
            }
            let logit = shr_round_i128(i128::from(dot) * i128::from(m), shift as u32, Rounding::HalfAwayFromZero)
                .clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32;
            scale_hash.update(m.to_le_bytes());
            scale_hash.update([shift as u8]);
            logits_hash.update(logit.to_le_bytes());
            minimum = minimum.min(logit);
            if logit > best.1 {
                best = (row, logit);
                ties = 1;
            } else if logit == best.1 {
                ties += 1;
            }
            if [0, 151643, 151645, ROWS - 1].contains(&row) {
                samples.push(json!({"row":row,"accumulator":dot,"multiplier":m,"shift":shift,"logit":logit}));
            }
        }
    }
    if input.read(&mut [0u8])? != 0 || format!("{:x}", source.finalize()) != PIN {
        return Err("pinned shard digest differs".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "source_sha256":PIN,"source_bytes":FILE_SIZE,"rows":ROWS,"width":WIDTH,"input_scale":"1/1024","logit_scale":"1/4096",
            "weights_i8_sha256":format!("{:x}",weight_hash.finalize()),"mul_shift_sha256":format!("{:x}",scale_hash.finalize()),
            "logits_i32le_sha256":format!("{:x}",logits_hash.finalize()),"argmax":best.0,"max_logit":best.1,"min_logit":minimum,
            "max_ties":ties,"sample_rows":samples
        }))?
    );
    Ok(())
}
