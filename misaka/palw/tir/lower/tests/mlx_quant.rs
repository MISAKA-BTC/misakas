//! **MLX's affine quantisation (`MLX_QUANT_V1`, the built-in descriptor `MLX_AFFINE`)**: what mlx-lm saves — a module's codes
//! under the float export's own name (`<module>.weight`, `uint32`, a little-endian bit stream per row), beside `<module>.scales`
//! and `<module>.biases` — is served as the float weight `scales · q + biases` under that name, through the virtual interpreter
//! with a served-name suffix (`serve_suffix: ".weight"`). MLX writes no `quant_method`: the configuration's `quantization`
//! (and `quantization_config`, which mlx-lm also writes) is named `mlx` by `prequant::quant_block`, and its per-module entries
//! (a module's own bits / group size, or `false`: kept in float) are read by the same descriptor. Nothing here is specific to a
//! model; the end-to-end fixtures (MLX itself as the quantiser) are `tests/quantized.rs`' `mlx_*`.

use misaka_palw_tir_lower::LowerError;
use misaka_palw_tir_lower::prequant::{MLX_QUANT_METHOD, QFormat, QuantConfig, is_mlx_block, parse_quant_config, quant_block};
use misaka_palw_tir_lower::quantfmt::QuantRegistry;
use misaka_palw_tir_lower::weights::described::{DescribedSource, served_names};
use misaka_palw_tir_lower::weights::{Checkpoint, TensorSource};
use serde_json::json;
use std::collections::BTreeSet;

/// A small deterministic generator (xorshift64*).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn write_safetensors(path: &std::path::Path, tensors: &[(&str, &str, Vec<usize>, Vec<u8>)]) {
    let mut header = serde_json::Map::new();
    let mut at = 0usize;
    for (name, dtype, shape, bytes) in tensors {
        header.insert((*name).into(), json!({"dtype": dtype, "shape": shape, "data_offsets": [at, at + bytes.len()]}));
        at += bytes.len();
    }
    let h = serde_json::to_vec(&header).expect("header");
    let mut out = (h.len() as u64).to_le_bytes().to_vec();
    out.extend(h);
    for (.., bytes) in tensors {
        out.extend_from_slice(bytes);
    }
    std::fs::write(path, out).expect("write");
}

/// One MLX module packed here, independently of the descriptor: `rows × cols` codes of `bits` as a little-endian bit stream per
/// row (code `i` in bits `[i·bits, (i+1)·bits)`, so 3, 5 and 6 bits straddle the words), float32 scales and biases per group,
/// and the float32 weight they define (f64 `s·q + b`, rounded once).
struct Packed {
    weight: Vec<u8>,
    words: usize,
    scales: Vec<u8>,
    biases: Vec<u8>,
    groups: usize,
    values: Vec<f32>,
}

fn pack(rng: &mut Rng, rows: usize, cols: usize, bits: usize, gs: usize) -> Packed {
    let words = cols * bits / 32;
    let groups = cols / gs;
    let (mut weight, mut scales, mut biases, mut values) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut s = vec![0f32; rows * groups];
    let mut b = vec![0f32; rows * groups];
    for k in 0..rows * groups {
        // A scale's sign follows MLX's rule (the edge of larger magnitude is the bias), so both signs occur.
        s[k] = (1 + rng.below(4000)) as f32 / 65536.0 * if rng.below(2) == 0 { 1.0 } else { -1.0 };
        b[k] = (rng.below(20000) as f32 - 10000.0) / 32768.0;
        scales.extend_from_slice(&s[k].to_le_bytes());
        biases.extend_from_slice(&b[k].to_le_bytes());
    }
    for o in 0..rows {
        let mut row = vec![0u32; words + 1];
        for i in 0..cols {
            let q = rng.below(1 << bits) as u64;
            let at = i * bits;
            let wide = (row[at / 32] as u64 | ((row[at / 32 + 1] as u64) << 32)) | (q << (at % 32));
            row[at / 32] = wide as u32;
            row[at / 32 + 1] = (wide >> 32) as u32;
            let g = o * groups + i / gs;
            values.push((s[g] as f64 * q as f64 + b[g] as f64) as f32);
        }
        assert_eq!(row[words], 0, "the codes fill the row's words exactly");
        weight.extend(row[..words].iter().flat_map(|w| w.to_le_bytes()));
    }
    Packed { weight, words, scales, biases, groups, values }
}

fn mlx_config(cfg: serde_json::Value) -> QuantConfig {
    parse_quant_config(&cfg, "LlamaForCausalLM", "llama").expect("an MLX block reads")
}

/// **A packed module is served as its float weight under its own name**, a module MLX left in float passes through, the
/// companions are hidden, a per-module entry (8 bits / group 64 in a 3-bit / group-32 model) decodes that module with its own
/// parameters, and every value is the one an independent packing defines, bit for bit.
#[test]
fn mlx_codes_are_served_as_the_float_weight_under_their_own_name_with_per_module_entries() {
    let reg = QuantRegistry::builtin();
    let f = reg.named("MLX_AFFINE").expect("the built-in descriptor").clone();
    let dir = std::env::temp_dir().join(format!("mlx-quant-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let mut rng = Rng(0x6d6c_7871);
    let q = pack(&mut rng, 3, 64, 3, 32);
    let d = pack(&mut rng, 2, 128, 8, 64);
    let kept: Vec<u8> = (0..2 * 32).flat_map(|i| (i as f32 / 64.0).to_le_bytes()).collect();
    let norm: Vec<u8> = (0..4).flat_map(|_| 1.0f32.to_le_bytes()).collect();
    let tensors = |kept_packed: bool| {
        let mut t = vec![
            ("m.q_proj.weight", "U32", vec![3, q.words], q.weight.clone()),
            ("m.q_proj.scales", "F32", vec![3, q.groups], q.scales.clone()),
            ("m.q_proj.biases", "F32", vec![3, q.groups], q.biases.clone()),
            ("m.down.weight", "U32", vec![2, d.words], d.weight.clone()),
            ("m.down.scales", "F32", vec![2, d.groups], d.scales.clone()),
            ("m.down.biases", "F32", vec![2, d.groups], d.biases.clone()),
            ("m.norm.weight", "F32", vec![4], norm.clone()),
        ];
        if kept_packed {
            t.push(("m.kept.weight", "U32", vec![3, q.words], q.weight.clone()));
            t.push(("m.kept.scales", "F32", vec![3, q.groups], q.scales.clone()));
            t.push(("m.kept.biases", "F32", vec![3, q.groups], q.biases.clone()));
        } else {
            t.push(("m.kept.weight", "F32", vec![2, 32], kept.clone()));
        }
        t
    };
    write_safetensors(&dir.join("model.safetensors"), &tensors(false));
    let qc = mlx_config(json!({"quant_method": "mlx", "group_size": 32, "bits": 3, "mode": "affine",
        "m.down": {"group_size": 64, "bits": 8}, "m.kept": false}));
    assert!(qc.fmt.is_virtual() && qc.fmt.label().starts_with("MLX_AFFINE"), "{}", qc.fmt.label());
    assert_eq!(qc.module_params.get("m.kept"), Some(&None), "`false`: kept in float");
    let (f2, params) = qc.fmt.binding().expect("a described format");
    assert_eq!(f2.name(), f.name());
    assert_eq!((params["bits"], params["group_size"]), (3, 32));
    let src =
        DescribedSource::new_with(Box::new(Checkpoint::open(&dir).expect("opens")), f2.clone(), params.clone(), &qc.module_params)
            .expect("serves");
    let names: BTreeSet<String> = src.names().into_iter().collect();
    let want: BTreeSet<String> =
        ["m.q_proj.weight", "m.down.weight", "m.norm.weight", "m.kept.weight"].iter().map(|s| s.to_string()).collect();
    assert_eq!(names, want, "the served weights and the float tensors; no companion is listed");
    assert_eq!(src.shape("m.q_proj.weight"), Some(vec![3, 64]));
    assert_eq!(src.shape("m.down.weight"), Some(vec![2, 128]), "the module's own entry: 8 bits");
    assert!(src.shape("m.q_proj.scales").is_none() && src.load("m.down.biases").is_err(), "the companions are hidden");
    for (name, p) in [("m.q_proj.weight", &q), ("m.down.weight", &d)] {
        let t = src.load(name).expect("decodes");
        assert_eq!(t.data.len(), p.values.len());
        for (k, (g, w)) in t.data.iter().zip(&p.values).enumerate() {
            assert!(g.to_bits() == w.to_bits() || (*g == 0.0 && *w == 0.0), "{name}[{k}]: {g} against {w}");
        }
        // A row range is the same rows of the whole.
        let cols = t.shape[1];
        assert_eq!(src.load_rows(name, 1..2).expect("rows").data, t.data[cols..2 * cols]);
    }
    let meta = src.metadata("m.down.weight").expect("a header");
    assert_eq!((meta.dtype.as_str(), meta.shape.clone()), ("F32", vec![2, 128]));
    assert_eq!(src.load("m.kept.weight").expect("float").data[5], 5.0 / 64.0, "a float module passes through");
    // A names-only listing is rewritten the same way.
    let listing: BTreeSet<String> = Checkpoint::open(&dir).expect("opens").names().into_iter().collect();
    assert_eq!(served_names(&listing, &f2), want);
    // Without the module's own entry, its codes do not fit the configuration's bits: refused by the descriptor's rule, by name.
    let e = DescribedSource::new(Box::new(Checkpoint::open(&dir).expect("opens")), f2.clone(), params.clone())
        .err()
        .expect("refused")
        .to_string();
    assert!(e.contains("m.down") && e.contains("whole number of codes"), "{e}");
    // An entry for a module the checkpoint does not hold packed, and a module declared float that it holds packed: refused.
    let ghost = mlx_config(
        json!({"quant_method": "mlx", "group_size": 32, "bits": 3, "m.down": {"group_size": 64, "bits": 8}, "m.ghost": {"bits": 4}}),
    );
    let e =
        DescribedSource::new_with(Box::new(Checkpoint::open(&dir).expect("opens")), f2.clone(), params.clone(), &ghost.module_params)
            .err()
            .expect("refused")
            .to_string();
    assert!(e.contains("m.ghost"), "{e}");
    write_safetensors(&dir.join("model.safetensors"), &tensors(true));
    let e = DescribedSource::new_with(Box::new(Checkpoint::open(&dir).expect("opens")), f2, params, &qc.module_params)
        .err()
        .expect("refused")
        .to_string();
    assert!(e.contains("m.kept") && e.contains("float"), "{e}");
    let _ = std::fs::remove_dir_all(dir);
}

fn block_of(root: serde_json::Value) -> Result<Option<serde_json::Value>, LowerError> {
    quant_block(root.as_object().expect("an object"))
}

/// **Where a configuration carries MLX's block, it is read as MLX's** — `quantization` alone (an older mlx-lm), both keys (mlx-lm
/// writes both), or `quantization_config` without a `quant_method` — and named `mlx`. Two blocks that disagree, a `quantization`
/// that is not MLX's, and one beside a `quantization_config` that names another method are refused by name; a
/// `quantization_config` without a method that is not MLX's stands as it is (the descriptor lookup refuses it, as before).
#[test]
fn an_mlx_block_is_named_and_read_wherever_the_configuration_carries_it() {
    let b = json!({"group_size": 64, "bits": 4});
    let named = |v: &serde_json::Value| v.get("quant_method").and_then(|m| m.as_str()) == Some(MLX_QUANT_METHOD);
    for root in [json!({"quantization": b}), json!({"quantization": b, "quantization_config": b}), json!({"quantization_config": b})] {
        let q = block_of(root.clone()).expect("reads").expect("a block");
        assert!(named(&q) && q["bits"] == 4 && q["group_size"] == 64, "{root} -> {q}");
    }
    assert_eq!(block_of(json!({"hidden_size": 8})).expect("reads"), None);
    assert_eq!(block_of(json!({"quantization": null, "quantization_config": null})).expect("reads"), None);
    let gptq = json!({"quant_method": "gptq", "bits": 4});
    assert_eq!(block_of(json!({"quantization_config": gptq})).expect("reads"), Some(gptq.clone()), "a named method stands as it is");
    let not_mlx = json!({"weights": "int4"});
    assert_eq!(block_of(json!({"quantization_config": not_mlx})).expect("reads"), Some(not_mlx), "no method, not MLX's: as it is");
    for (root, why) in [
        (json!({"quantization": b, "quantization_config": {"group_size": 32, "bits": 4}}), "differ"),
        (json!({"quantization": "int4"}), "not an MLX"),
        (json!({"quantization": {"bits": 4}}), "not an MLX"),
        (json!({"quantization": b, "quantization_config": gptq}), "two announcements"),
    ] {
        let e = block_of(root.clone()).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(e.contains(why), "{root}: {e}");
    }
    assert!(is_mlx_block(&json!({"group_size": 64, "bits": 4, "mode": "affine", "a.b": false, "c.d": {"bits": 8}})));
    assert!(!is_mlx_block(&json!({"group_size": 64, "bits": 4, "a.b": "x"})));
    assert!(!is_mlx_block(&json!({"group_size": 64, "bits": 4, "a.b": {"bits": 8, "zero_point": 1}})));
    assert!(
        !is_mlx_block(&json!({"quant_method": "mlx", "group_size": 64, "bits": 4})),
        "a block that names a method is that method's"
    );
}

/// **What MLX's format does not cover is refused by name**: another mode (MLX's mxfp4 stores other tensors), a bit width MLX does
/// not write, and an entry with a key MLX does not write or a value that is neither a bool nor an object.
#[test]
fn mlx_modes_and_entries_it_does_not_write_are_refused_by_name() {
    let refused = |cfg: serde_json::Value| {
        parse_quant_config(&cfg, "LlamaForCausalLM", "llama").err().map(|e| e.to_string()).unwrap_or_default()
    };
    let e = refused(json!({"quant_method": "mlx", "group_size": 32, "bits": 4, "mode": "mxfp4"}));
    assert!(e.contains("affine") && e.contains("mxfp4"), "{e}");
    let e = refused(json!({"quant_method": "mlx", "group_size": 32, "bits": 7}));
    assert!(e.contains("bits = 7"), "{e}");
    let e = refused(json!({"quant_method": "mlx", "group_size": 32, "bits": 4, "m.x": {"bits": 4, "zero_point": 1}}));
    assert!(e.contains("m.x") && e.contains("zero_point"), "{e}");
    let e = refused(json!({"quant_method": "mlx", "group_size": 32, "bits": 4, "m.x": "8bit"}));
    assert!(e.contains("m.x"), "{e}");
    let e = refused(json!({"quant_method": "mlx", "group_size": 32, "bits": 4, "m.x": {"bits": 7}}));
    assert!(e.contains("m.x") && e.contains("bits = 7"), "{e}");
    // `true` is the defaults.
    let c = parse_quant_config(&json!({"quant_method": "mlx", "group_size": 32, "bits": 4, "m.x": true}), "LlamaForCausalLM", "llama")
        .expect("reads");
    let QFormat::Described(d) = &c.fmt else { panic!("a described format") };
    assert_eq!(c.module_params.get("m.x"), Some(&Some(d.params.clone())));
}

/// **A Hugging Face configuration with MLX's block reads as a pre-quantised decoder**: `quantization` is the reader's key (not an
/// unread one), the spec carries the MLX format, and a `quantization` that is not MLX's is refused by name instead of being an
/// unread key.
#[test]
fn a_decoder_configuration_with_mlx_quantization_reads_without_an_unread_key() {
    let base = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real/llama-3.1-8b.json"))
        .expect("config");
    let mut cfg: serde_json::Value = serde_json::from_str(&base).expect("json");
    cfg["quantization"] = json!({"group_size": 64, "bits": 4});
    let spec = misaka_palw_tir_lower::parse_config(&cfg).expect("reads");
    let q = spec.hf.quant.as_ref().expect("the MLX format is attached");
    assert!(q.fmt.is_virtual() && q.fmt.label().starts_with("MLX_AFFINE"), "{}", q.fmt.label());
    // mlx-lm writes both keys; the same block twice reads the same.
    cfg["quantization_config"] = cfg["quantization"].clone();
    let both = misaka_palw_tir_lower::parse_config(&cfg).expect("reads");
    assert_eq!(both.hf.quant, spec.hf.quant);
    cfg["quantization"] = json!("int4");
    let e = misaka_palw_tir_lower::parse_config(&cfg).err().expect("refused").to_string();
    assert!(e.contains("not an MLX quantisation block"), "{e}");
}
