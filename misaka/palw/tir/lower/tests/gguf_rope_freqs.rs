//! **`ROPE_FREQ_FACTORS_V1`: a GGUF's `rope_freqs.weight` is read as what it is — a table of frequency divisors.**
//!
//! llama.cpp writes a Llama-3.x rope scaling as the tensor `rope_freqs.weight` (and no metadata). The mapping used to read it only
//! when its bytes matched one of three Llama-3 parameter sets, and **needed the bytes**: a header-only view (a census, a preflight
//! of a file not yet downloaded) could not say anything, so 128–256 bytes made the whole file `NOT_RUN_NEEDS_TENSOR_DATA`.
//!
//! * with the data: a table that is a known Llama-3 scaling is the `llama3` rope (the program the Hugging Face configuration lowers
//!   to — unchanged); **any other table is the `freq_factors` rope with its own values** — a conversion never refuses a GGUF for the
//!   numbers in it;
//! * without it: the mapping reads a table of ones of the right length and records that the values are read at conversion
//!   (`pending_tensor_data`). The program's structure — every bound static admission checks — is the same for every table, which
//!   this file checks on the lowered programs (byte-identical under a different table because the rotation tables are params);
//! * a table of the wrong length or on another architecture is refused by name.
//!
//! No transformers here: a tiny llama written in this file with seeded weights; the equality of the `freq_factors` rope with
//! transformers' `llama3` table is `rope.rs`'s unit test (`freq_factors_divide_the_default_frequencies_and_reproduce_llama3_scaling`).

use misaka_palw_tir_lower::gguf::{GgufFile, GgufModel};
use misaka_palw_tir_lower::lower::LowerOpts;
use serde_json::Value;
use std::path::PathBuf;

const HIDDEN: u64 = 8;
const FFN: u64 = 16;
const VOCAB: u64 = 16;
const HEADS: u64 = 2;
const HEAD_DIM: u64 = HIDDEN / HEADS;
const THETA: f64 = 10000.0;

fn st(out: &mut Vec<u8>, s: &str) {
    out.extend((s.len() as u64).to_le_bytes());
    out.extend(s.as_bytes());
}

enum Kv {
    Str(&'static str),
    U32(u32),
    F32(f32),
}

fn f32s(n: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(12345);
    (0..n)
        .flat_map(|_| {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            (((x >> 9) as f32 / (1u32 << 23) as f32) * 0.2 - 0.1).to_le_bytes()
        })
        .collect()
}

/// A GGUF v3 of a one-layer llama (all F32) with a `rope_freqs.weight` of `factors`, and its offset of the data section.
fn tiny_llama(factors: &[f32]) -> (Vec<u8>, usize) {
    let meta: Vec<(&str, Kv)> = vec![
        ("general.architecture", Kv::Str("llama")),
        ("llama.context_length", Kv::U32(64)),
        ("llama.embedding_length", Kv::U32(HIDDEN as u32)),
        ("llama.block_count", Kv::U32(1)),
        ("llama.feed_forward_length", Kv::U32(FFN as u32)),
        ("llama.attention.head_count", Kv::U32(HEADS as u32)),
        ("llama.attention.head_count_kv", Kv::U32(HEADS as u32)),
        ("llama.attention.layer_norm_rms_epsilon", Kv::F32(1e-5)),
        ("llama.rope.freq_base", Kv::F32(THETA as f32)),
    ];
    let m = HIDDEN as usize;
    let f = FFN as usize;
    let v = VOCAB as usize;
    let freq_bytes: Vec<u8> = factors.iter().flat_map(|x| x.to_le_bytes()).collect();
    let tensors: Vec<(&str, Vec<u64>, Vec<u8>)> = vec![
        ("token_embd.weight", vec![HIDDEN, VOCAB], f32s(m * v, 1)),
        ("output_norm.weight", vec![HIDDEN], f32s(m, 2)),
        ("output.weight", vec![HIDDEN, VOCAB], f32s(m * v, 3)),
        ("blk.0.attn_norm.weight", vec![HIDDEN], f32s(m, 4)),
        ("blk.0.attn_q.weight", vec![HIDDEN, HIDDEN], f32s(m * m, 5)),
        ("blk.0.attn_k.weight", vec![HIDDEN, HIDDEN], f32s(m * m, 6)),
        ("blk.0.attn_v.weight", vec![HIDDEN, HIDDEN], f32s(m * m, 7)),
        ("blk.0.attn_output.weight", vec![HIDDEN, HIDDEN], f32s(m * m, 8)),
        ("blk.0.ffn_norm.weight", vec![HIDDEN], f32s(m, 9)),
        ("blk.0.ffn_gate.weight", vec![HIDDEN, FFN], f32s(m * f, 10)),
        ("blk.0.ffn_up.weight", vec![HIDDEN, FFN], f32s(m * f, 11)),
        ("blk.0.ffn_down.weight", vec![FFN, HIDDEN], f32s(f * m, 12)),
        ("rope_freqs.weight", vec![factors.len() as u64], freq_bytes),
    ];
    let mut out = b"GGUF".to_vec();
    out.extend(3u32.to_le_bytes());
    out.extend((tensors.len() as u64).to_le_bytes());
    out.extend((meta.len() as u64).to_le_bytes());
    for (k, val) in &meta {
        st(&mut out, k);
        match val {
            Kv::Str(s) => {
                out.extend(8u32.to_le_bytes());
                st(&mut out, s);
            }
            Kv::U32(n) => {
                out.extend(4u32.to_le_bytes());
                out.extend(n.to_le_bytes());
            }
            Kv::F32(x) => {
                out.extend(6u32.to_le_bytes());
                out.extend(x.to_le_bytes());
            }
        }
    }
    let mut off = 0u64;
    for (name, dims, data) in &tensors {
        st(&mut out, name);
        out.extend((dims.len() as u32).to_le_bytes());
        for d in dims {
            out.extend(d.to_le_bytes());
        }
        out.extend(0u32.to_le_bytes()); // F32
        out.extend(off.to_le_bytes());
        off += (data.len() as u64).div_ceil(32) * 32;
    }
    while !out.len().is_multiple_of(32) {
        out.push(0);
    }
    let data_start = out.len();
    for (_, _, data) in &tensors {
        out.extend(data);
        while !out.len().is_multiple_of(32) {
            out.push(0);
        }
    }
    (out, data_start)
}

fn on_disk(bytes: &[u8], tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tir-gguf-rope-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("m.gguf");
    std::fs::write(&p, bytes).unwrap();
    p
}

/// llama.cpp's divisors of a Llama-3 scaling (`LlamaModel.generate_extra_tensors`), in float32.
fn llama3_divisors(dim: usize, theta: f64, factor: f64, lo: f64, hi: f64, orig: f64) -> Vec<f32> {
    let (low_wl, high_wl) = (orig / lo, orig / hi);
    (0..dim / 2)
        .map(|i| {
            let freq = 1.0 / theta.powf((2 * i) as f64 / dim as f64);
            let wl = 2.0 * std::f64::consts::PI / freq;
            let f = if wl < high_wl {
                1.0
            } else if wl > low_wl {
                factor
            } else {
                let smooth = (orig / wl - lo) / (hi - lo);
                1.0 / ((1.0 - smooth) / factor + smooth)
            };
            f as f32
        })
        .collect()
}

fn scaling(m: &GgufModel) -> Value {
    m.config.get("rope_scaling").cloned().unwrap_or(Value::Null)
}

#[test]
fn a_header_only_view_reads_a_table_of_ones_and_says_its_values_are_read_at_conversion() {
    let factors = [1.0f32, 3.5];
    let (bytes, data_start) = tiny_llama(&factors);
    let head = GgufFile::parse(&bytes[..data_start], None, std::path::Path::new("header-only"), misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin()).unwrap();
    assert!(!head.has_data());
    // It used to be an error: "a header-only view holds no tensor data" (the census's `NOT_RUN_NEEDS_TENSOR_DATA`).
    let m = GgufModel::from_file(head).expect("a header-only view maps");
    let s = scaling(&m);
    assert_eq!(s["rope_type"], "freq_factors");
    assert_eq!(s["factors"], serde_json::json!([1.0, 1.0]));
    assert_eq!(m.pending_tensor_data(), ["rope_freqs.weight (8 bytes)".to_string()]);
    // The tensor is accounted for (not reported unread) and the model lowers.
    assert!(!m.unmapped().iter().any(|t| t == "rope_freqs.weight"), "{:?}", m.unmapped());
    m.prepare(&LowerOpts::default()).expect("the header-only mapping lowers");
}

#[test]
fn with_the_data_an_arbitrary_table_reads_as_itself_and_a_llama3_table_as_llama3() {
    // Any positive table: the freq_factors rope with its values.
    let (bytes, _) = tiny_llama(&[1.0, 3.5]);
    let m = GgufModel::open(&on_disk(&bytes, "any")).unwrap();
    let s = scaling(&m);
    assert_eq!(s["rope_type"], "freq_factors");
    assert_eq!(s["factors"], serde_json::json!([1.0, 3.5]));
    assert!(m.pending_tensor_data().is_empty());
    m.prepare(&LowerOpts::default()).expect("an unfamiliar table lowers");
    // A table that is a Llama-3 scaling: the llama3 rope, exactly as before.
    let f = llama3_divisors(HEAD_DIM as usize, THETA, 8.0, 1.0, 4.0, 8192.0);
    let (bytes, _) = tiny_llama(&f);
    let m = GgufModel::open(&on_disk(&bytes, "llama3")).unwrap();
    let s = scaling(&m);
    assert_eq!(s["rope_type"], "llama3", "{s}");
    assert_eq!(s["factor"], 8.0);
}

/// The static admission of a class is a function of its program's structure. The header-only reading (a table of ones) and the
/// reading with the data (any table) lower to the SAME program, byte for byte: the rotation tables are params, and the table's
/// values are weights, not structure. That is what makes a header-only judgment of this file a judgment of the file.
#[test]
fn the_program_does_not_depend_on_the_values_of_the_table() {
    let (bytes, data_start) = tiny_llama(&[1.0, 3.5]);
    let head = GgufFile::parse(&bytes[..data_start], None, std::path::Path::new("header-only"), misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin()).unwrap();
    let a = GgufModel::from_file(head).unwrap().prepare(&LowerOpts::default()).unwrap();
    let b = GgufModel::open(&on_disk(&bytes, "same")).unwrap().prepare(&LowerOpts::default()).unwrap();
    let c = {
        let (bytes, _) = tiny_llama(&[2.0, 9.25]);
        GgufModel::open(&on_disk(&bytes, "same2")).unwrap().prepare(&LowerOpts::default()).unwrap()
    };
    assert_eq!(a.lowered.program.encode(), b.lowered.program.encode(), "the header-only program is the program with the data");
    assert_eq!(b.lowered.program.encode(), c.lowered.program.encode(), "and any other table's");
}

#[test]
fn a_table_of_the_wrong_length_is_refused_by_name() {
    let (bytes, _) = tiny_llama(&[1.0, 2.0, 3.0]);
    let e = GgufModel::open(&on_disk(&bytes, "bad")).err().expect("refused").to_string();
    assert!(e.contains("rope_freqs.weight") && e.contains("needs 2"), "{e}");
}
