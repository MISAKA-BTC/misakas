//! **`VLM_CROSS_LAYERS_SKIPPED_V1`** and **`ATTN_CROSS_V1`** (FR-21, Mllama): the text-only stage. `MllamaTextModel.forward` skips
//! every cross-attention layer when no vision states are bound, so the float reference over the Llama layers equals transformers'
//! text-only logits; the cross layers stay in the spec (and the model's layer numbering), their tensors are dormant, and image rows
//! bound to such a spec are refused by name. The integer-versus-float numbers are `tests/fidelity_tiny.rs::mllama`.

use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::{ImageRows, LowerOpts};
use misaka_palw_tir_lower::spec::Mixer;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{fidelity, hf_weights, hl, parse_config_str};
use std::path::Path;

fn dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf/mllama")
}

#[test]
fn mllama_text_only_matches_its_hf_fixture() {
    let cfg = std::fs::read_to_string(dir().join("config.json")).expect("config");
    let spec = parse_config_str(&cfg).expect("mllama reads");
    // The cross layer is in the spec and not in the program.
    assert_eq!(spec.layers.iter().filter(|l| matches!(l.mixer, Mixer::CrossAttention(_))).count(), 1);
    let prog = hl::build_program(&spec).expect("hl");
    assert_eq!(prog.schedule.len(), spec.layers.len() - 1);
    assert_eq!(prog.layer_of, vec![0], "the surviving layer keeps its model index");
    let binding = hf_weights::bind(&spec, &prog).expect("bind");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, unused) = ParamStore::from_source(&prog, &binding, &ck).expect("params");
    assert!(unused.is_empty(), "the cross layer, the tower and the projector are dormant by prefix: {unused:?}");
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir().join("logits.json")).unwrap()).unwrap();
    let tokens: Vec<usize> = meta["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let got = Session::new(&prog, &params).run(&tokens).expect("run");
    let want = meta["logits_full"].as_array().unwrap();
    let (mut worst, mut scale) = (0f64, 1f64);
    for (g, w) in got.iter().zip(want) {
        for (a, b) in g.iter().zip(w.as_array().unwrap()) {
            let b = b.as_f64().unwrap();
            worst = worst.max((*a as f64 - b).abs());
            scale = scale.max(b.abs());
        }
    }
    assert!(worst <= 1e-4 * scale, "max |dlogit| {worst:e} against transformers (scale {scale})");
}

#[test]
fn binding_image_rows_to_a_cross_attention_spec_is_refused_by_name() {
    let cfg = std::fs::read_to_string(dir().join("config.json")).expect("config");
    let spec = parse_config_str(&cfg).expect("mllama reads");
    let opts = LowerOpts { image_rows: Some(ImageRows { rows: 4, width: 32, unit: 1.0 / 4096.0, placeholder: 63, mrope: None }), ..LowerOpts::default() };
    let e = fidelity::prepare_spec(spec, &opts).err().expect("refused").to_string();
    assert!(e.contains("ATTN_CROSS_V1"), "{e}");
}
