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
    let e = refusal(parse_quant_config(&json!({"quant_method": "brand_new_quant", "bits": 4}), "LlamaForCausalLM", "llama"));
    assert!(e.contains("quant_method=brand_new_quant") && e.contains("no quant-format descriptor") && e.contains("--quant-format"), "{e}");
    assert!(e.contains("compressed-tensors/pack-quantized") && e.contains("gptq"), "the refusal lists what is known: {e}");
    // bitsandbytes is described now — under conditions, and the refusal lists them.
    assert!(e.contains("bitsandbytes [load_in_4bit = true, bnb_4bit_quant_type = \"nf4\"]"), "{e}");
    // A method that is known and not yet described says so, and what writing its descriptor takes.
    let e = refusal(parse_quant_config(&json!({"quant_method": "hqq", "quant_config": {}}), "LlamaForCausalLM", "llama"));
    assert!(e.contains("`hqq` is known and not yet described") && e.contains("zero point"), "{e}");
    assert!(misaka_palw_tir_lower::quantfmt::known_undescribed().iter().any(|k| k.method == "aqlm" && k.status == "known"));
    assert!(misaka_palw_tir_lower::quantfmt::known_undescribed().iter().all(|k| k.method != "bitsandbytes"), "a described method is no longer listed as undescribed");
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

/// An independent decode of one MXFP4 element, for the test: the OCP FP4 (E2M1) value of a code times
/// two to the E8M0 exponent of its block.
fn mxfp4_element(blocks: &[u8], scales: &[u8], (e_n, o_n, g_n): (usize, usize, usize), (e, i, o): (usize, usize, usize)) -> f32 {
    let _ = e_n;
    let (ib, t) = (i / 32, i % 32);
    let byte = blocks[((e * o_n + o) * g_n + ib) * 16 + t / 2];
    let code = (byte >> (4 * (t % 2))) & 15;
    let mag = [0.0f32, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0][(code & 7) as usize];
    let v = if code & 8 != 0 { -mag } else { mag };
    v * 2f32.powi(scales[(e * o_n + o) * g_n + ib] as i32 - 127)
}

/// A format that serves packed tensors as float tensors — a leading expert axis, per-block scales over a
/// 3-D tensor — lists the served names in place of the packed ones, serves any row range without decoding
/// the rest, and hides the packed tensors. Nothing in it is specific to a model.
#[test]
fn a_virtual_format_serves_packed_tensors_as_the_float_export_and_by_row_ranges() {
    use misaka_palw_tir_lower::weights::described::{DescribedSource, served_names};
    use misaka_palw_tir_lower::weights::{Checkpoint, TensorSource};
    let reg = QuantRegistry::builtin();
    let f = reg.named("MXFP4_HF").expect("the built-in descriptor").clone();
    let dir = std::env::temp_dir().join(format!("quant-tensors-mxfp4-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let (e_n, o_n, g_n) = (3usize, 5usize, 2usize);
    let mut rng = Rng(0x6d78_6670);
    let blocks: Vec<u8> = (0..e_n * o_n * g_n * 16).map(|_| rng.below(256) as u8).collect();
    let scales: Vec<u8> = (0..e_n * o_n * g_n).map(|_| (120 + rng.below(12)) as u8).collect();
    write_safetensors(
        &dir.join("model.safetensors"),
        &[
            ("m.mlp.experts.gate_up_proj_blocks", "U8", vec![e_n, o_n, g_n, 16], blocks.clone()),
            ("m.mlp.experts.gate_up_proj_scales", "U8", vec![e_n, o_n, g_n], scales.clone()),
            ("m.mlp.experts.gate_up_proj_bias", "F32", vec![e_n, o_n], vec![0u8; e_n * o_n * 4]),
            ("m.self_attn.q_proj.weight", "F32", vec![2, 2], vec![0u8; 16]),
        ],
    );
    let src = DescribedSource::new(Box::new(Checkpoint::open(&dir).expect("opens")), f.clone(), Default::default()).expect("serves");
    let name = "m.mlp.experts.gate_up_proj";
    let names = src.names();
    assert!(names.contains(&name.to_string()) && !names.iter().any(|n| n.ends_with("_blocks") || n.ends_with("_scales")), "{names:?}");
    assert!(names.contains(&"m.mlp.experts.gate_up_proj_bias".to_string()) && names.contains(&"m.self_attn.q_proj.weight".to_string()), "the other tensors pass through");
    // [E, in, out]: the packed [E, out, in] transposed, exactly as the float export has it.
    assert_eq!(src.shape(name), Some(vec![e_n, g_n * 32, o_n]));
    assert!(src.shape("m.mlp.experts.gate_up_proj_blocks").is_none(), "the packed tensor is hidden");
    assert!(src.load("m.mlp.experts.gate_up_proj_scales").is_err());
    let whole = src.load(name).expect("decodes");
    for e in 0..e_n {
        for i in 0..g_n * 32 {
            for o in 0..o_n {
                let want = mxfp4_element(&blocks, &scales, (e_n, o_n, g_n), (e, i, o));
                let got = whole.data[(e * g_n * 32 + i) * o_n + o];
                assert!(got == want || (got == 0.0 && want == 0.0), "[{e}, {i}, {o}]: {got} vs {want}");
            }
        }
    }
    // A row range (every axis but the last flattened) is the same rows of the whole.
    let rows = src.load_rows(name, 17..40).expect("rows");
    assert_eq!(rows.shape, vec![23, o_n]);
    assert_eq!(rows.data, whole.data[17 * o_n..40 * o_n]);
    let meta = src.metadata(name).expect("a header");
    assert_eq!((meta.dtype.as_str(), meta.bytes), ("F32", (e_n * g_n * 32 * o_n * 4) as u64));
    let bytes = src.read_slice(name, 8..24).expect("bytes");
    assert_eq!(bytes, whole.data[2..6].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>());
    // A names-only listing (no tensors) is rewritten the same way.
    let listing: std::collections::BTreeSet<String> = Checkpoint::open(&dir).expect("opens").names().into_iter().collect();
    let served = served_names(&listing, &f);
    assert!(served.contains(name) && !served.contains("m.mlp.experts.gate_up_proj_blocks") && served.contains("m.self_attn.q_proj.weight"));
    // A module whose scales are not one per block breaks the descriptor's own rule, by name.
    write_safetensors(
        &dir.join("model.safetensors"),
        &[
            ("bad.experts.down_proj_blocks", "U8", vec![e_n, o_n, g_n, 16], blocks),
            ("bad.experts.down_proj_scales", "U8", vec![e_n, o_n, g_n + 1], vec![127u8; e_n * o_n * (g_n + 1)]),
        ],
    );
    let e = DescribedSource::new(Box::new(Checkpoint::open(&dir).expect("opens")), f, Default::default()).err().expect("refused").to_string();
    assert!(e.contains("bad.experts.down_proj") && e.contains("one per block"), "{e}");
    let _ = std::fs::remove_dir_all(dir);
}

/// gpt-oss's `quantization_config` is read by the descriptor: its keys are known, a key nobody declared is
/// refused, and `modules_to_not_convert` is a list of the regular expressions Hugging Face matches with.
#[test]
fn an_mxfp4_configuration_is_read_by_its_descriptor() {
    let cfg = json!({"quant_method": "mxfp4", "modules_to_not_convert": ["model.layers.*.self_attn", "model.layers.*.mlp.router", "model.embed_tokens", "lm_head"]});
    let c = parse_quant_config(&cfg, "GptOssForCausalLM", "gpt_oss").expect("parses");
    assert!(c.fmt.is_virtual() && !c.fmt.is_integers() && !c.lm_head);
    assert!(!c.converts("model.layers.7.self_attn.q_proj") && !c.converts("model.layers.7.mlp.router") && !c.converts("lm_head"));
    assert!(c.converts("model.layers.7.mlp.experts"), "the experts are what the format is for");
    let mut v = cfg.clone();
    v["mystery"] = json!(1);
    let e = refusal(parse_quant_config(&v, "GptOssForCausalLM", "gpt_oss"));
    assert!(e.contains("does not read") && e.contains("mystery"), "{e}");
}


// ───────────────────────────── bitsandbytes ─────────────────────────────

fn bnb_cfg(load4: bool, qtype: &str, dq: bool) -> serde_json::Value {
    json!({"_load_in_4bit": load4, "_load_in_8bit": !load4, "bnb_4bit_compute_dtype": "bfloat16", "bnb_4bit_quant_storage": "uint8",
           "bnb_4bit_quant_type": qtype, "bnb_4bit_use_double_quant": dq, "llm_int8_enable_fp32_cpu_offload": false,
           "llm_int8_has_fp16_weight": false, "llm_int8_skip_modules": null, "llm_int8_threshold": if load4 { json!(6.0) } else { json!(0.0) },
           "load_in_4bit": load4, "load_in_8bit": !load4, "quant_method": "bitsandbytes"})
}

fn name_of(c: &QuantConfig) -> String {
    let QFormat::Described(d) = &c.fmt else { panic!("{:?}", c.fmt) };
    d.format.name().to_string()
}

/// One `quant_method` announces three descriptors, told apart by other keys of the configuration.
#[test]
fn a_bitsandbytes_configuration_picks_nf4_fp4_or_int8_by_its_own_keys() {
    let read = |v: &serde_json::Value| parse_quant_config(v, "LlamaForCausalLM", "llama");
    assert_eq!(name_of(&read(&bnb_cfg(true, "nf4", true)).expect("nf4")), "BNB_NF4");
    assert_eq!(name_of(&read(&bnb_cfg(true, "fp4", false)).expect("fp4")), "BNB_FP4");
    assert_eq!(name_of(&read(&bnb_cfg(false, "fp4", false)).expect("int8")), "BNB_INT8", "the 4-bit keys an 8-bit configuration carries as defaults are not read");
    // The 4-bit type defaults to fp4 when the key is absent (as BitsAndBytesConfig's constructor does).
    let mut v = bnb_cfg(true, "fp4", false);
    v.as_object_mut().unwrap().remove("bnb_4bit_quant_type");
    assert_eq!(name_of(&read(&v).expect("fp4 by default")), "BNB_FP4");
    // A 4-bit type with no descriptor is a refusal that lists what is read, with the conditions.
    let e = refusal(read(&bnb_cfg(true, "af4", false)));
    assert!(e.contains("quant_method=bitsandbytes") && e.contains("no quant-format descriptor") && e.contains("bnb_4bit_quant_type"), "{e}");
    // Neither width: nothing to read.
    let mut v = bnb_cfg(true, "nf4", false);
    v["load_in_4bit"] = json!(false);
    assert!(refusal(read(&v)).contains("no quant-format descriptor"));
    // The head is stored in float unless the checkpoint says otherwise (here it is not skipped: the format would read it, and falls back to the plain tensor).
    let c = read(&bnb_cfg(true, "nf4", true)).unwrap();
    assert!(c.lm_head);
}

/// `llm_int8_skip_modules` is read as transformers reads it: a module's last component, its whole name, or a path prefix.
#[test]
fn llm_int8_skip_modules_are_read_as_transformers_reads_them() {
    let mut v = bnb_cfg(true, "nf4", false);
    v["llm_int8_skip_modules"] = json!(["lm_head", "down_proj", "gate", "model.layers.0.self_attn"]);
    let c = parse_quant_config(&v, "LlamaForCausalLM", "llama").unwrap();
    assert!(!c.lm_head, "the head is named");
    for (name, converted) in [
        ("lm_head", false),
        ("model.layers.3.mlp.down_proj", false),             // the last component
        ("model.layers.3.mlp.experts.5.down_proj", false),   // ... of an expert too
        ("model.layers.3.mlp.gate", false),                  // the router: its last component is `gate`
        ("model.layers.3.mlp.gate_proj", true),              // `gate_proj` is not `gate`, and `gate.` does not occur in it
        ("model.layers.0.self_attn.q_proj", false),          // a path prefix: `model.layers.0.self_attn.` occurs in it
        ("model.layers.1.self_attn.q_proj", true),
        ("model.layers.3.mlp.up_proj", true),
    ] {
        assert_eq!(c.converts(name), converted, "{name}");
    }
    // A key that is a suffix of the path but neither a whole name, a last component nor followed by a dot does not skip.
    assert!(!skip_matches(&["path:mlp.gate".to_string()], "model.layers.3.mlp.gate"));
    assert!(skip_matches(&["path:layers.3".to_string()], "model.layers.3.mlp.gate"), "a path prefix `layers.3.` inside the name");
    assert!(!skip_matches(&["path:layers.3".to_string()], "model.layers.30.mlp.gate"));
}

/// What a descriptor does not cover is refused BY NAME — int8 outlier decomposition above all.
#[test]
fn what_bitsandbytes_cannot_reproduce_is_refused_by_name() {
    let read = |v: &serde_json::Value| parse_quant_config(v, "LlamaForCausalLM", "llama");
    // LLM.int8 with the default threshold decomposes each matmul at run time.
    let mut v = bnb_cfg(false, "fp4", false);
    v["llm_int8_threshold"] = json!(6.0);
    let e = refusal(read(&v));
    assert!(e.contains("llm_int8_threshold") && e.contains("outlier decomposition") && e.contains("run time") && e.contains("llm_int8_threshold 0"), "{e}");
    // ... threshold 0 (written as an integer or a float) is read.
    for t in [json!(0), json!(0.0)] {
        let mut v = bnb_cfg(false, "fp4", false);
        v["llm_int8_threshold"] = t;
        assert_eq!(name_of(&read(&v).expect("threshold 0")), "BNB_INT8");
    }
    // 4-bit weights packed into another storage dtype have another stored shape.
    let mut v = bnb_cfg(true, "nf4", false);
    v["bnb_4bit_quant_storage"] = json!("bfloat16");
    assert!(refusal(read(&v)).contains("storage dtype other than uint8"));
    // A key nobody declared.
    let mut v = bnb_cfg(true, "nf4", false);
    v["bnb_4bit_something_new"] = json!(1);
    let e = refusal(read(&v));
    assert!(e.contains("does not read") && e.contains("bnb_4bit_something_new"), "{e}");
}

fn bnb_vector(file: &str, n: usize) -> (QuantFormat, serde_json::Value) {
    let text = std::fs::read_to_string(format!("{}/quant-formats/{file}.json", env!("CARGO_MANIFEST_DIR"))).expect("descriptor");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    (QuantFormat::from_json(&text).expect("loads"), v["tests"][n].clone())
}

/// The role tensors of a descriptor's test vector, in the descriptor's role order.
fn vector_roles(f: &QuantFormat, vec: &serde_json::Value) -> Vec<Option<RoleTensor>> {
    let t = f.as_tensors().expect("a tensors format");
    t.roles()
        .map(|(name, ..)| {
            vec["roles"].get(name).map(|r| RoleTensor {
                shape: r["shape"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as usize).collect(),
                dtype: r["dtype"].as_str().unwrap().to_string(),
                data: (0..r["hex"].as_str().unwrap().len() / 2).map(|i| u8::from_str_radix(&r["hex"].as_str().unwrap()[2 * i..2 * i + 2], 16).unwrap()).collect(),
            })
        })
        .collect()
}

/// A corrupted vector is refused: the vectors are checked, not decoration.
#[test]
fn a_bitsandbytes_descriptor_whose_vector_is_wrong_is_refused() {
    for file in ["bnb_nf4", "bnb_fp4", "bnb_int8"] {
        let text = std::fs::read_to_string(format!("{}/quant-formats/{file}.json", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let hex = v["tests"][0]["values_f32_hex"].as_str().unwrap().to_string();
        let mut bytes: Vec<char> = hex.chars().collect();
        bytes[9] = if bytes[9] == '0' { '1' } else { '0' };
        v["tests"][0]["values_f32_hex"] = json!(bytes.into_iter().collect::<String>());
        assert!(QuantFormat::from_json(&v.to_string()).is_err(), "{file}: a wrong expected value was accepted");
    }
}

/// Double quantisation is a property of the module: absmax is 8-bit exactly when the nested statistics are there.
#[test]
fn a_double_quantised_module_with_float_absmax_or_a_plain_one_with_byte_absmax_is_refused() {
    let (f, vec) = bnb_vector("bnb_nf4", 3); // (5, 24) bs 64, double quantised
    let t = f.as_tensors().unwrap();
    let params = f.read_config(&vec["config"]).expect("config").params;
    let good = vector_roles(&f, &vec);
    assert!(t.decode_floats(&good, &params).is_ok());
    let idx = |name: &str| t.roles().position(|(n, ..)| n == name).unwrap();
    // The nested tensors gone: absmax is bytes and nothing scales them.
    let mut r = good.clone();
    r[idx("nabs")] = None;
    r[idx("nmap")] = None;
    let e = t.decode_floats(&r, &params).unwrap_err().to_string();
    assert!(e.contains("8-bit exactly when"), "{e}");
    // Only one of the two.
    let mut r = good.clone();
    r[idx("nmap")] = None;
    let e = t.decode_floats(&r, &params).unwrap_err().to_string();
    assert!(e.contains("come together"), "{e}");
    // Float absmax under nested tensors.
    let mut r = good.clone();
    let a = r[idx("absmax")].take().unwrap();
    let n = a.shape[0];
    r[idx("absmax")] = Some(RoleTensor { shape: vec![n], dtype: "F32".into(), data: vec![0u8; 4 * n] });
    let e = t.decode_floats(&r, &params).unwrap_err().to_string();
    assert!(e.contains("8-bit exactly when"), "{e}");
}

/// A document that is not one, a shape that is not a matrix, an absmax of the wrong length: each is its own refusal.
#[test]
fn a_malformed_bitsandbytes_module_is_refused_with_its_reason() {
    let (f, vec) = bnb_vector("bnb_nf4", 0);
    let t = f.as_tensors().unwrap();
    let params = f.read_config(&vec["config"]).expect("config").params;
    let good = vector_roles(&f, &vec);
    let q = t.roles().position(|(n, ..)| n == "qstate").unwrap();
    let doc = |r: &mut Vec<Option<RoleTensor>>, text: &str| {
        r[q] = Some(RoleTensor { shape: vec![text.len()], dtype: "U8".into(), data: text.as_bytes().to_vec() });
    };
    let cases: Vec<(&str, String, &str)> = vec![
        ("not JSON", "this is not json".into(), "not a JSON document"),
        ("a truncated document", r#"{"quant_type": "nf4", "blocksize": 64"#.into(), "not a JSON document"),
        ("no shape", r#"{"quant_type": "nf4", "blocksize": 64, "dtype": "float16"}"#.into(), "has no `shape[0]`"),
        ("a 3-D shape", r#"{"quant_type": "nf4", "blocksize": 64, "dtype": "float16", "shape": [8, 4, 8]}"#.into(), "not a matrix"),
        ("a dtype nobody uses", r#"{"quant_type": "nf4", "blocksize": 64, "dtype": "int4", "shape": [8, 32]}"#.into(), "int4"),
        ("a shape that does not match the tensors", r#"{"quant_type": "nf4", "blocksize": 64, "dtype": "float16", "shape": [8, 33]}"#.into(), "packed weight is not"),
        ("a block size that does not match absmax", r#"{"quant_type": "nf4", "blocksize": 32, "dtype": "float16", "shape": [8, 32]}"#.into(), "one per block"),
        ("a float block size", r#"{"quant_type": "nf4", "blocksize": 64.5, "dtype": "float16", "shape": [8, 32]}"#.into(), "not an integer"),
    ];
    for (what, text, needle) in cases {
        let mut r = good.clone();
        doc(&mut r, &text);
        let e = t.decode_floats(&r, &params).unwrap_err().to_string();
        assert!(e.contains(needle), "{what}: `{e}` does not mention `{needle}`");
    }
}

/// An int8 weight stored in a hardware-reordered layout is not row-major: refused; `weight_format` 0 and an absent one are read.
#[test]
fn a_reordered_int8_layout_is_refused_and_row_major_is_read() {
    let (f, vec) = bnb_vector("bnb_int8", 0);
    let t = f.as_tensors().unwrap();
    let params = f.read_config(&vec["config"]).expect("config").params;
    let good = vector_roles(&f, &vec);
    let w = t.decode_integers(&good, &params).expect("row-major");
    let w_idx = t.roles().position(|(n, ..)| n == "wfmt").unwrap();
    let mut r = good.clone();
    r[w_idx] = None;
    let w2 = t.decode_integers(&r, &params).expect("older bitsandbytes does not write weight_format");
    assert_eq!(w.q, w2.q);
    for fmt in [1u8, 2, 3] {
        let mut r = good.clone();
        r[w_idx] = Some(RoleTensor { shape: vec![], dtype: "U8".into(), data: vec![fmt] });
        let e = t.decode_integers(&r, &params).unwrap_err().to_string();
        assert!(e.contains("hardware-reordered layout") && e.contains("col32"), "{e}");
    }
    // The stored integers are the integers, whatever their values: the scale is SCB / 127.
    assert_eq!((w.out, w.inp), (6, 16));
    assert!(w.q.iter().all(|q| (-128..=127).contains(q)));
    assert_eq!(w.scale[0], 0.0, "an all-zero row has SCB 0");
}
