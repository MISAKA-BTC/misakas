//! **Checkpoint quantisation formats are data** (RFC-0002 Part II, the quant registry, `tensors`
//! layouts): GPTQ and AWQ decode through their descriptors exactly as the hand-written unpackers
//! (kept as oracles) do; a descriptor says how its `quantization_config` is read and which
//! configurations it refuses; a format nobody wrote code for is added by a file.
//!
//! The descriptors' own test vectors — random valid tensors and the weight an independent torch
//! implementation of each library's dequantiser gives — run when a descriptor is loaded
//! (`QuantFormat::from_json`); the registry test below counts them.

use misaka_palw_tir_lower::LowerError;
use misaka_palw_tir_lower::prequant::{QFormat, QuantConfig, parse_quant_config, parse_quant_config_with, skip_matches, unpack_awq, unpack_gptq};
use misaka_palw_tir_lower::quantfmt::tensors::RoleTensor;
use misaka_palw_tir_lower::quantfmt::{QuantFormat, QuantRegistry};
use misaka_palw_tir_lower::weights::Tensor;
use serde_json::json;

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
    fn word(&mut self) -> i32 {
        self.next() as u32 as i32
    }
}

fn bytes_i32(v: &[i32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn f16_bits(x: f32) -> u16 {
    // Round to the nearest binary16 for the values used here (0.01 .. 2, a few mantissa bits).
    let b = x.to_bits();
    let (s, e, m) = ((b >> 31) as u16, ((b >> 23) & 0xFF) as i32 - 127, b & 0x7F_FFFF);
    let he = e + 15;
    assert!((1..30).contains(&he), "{x} out of the range the test uses");
    let mut h = ((he as u32) << 10) | (m >> 13);
    if m & 0x1000 != 0 && (m & 0x1FFF != 0x1000 || h & 1 == 1) {
        h += 1;
    }
    (s << 15) | h as u16
}

fn scales(rng: &mut Rng, shape: &[usize]) -> (Tensor, Vec<u8>) {
    let n: usize = shape.iter().product();
    let mut data = Vec::with_capacity(n);
    let mut raw = Vec::with_capacity(2 * n);
    for _ in 0..n {
        let h = f16_bits(0.02 + rng.below(1000) as f32 / 700.0);
        raw.extend_from_slice(&h.to_le_bytes());
        data.push(misaka_palw_tir_lower::weights::f16_to_f32(h));
    }
    (Tensor::new(shape.to_vec(), data), raw)
}

fn role(shape: &[usize], dtype: &str, data: Vec<u8>) -> Option<RoleTensor> {
    Some(RoleTensor { shape: shape.to_vec(), dtype: dtype.into(), data })
}

/// The oracle's `QWeight` and the descriptor's, equal in everything but the label.
fn same(a: &misaka_palw_tir_lower::prequant::QWeight, b: &misaka_palw_tir_lower::prequant::QWeight, what: &str) {
    let mut b = b.clone();
    b.label = a.label.clone();
    assert_eq!(*a, b, "{what}");
}

#[test]
fn gptq_through_its_descriptor_is_the_hand_written_unpacker() {
    let mut rng = Rng(0x6770_7471);
    let mut cases = 0;
    for bits in [2u8, 4, 8] {
        for group in [0usize, 8, 16] {
            for (act, v2, sym) in [(false, false, true), (true, false, false), (true, true, false), (false, true, true)] {
                let (out, inp) = (16usize, 32usize);
                let pack = 32 / bits as usize;
                let gs = if group == 0 { inp } else { group };
                let ng = inp / gs;
                let qweight: Vec<i32> = (0..inp / pack * out).map(|_| rng.word()).collect();
                let qzeros: Vec<i32> = (0..ng * out / pack).map(|_| rng.word()).collect();
                let (sc, sc_raw) = scales(&mut rng, &[ng, out]);
                let g_idx: Vec<i32> = if act {
                    let mut perm: Vec<usize> = (0..inp).collect();
                    for i in (1..inp).rev() {
                        perm.swap(i, rng.below(i as u64 + 1) as usize);
                    }
                    perm.iter().map(|p| (p / gs) as i32).collect()
                } else {
                    (0..inp).map(|i| (i / gs) as i32).collect()
                };
                let fmt = QFormat::Gptq { bits, group, desc_act: act, sym, v2 };
                let want = unpack_gptq(&fmt, (&[inp / pack, out], &qweight), (&[ng, out / pack], &qzeros), &sc, Some((&[inp], &g_idx)));
                let (f, params) = fmt.binding().expect("a tensors format");
                let t = f.as_tensors().expect("tensors");
                let roles = vec![
                    role(&[inp / pack, out], "I32", bytes_i32(&qweight)),
                    role(&[ng, out / pack], "I32", bytes_i32(&qzeros)),
                    role(&[ng, out], "F16", sc_raw),
                    role(&[inp], "I32", bytes_i32(&g_idx)),
                ];
                let got = t.decode_integers(&roles, &params);
                match (want, got) {
                    (Ok(a), Ok(b)) => same(&a, &b, &format!("gptq b{bits} g{group} act={act} v2={v2} sym={sym}")),
                    (Err(_), Err(_)) => {}
                    (a, b) => panic!("gptq b{bits} g{group}: oracle {a:?}, descriptor {b:?}"),
                }
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 36);
}

#[test]
fn awq_through_its_descriptor_is_the_hand_written_unpacker() {
    let mut rng = Rng(0x0061_7771);
    for group in [0usize, 8, 16] {
        let (out, inp) = (16usize, 32usize);
        let gs = if group == 0 { inp } else { group };
        let ng = inp / gs;
        let qweight: Vec<i32> = (0..inp * out / 8).map(|_| rng.word()).collect();
        let qzeros: Vec<i32> = (0..ng * out / 8).map(|_| rng.word()).collect();
        let (sc, sc_raw) = scales(&mut rng, &[ng, out]);
        let fmt = QFormat::Awq { bits: 4, group };
        let a = unpack_awq(&fmt, (&[inp, out / 8], &qweight), (&[ng, out / 8], &qzeros), &sc).expect("oracle");
        let (f, params) = fmt.binding().expect("tensors");
        let roles = vec![role(&[inp, out / 8], "I32", bytes_i32(&qweight)), role(&[ng, out / 8], "I32", bytes_i32(&qzeros)), role(&[ng, out], "F16", sc_raw)];
        let b = f.as_tensors().expect("tensors").decode_integers(&roles, &params).expect("descriptor");
        same(&a, &b, &format!("awq g{group}"));
    }
}

/// The program structure a format lowers to was hand-coded (`order: true` for GPTQ, the offset term
/// for 8-bit asymmetric); it is now the descriptors' (`decode.order`, `decode.offset_term`) — and the
/// same.
#[test]
fn the_descriptors_give_the_layouts_the_code_used_to() {
    for bits in [2u8, 4, 8] {
        for group in [0usize, 32, 128] {
            for sym in [false, true] {
                let l = QFormat::Gptq { bits, group, desc_act: false, sym, v2: false }.layout();
                assert_eq!((l.group, l.order, l.offset_term), (group, true, bits == 8 && !sym), "gptq b{bits} g{group} sym={sym}");
            }
        }
    }
    for group in [0usize, 32, 128] {
        let l = QFormat::Awq { bits: 4, group }.layout();
        assert_eq!((l.group, l.order, l.offset_term), (group, false, false));
    }
}

#[test]
fn every_builtin_descriptor_carries_vectors_and_the_registry_knows_its_configs() {
    let reg = QuantRegistry::builtin();
    for f in reg.all() {
        assert!(!f.desc.tests.is_empty(), "{} has no vector", f.name());
    }
    for m in ["gptq", "awq", "fp8", "compressed-tensors/pack-quantized", "compressed-tensors/float-quantized", "compressed-tensors/int-quantized"] {
        assert!(reg.config_methods().contains(&m.to_string()), "{m}");
    }
    assert_eq!(reg.config("compressed-tensors", Some("pack-quantized")).map(|f| f.name()), Some("CT_PACK_QUANTIZED"));
    assert_eq!(reg.config("compressed-tensors", Some("Pack-Quantized")).map(|f| f.name()), Some("CT_PACK_QUANTIZED"));
    assert!(reg.config("compressed-tensors", Some("mixed-precision")).is_none());
    assert_eq!(reg.config("fp8", None).map(|f| f.name()), Some("FP8_BLOCK"));
}

fn ct_cfg() -> serde_json::Value {
    json!({
        "quant_method": "compressed-tensors", "format": "pack-quantized", "quantization_status": "compressed",
        "ignore": ["lm_head", "re:.*mlp.gate$", "model.layers.0.self_attn.k_proj"], "kv_cache_scheme": null, "sparsity_config": {},
        "transform_config": {}, "global_compression_ratio": 2.3, "version": "0.10.0",
        "config_groups": {"group_0": {"targets": ["Linear"], "input_activations": null, "output_activations": null,
            "weights": {"num_bits": 4, "type": "int", "symmetric": false, "strategy": "group", "group_size": 128, "actorder": "group", "dynamic": false, "observer": "minmax"}}}
    })
}

fn refusal(r: Result<QuantConfig, LowerError>) -> String {
    match r {
        Err(LowerError::NotLowerable(s)) | Err(LowerError::BadConfig(s)) => s,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_compressed_tensors_config_is_read_by_its_descriptor() {
    let c = parse_quant_config(&ct_cfg(), "LlamaForCausalLM", "llama").expect("parses");
    let QFormat::Described(d) = &c.fmt else { panic!("{:?}", c.fmt) };
    assert_eq!(d.format.name(), "CT_PACK_QUANTIZED");
    assert_eq!(d.params["bits"], 4);
    assert_eq!(d.params["group_size"], 128);
    assert_eq!((d.params["sym"], d.params["actorder"]), (0, 1));
    // Act-order gathers the input through the group index; 4-bit asymmetric needs no offset term.
    let l = c.fmt.layout();
    assert_eq!((l.group, l.order, l.offset_term), (128, true, false));
    // `ignore`: the head is not quantised, the router matches the pattern, an explicit name is exact.
    assert!(!c.lm_head);
    assert!(!c.converts("model.layers.3.mlp.gate") && c.converts("model.layers.3.mlp.gate_proj") && c.converts("model.layers.3.mlp.up_proj"));
    assert!(!c.converts("model.layers.0.self_attn.k_proj") && c.converts("model.layers.1.self_attn.k_proj"));
    // Without `lm_head` in `ignore`, the head is stored in the format.
    let mut v = ct_cfg();
    v["ignore"] = json!([]);
    assert!(parse_quant_config(&v, "LlamaForCausalLM", "llama").expect("parses").lm_head);
    // 8-bit asymmetric: the zero point does not fit i8 with the code, so the offset term is carried.
    let mut v = ct_cfg();
    v["config_groups"]["group_0"]["weights"]["num_bits"] = json!(8);
    assert!(parse_quant_config(&v, "LlamaForCausalLM", "llama").unwrap().fmt.layout().offset_term);
}

#[test]
fn what_a_descriptor_does_not_cover_is_refused_by_name() {
    let cases: Vec<(&str, Box<dyn Fn(&mut serde_json::Value)>, &str)> = vec![
        ("two config groups", Box::new(|v| v["config_groups"]["group_1"] = json!({"targets": ["re:.*q_proj"]})), "more than one config group"),
        ("a quantised KV cache", Box::new(|v| v["kv_cache_scheme"] = json!({"num_bits": 8, "type": "float"})), "KV cache"),
        ("a transform", Box::new(|v| v["transform_config"] = json!({"config_groups": {}, "x": 1})), "transforms"),
        ("a sparse checkpoint", Box::new(|v| v["sparsity_config"] = json!({"format": "sparse-24-bitmask"})), "sparse"),
        ("per-tensor weights", Box::new(|v| v["config_groups"]["group_0"]["weights"]["strategy"] = json!("tensor")), "other than group or channel"),
        ("float weights under the int format", Box::new(|v| v["config_groups"]["group_0"]["weights"]["type"] = json!("float")), "float-quantized"),
        ("targets other than Linear", Box::new(|v| v["config_groups"]["group_0"]["targets"] = json!(["re:.*q_proj"])), "targets other than"),
        ("2-bit codes", Box::new(|v| v["config_groups"]["group_0"]["weights"]["num_bits"] = json!(2)), "defined for"),
        ("a key nobody declared", Box::new(|v| v["mystery"] = json!(1)), "does not read"),
        ("an actorder it does not know", Box::new(|v| v["config_groups"]["group_0"]["weights"]["actorder"] = json!("sideways")), "defined for"),
    ];
    for (what, edit, needle) in cases {
        let mut v = ct_cfg();
        edit(&mut v);
        let e = refusal(parse_quant_config(&v, "LlamaForCausalLM", "llama"));
        assert!(e.contains(needle), "{what}: `{e}` does not mention `{needle}`");
    }
}

#[test]
fn a_method_no_descriptor_reads_says_what_to_supply() {
    let e = refusal(parse_quant_config(&json!({"quant_method": "bitsandbytes", "load_in_4bit": true}), "LlamaForCausalLM", "llama"));
    assert!(e.contains("quant_method=bitsandbytes") && e.contains("no quant-format descriptor") && e.contains("--quant-format"), "{e}");
    assert!(e.contains("compressed-tensors/pack-quantized") && e.contains("gptq"), "the refusal lists what is known: {e}");
    // A method that is known and not yet described says so, and what writing its descriptor takes.
    let e = refusal(parse_quant_config(&json!({"quant_method": "hqq", "quant_config": {}}), "LlamaForCausalLM", "llama"));
    assert!(e.contains("`hqq` is known and not yet described") && e.contains("zero point"), "{e}");
    assert!(misaka_palw_tir_lower::quantfmt::known_undescribed().iter().any(|k| k.method == "aqlm" && k.status == "known"));
    assert!(misaka_palw_tir_lower::quantfmt::known_undescribed().iter().any(|k| k.method == "bitsandbytes" && k.status == "queued"));
    let e = refusal(parse_quant_config(&json!({"quant_method": "compressed-tensors", "format": "mixed-precision", "config_groups": {}}), "LlamaForCausalLM", "llama"));
    assert!(e.contains("quant_method=compressed-tensors/mixed-precision"), "{e}");
}

#[test]
fn the_ignore_list_reads_a_small_regular_expression_subset_and_refuses_the_rest() {
    let s = |p: &[&str]| p.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let pats = s(&["re:.*mlp.gate$", "re:^lm_head", "re:model\\.visual.*", "exact:model.embed_tokens", "contains:norm"]);
    for (name, want) in [
        ("model.layers.4.mlp.gate", true),
        ("model.layers.4.mlp.gate_proj", false),
        ("lm_head", true),
        ("model.lm_head", false),
        ("model.visual.blocks.0.attn.qkv", true),
        ("modelXvisual.blocks", false),
        ("model.embed_tokens", true),
        ("model.embed_tokens2", false),
        ("model.layers.0.input_layernorm", true),
    ] {
        assert_eq!(skip_matches(&pats, name), want, "{name}");
    }
    // Outside the subset: refused when the configuration is read, not guessed at.
    for bad in ["re:(a|b)", "re:a+", "re:[abc]", "re:a{2}", "re:ab*"] {
        let mut v = ct_cfg();
        v["ignore"] = json!([bad]);
        let e = refusal(parse_quant_config(&v, "LlamaForCausalLM", "llama"));
        assert!(e.contains("outside the subset"), "{bad}: {e}");
    }
}

/// A format nobody wrote code for: one scale per output row and 4-bit signed codes packed two to a
/// byte along the input — added by a file, supplied to the registry, read from a `quantization_config`
/// and decoded with no change to this crate.
const TOYQ: &str = r#"{ "schema": "misaka.palw.quant-format.v1", "name": "TOYQ",
  "ids": [ { "scheme": "config", "method": "toyq" } ],
  "config": { "inert": ["version"], "skip": "modules_to_not_convert", "skip_match": "contains", "lm_head": "never" },
  "params": { "offset": { "config": "zero_offset", "default": 8, "allowed": [8] } },
  "layout": { "kind": "tensors",
    "roles": [ { "name": "qw", "suffix": ".qw", "dtypes": ["U8"], "rank": 2 }, { "name": "sc", "suffix": ".sc", "dtypes": ["F16", "F32"], "rank": 1 } ],
    "dims": { "out": "dim_qw[0]", "inp": "dim_qw[1] * 2" },
    "checks": [ { "expr": "dim_sc[0] == out", "message": "toyq has one scale per output row" } ] },
  "decode": { "target": "integers", "group": { "size": "inp" },
    "q": "(qw[o, i / 2] >> (4 * (i % 2))) & 15", "scale": "sc[o]", "zero": "offset", "code": { "min": 0, "max": 15 } } }"#;

#[test]
fn a_format_nobody_wrote_code_for_is_added_by_a_file() {
    let cfg = json!({"quant_method": "toyq", "version": 2, "modules_to_not_convert": ["lm_head"], "zero_offset": 8});
    let e = refusal(parse_quant_config(&cfg, "LlamaForCausalLM", "llama"));
    assert!(e.contains("quant_method=toyq") && e.contains("no quant-format descriptor"), "{e}");

    let dir = std::env::temp_dir().join(format!("quant-tensors-toyq-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("toyq.json");
    std::fs::write(&path, TOYQ).expect("write");
    let reg = QuantRegistry::with_files(std::slice::from_ref(&path)).expect("the descriptor loads");
    let c = parse_quant_config_with(&cfg, "LlamaForCausalLM", "llama", &reg).expect("now it parses");
    let QFormat::Described(d) = &c.fmt else { panic!() };
    assert_eq!(d.format.name(), "TOYQ");
    assert!(!c.converts("lm_head") && c.converts("model.layers.0.mlp.up_proj") && !c.lm_head);
    let l = c.fmt.layout();
    assert_eq!((l.group, l.order, l.offset_term), (0, false, false), "one group per row, no column order, codes − 8 fit i8");
    // Decode: 2 rows × 4 columns. Row 0 codes 0..=3, row 1 codes 15, 14, 13, 12; scales 0.5 and 0.25.
    let qw = vec![0x10u8, 0x32, 0xEF, 0xCD];
    let sc = [f16_bits(0.5).to_le_bytes(), f16_bits(0.25).to_le_bytes()].concat();
    let roles = vec![role(&[2, 2], "U8", qw), role(&[2], "F16", sc)];
    let (f, params) = c.fmt.binding().expect("binding");
    let t = f.as_tensors().expect("tensors");
    assert_eq!(t.dims(&roles, &params).expect("dims"), (2, 4));
    let w = t.decode_integers(&roles, &params).expect("decodes");
    assert_eq!(w.q, vec![0, 1, 2, 3, 15, 14, 13, 12]);
    assert_eq!((w.group, w.bits, w.signed), (4, 4, false));
    assert_eq!(w.value(0, 3), 0.5 * (3.0 - 8.0));
    assert_eq!(w.value(1, 0), 0.25 * (15.0 - 8.0));
    // A weight whose scales are not one per row is refused by the descriptor's own rule.
    let bad = vec![role(&[2, 2], "U8", vec![0; 4]), role(&[3], "F16", vec![0; 6])];
    let e = t.decode_integers(&bad, &params).unwrap_err().to_string();
    assert!(e.contains("one scale per output row"), "{e}");
    // The registry refuses to let a supplied file redefine a method it already reads.
    let mut again: serde_json::Value = serde_json::from_str(TOYQ).unwrap();
    again["name"] = json!("TOYQ2");
    let e = QuantRegistry::with_files(&[path]).and_then(|r| r.with(vec![QuantFormat::from_json(&again.to_string())?])).unwrap_err().to_string();
    assert!(e.contains("cannot redefine") && e.contains("toyq"), "{e}");
    let _ = std::fs::remove_dir_all(dir);
}

/// A minimal safetensors file: `(name, dtype, shape, bytes)` in the order given.
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

/// A format that decodes to floats reads whatever the quantiser converted — and the module it left
/// alone as the plain float tensor it is (a float is its own value). Neither is a guess: the format's
/// own roles and dtypes decide which it is, and every tensor read is accounted for.
#[test]
fn an_fp8_module_decodes_and_a_module_the_quantiser_left_in_bf16_is_read_as_it_is() {
    use misaka_palw_tir_lower::weights::{Checkpoint, Resolver, Src, eval_src};
    let dir = std::env::temp_dir().join(format!("quant-tensors-fp8-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    // e4m3fn: 0x38 = 1, 0x40 = 2, 0xB8 = -1, 0x30 = 0.5.
    let fp8: Vec<u8> = (0..32).map(|i| [0x38u8, 0x40, 0xB8, 0x30][i % 4]).collect();
    let bf16: Vec<u8> = (0..32).flat_map(|i| [0x3F80u16, 0x4000, 0xBF80, 0x3F00][i % 4].to_le_bytes()).collect();
    write_safetensors(
        &dir.join("model.safetensors"),
        &[
            ("m.weight", "F8_E4M3", vec![4, 8], fp8),
            ("m.weight_scale_inv", "F32", vec![1, 1], 0.5f32.to_le_bytes().to_vec()),
            ("n.weight", "BF16", vec![4, 8], bf16),
        ],
    );
    let cfg = json!({"quant_method": "fp8", "activation_scheme": "dynamic", "fmt": "e4m3", "weight_block_size": [4, 8]});
    let c = parse_quant_config(&cfg, "LlamaForCausalLM", "llama").expect("parses");
    assert!(!c.fmt.is_integers() && !c.lm_head);
    let ck = Checkpoint::open(&dir).expect("opens");
    let r = Resolver::new(&ck, &[]);
    let none = Default::default();
    let m = eval_src(&Src::Quant { module: "m".into(), fmt: c.fmt.clone() }, &r, None, &none).expect("an FP8 module");
    assert_eq!(m.shape, vec![4, 8]);
    assert_eq!(&m.data[..4], &[0.5, 1.0, -0.5, 0.25]);
    let n = eval_src(&Src::Quant { module: "n".into(), fmt: c.fmt.clone() }, &r, None, &none).expect("a bf16 module under an fp8 config");
    assert_eq!(&n.data[..4], &[1.0, 2.0, -1.0, 0.5]);
    assert!(r.untouched().is_empty(), "every tensor is read: {:?}", r.untouched());
    // A module with neither form is an error that says what is missing.
    let e = eval_src(&Src::Quant { module: "absent".into(), fmt: c.fmt }, &r, None, &none).unwrap_err().to_string();
    assert!(e.contains("missing tensor"), "{e}");
    let _ = std::fs::remove_dir_all(dir);
}
