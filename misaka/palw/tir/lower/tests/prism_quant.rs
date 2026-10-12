//! **PrismML's PTQ1_0 (ggml type 143) and PQ2_0 (type 142), decoded by data** (`quant-formats/{ptq1_0,pq2_0}.json`; COV-P1P2,
//! 2026-10-08).
//!
//! The layouts are those of PrismML-Eng/llama.cpp @`7dffb158de30ebb8ef9d64f33c6b0b2d7c1e6313` (release `prism-b10685`):
//! `block_ptq1_0` / `block_pq2_0` (ggml-common.h) and `dequantize_row_ptq1_0` / `dequantize_row_pq2_0` (ggml-quants.c), read as
//! text, never built or run. Two derivations from that text, sharing no code, are compared: the descriptor's expression (the Rust
//! interpreter, `quantfmt::blocks`) and `tools/prism_quant_reference.py` (a literal transcription of the C loops, run with `-I`),
//! which wrote each descriptor's test vectors — so a descriptor that disagreed with it would not load.
//!
//! The decoded integers are what a class commits: a code in `{0, 1, 2}` (PTQ1_0) or `{0..3}` (PQ2_0) with zero point 1 and one fp16
//! scale per 128 weights — the representation TQ1_0 and Q2_0 already lower through (`lower::qlinear`). A checkpoint that declares a
//! weight-space rotation beside them (`prism.hadamard.*`) is still refused by the GGUF reader (`tests/gguf_unmodelled.rs`): the
//! descriptor decodes the STORED weights, never the model's, until the rotation is lowered.
use misaka_palw_tir_lower::quantfmt::QuantRegistry;

#[test]
fn the_prism_descriptors_load_self_tested_under_their_ggml_ids() {
    let reg = QuantRegistry::builtin();
    let ptq = reg.ggml(143).expect("type 143 is described");
    assert_eq!(ptq.name(), "PTQ1_0");
    let b = ptq.as_blocks().expect("a block format");
    assert!(b.is_integers());
    assert_eq!(b.row_bytes(5120).unwrap(), 5120 / 128 * 28, "1.75 bits a weight: 28 bytes a 128-weight block");
    let pq2 = reg.ggml(142).expect("type 142 is described");
    assert_eq!(pq2.name(), "PQ2_0");
    assert_eq!(pq2.as_blocks().unwrap().row_bytes(5120).unwrap(), 5120 / 128 * 34);
}

#[test]
fn a_ptq1_0_block_decodes_to_ternary_integers_and_its_scale() {
    // All digits of 0xFF are 2 (+1) and every digit of 0x00 is 0 (−1): the extremes of the codec.
    let reg = QuantRegistry::builtin();
    let b = reg.ggml(143).unwrap().as_blocks().unwrap();
    let mut blk = vec![0xFFu8; 26];
    blk.extend([0x00, 0x3E]); // fp16 1.5
    let mut zero = vec![0u8; 26];
    zero.extend([0x00, 0xB4]); // fp16 −0.25
    let raw: Vec<u8> = blk.iter().chain(zero.iter()).copied().collect();
    let f = b.decode_floats(&raw, 1, 256).unwrap();
    assert!(f[..128].iter().all(|v| *v == 1.5) && f[128..].iter().all(|v| *v == 0.25), "{:?}", &f[..4]);
}

/// The REAL Mitsuba file (`hf-ckpt/isichan-ai/…/Mitsuba-ComfyUI-27B-v1.18-PTQ1_0.gguf`, H1's sha256-verified download): the first
/// eight rows of three tensors, decoded by the descriptor, against the BLAKE2b-512 digests the independent Python reader printed for the same
/// rows (`python3 -I tools/prism_quant_reference.py gguf <file> <tensor> 8`). Ignored: it needs the 6 GB file.
#[test]
#[ignore]
fn the_mitsuba_tensors_decode_as_the_independent_reader_decodes_them() {
    let path = std::env::var("MITSUBA_PTQ1_0").expect("MITSUBA_PTQ1_0 names the local GGUF");
    let file = misaka_palw_tir_lower::gguf::GgufFile::open(std::path::Path::new(&path)).expect("the GGUF");
    let reg = QuantRegistry::builtin();
    let b = reg.ggml(143).unwrap().as_blocks().unwrap();
    for (name, want, counts) in [
        (
            "blk.0.ssm_alpha.weight",
            "00e2270454f808a2479d8312f58939c241950b98d0601b2cbed6e4bd7eb07427ba0482f6e0b75062fc65a7d52e9eca4061b36bc82eb9af9da3f724a64ba5e9a3",
            (3_368, 34_199, 3_393),
        ),
        (
            "blk.0.attn_qkv.weight",
            "a366d0ef7244dd9816828cd2fa54d16ce0551e4b5e55110589be0056a75bcfcf9f7524044a7eefdb09756b91782b0fd1b07037e58960a042048683b2736273cc",
            (13_636, 13_440, 13_884),
        ),
        (
            "token_embd.weight",
            "1be5c15d0b572171095113053a301c185167f80f359142507f2c0f2982ea08b79f6b7b6538f8d6ab1ce05cda10355add17cb6bc22407bbf49e0fa833582d52b5",
            (13_672, 13_440, 13_848),
        ),
    ] {
        let rows = 8usize;
        let bytes = file.read_range(name, 0..(rows * b.row_bytes(5120).unwrap()) as u64).expect("the rows");
        let f = b.decode_floats(&bytes, rows, 5120).expect("decodes");
        let bytes_f32: Vec<u8> = f.iter().flat_map(|v| v.to_le_bytes()).collect();
        let got = blake2b_simd::blake2b(&bytes_f32).to_hex().to_string();
        assert_eq!(got, want, "{name}: the descriptor and the independent reader disagree");
        let q = b.decode_integers(&bytes, rows, 5120).expect("integers");
        let c = |k: i64| q.q.iter().filter(|x| **x as i64 - 1 == k).count();
        assert_eq!((c(-1), c(0), c(1)), counts, "{name}: ternary counts");
        assert!(q.q.iter().all(|x| *x <= 2), "{name}: a PTQ1_0 code is a base-3 digit");
    }
}
