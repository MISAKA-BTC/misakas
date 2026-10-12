//! **`ATTN_DIFFERENTIAL_V1`** (FR-04, DiffLlama): the float reference equals transformers' `DiffLlamaForCausalLM`, the integer program
//! follows it, the court reproduces every node of the program, and a differential attention that the lowering would not model is
//! refused by name.
//!
//! The integer-versus-float numbers are `tests/fidelity_tiny.rs::diffllama` and the three implementations' identity is
//! `tests/three_way.rs` (it walks every fixture). Here: the equality with the transformers class on the same weights, λ as a
//! constant of the weights (the two `[1]` params of each layer), the court, and the refusals.

mod common;

use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::spec::Mixer;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{hf_weights, hl, parse_config_str};
use std::path::Path;
use std::sync::Arc;

fn dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf/diffllama")
}

/// The float reference against transformers' logits on the fixture's weights: the two-call differential attention (one softmax per
/// head, the value halves repeated, the heads split, lambda, the group norm) is what the lowering's decomposition computes.
#[test]
fn diffllama_matches_its_hf_fixture() {
    let cfg = std::fs::read_to_string(dir().join("config.json")).expect("config");
    let spec = parse_config_str(&cfg).expect("diffllama reads");
    let prog = hl::build_program(&spec).expect("hl");
    let binding = hf_weights::bind(&spec, &prog).expect("bind");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, unused) = ParamStore::from_source(&prog, &binding, &ck).expect("params");
    assert!(unused.is_empty(), "every checkpoint tensor is read: {unused:?}");
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir().join("logits.json")).unwrap()).unwrap();
    let tokens: Vec<usize> = meta["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let got = Session::new(&prog, &params).run(&tokens).expect("run");
    let want = meta["logits_full"].as_array().unwrap();
    let mut worst = 0f64;
    let mut scale = 1f64;
    for (g, w) in got.iter().zip(want) {
        for (a, b) in g.iter().zip(w.as_array().unwrap()) {
            let b = b.as_f64().unwrap();
            worst = worst.max((*a as f64 - b).abs());
            scale = scale.max(b.abs());
        }
    }
    assert!(worst <= 1e-4 * scale, "max |dlogit| {worst:e} against transformers (scale {scale})");
}

/// λ and `1 − λ_init` are `[1]` constants made at conversion, one per layer, and λ_init follows the layer: layer 0 reads 0.2, layer 1
/// reads `0.8 − 0.6·e^{−0.3}`.
#[test]
fn lambda_is_a_constant_of_the_layers_own_weights() {
    let cfg = std::fs::read_to_string(dir().join("config.json")).expect("config");
    let spec = parse_config_str(&cfg).expect("diffllama reads");
    let prog = hl::build_program(&spec).expect("hl");
    let binding = hf_weights::bind(&spec, &prog).expect("bind");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, _) = ParamStore::from_source(&prog, &binding, &ck).expect("params");
    let find = |name: &str| prog.params.iter().position(|p| p.name == name).unwrap_or_else(|| panic!("param {name}"));
    let (lam, sc) = (find("attn.diff.lambda"), find("attn.diff.scale"));
    assert!(prog.params[lam].per_layer && prog.params[lam].shape == vec![1]);
    for (layer, init) in [(0usize, 0.2f64), (1, 0.8 - 0.6 * (-0.3f64).exp())] {
        let s = params.get(sc as u32, Some(layer)).expect("scale").data[0] as f64;
        assert!((s - (1.0 - init)).abs() < 1e-6, "layer {layer}: 1 - lambda_init = {s}");
        let l = params.get(lam as u32, Some(layer)).expect("lambda").data[0] as f64;
        // exp(x1) - exp(x2) + init with |x| tiny on this fixture (lambda_std_dev 0.1): lambda sits within a few hundredths of init.
        assert!((l - init).abs() < 0.2, "layer {layer}: lambda {l} vs lambda_init {init}");
    }
}

/// The court reproduces every node of every occurrence of the lowered program from the committed leaves.
#[test]
fn the_court_reproduces_every_node_of_the_differential_program() {
    let cfg = std::fs::read_to_string(dir().join("config.json")).expect("config");
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).expect("prepare");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).expect("params");
    let loader = Resident(Arc::new(params));
    let calib = fidelity::random_sequences(prep.hl.vocab, 4, 24, 11);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrate");
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let tokens: Vec<u32> = fidelity::random_sequences(prep.hl.vocab, 1, 12, 5)[0].iter().map(|t| *t as u32).collect();
    let p = &prep.lowered.program;
    let last = tokens.len() as u32 - 1;
    let r = common::court_coverage(p, &mat.params, &tokens, &[0, 3, last], &[4]).unwrap_or_else(|e| panic!("{e}"));
    let used: std::collections::BTreeSet<&str> = p.blocks.iter().flat_map(|b| b.nodes.iter().map(|n| n.prim.name())).collect();
    for u in &used {
        assert!(r.primitives.get(u).copied().unwrap_or(0) > 0, "primitive {u} is used but the court never evaluated it");
    }
    assert!(r.commits > 0);
}

/// A differential attention the lowering does not model is refused by name: an odd number of kv heads (HF refuses it too), and
/// values of another width than the keys.
#[test]
fn a_differential_attention_it_does_not_model_is_refused_by_name() {
    let cfg = std::fs::read_to_string(dir().join("config.json")).expect("config");
    let mut spec = parse_config_str(&cfg).expect("diffllama reads");
    for l in &mut spec.layers {
        if let Mixer::Attention(a) = &mut l.mixer {
            a.v_head_dim = a.head_dim + 1;
        }
    }
    let e = hl::build_program(&spec).err().expect("refused").to_string();
    assert!(e.contains("ATTN_DIFFERENTIAL_V1"), "{e}");
    let mut odd = cfg.replace("\"num_key_value_heads\": 2", "\"num_key_value_heads\": 1");
    odd = odd.replace("\"num_attention_heads\": 4", "\"num_attention_heads\": 4");
    let e = parse_config_str(&odd).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("even"), "{e}");
}
