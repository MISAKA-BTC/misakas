//! **Freeze criterion 4 on the HF-lowered programs**: for every one of the 84 tiny-fixture
//! architectures (and the 11 pre-quantised GPTQ/AWQ fixtures), the lowered program with its
//! calibrated integer params is run on
//!
//! * the reference evaluator (`misaka-palw-tir`),
//! * the independent second implementation (`misaka-palw-tir-ref2`, written from 04b alone, fed
//!   the canonical bytes — it decodes them with its own codec), and
//! * the typed backend that ships on nodes (`misaka-palw-tir-exec`),
//!
//! and at every position the logits and every commit point (slot, block, layer, node, value)
//! must be equal across all three, byte for byte.

mod common;

use common::three_ways;
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use std::path::Path;
use std::sync::Arc;

/// Positions run on all three for one fixture, or why it could not be prepared.
fn run(name: &str) -> Result<usize, String> {
    run_in("tests/fixtures/hf", name)
}

fn run_in(root: &str, name: &str) -> Result<usize, String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(root).join(name);
    // A Hugging Face directory or a GGUF one (`model.gguf`).
    let (prep, ck) = fidelity::open_model(&dir, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, ck.as_ref()).map_err(|e| e.to_string())?;
    let loader = Resident(Arc::new(params));
    let max_len = prep.spec.embedding.positions.as_ref().map_or(usize::MAX, |p| p.rows - p.offset);
    let calib = fidelity::random_sequences(prep.hl.vocab, 4, 24.min(max_len), 11);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).map_err(|e| format!("calibrate: {e}"))?;
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet)
        .map_err(|e| format!("materialise: {e}"))?;
    let eval = fidelity::random_sequences(prep.hl.vocab, 2, 16.min(max_len), 97);
    three_ways(&prep.lowered.program, &mat.params, &eval)
}

#[test]
fn every_hf_tiny_fixture_program_is_the_same_on_all_three_implementations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut names: Vec<String> =
        std::fs::read_dir(&root).expect("fixtures").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    names.sort();
    assert_eq!(names.len(), 84);
    let mut failed = Vec::new();
    let mut total = 0;
    for n in &names {
        let has_weights = root.join(n).join("model.safetensors").exists();
        if !has_weights {
            eprintln!("{n:>22}: SKIPPED (no weights)");
            continue;
        }
        match run(n) {
            Ok(k) => {
                total += k;
                eprintln!("{n:>22}: {k} positions, logits and every commit equal on reference, ref2 and exec");
            }
            Err(e) => {
                eprintln!("{n:>22}: {e}");
                failed.push(n.clone());
            }
        }
    }
    eprintln!("{total} positions in all");
    assert!(failed.is_empty(), "{failed:?}");
}

/// The pre-quantised fixtures (`tests/quantized.rs`, `tests/gguf.rs`): the grouped integer MatMul,
/// the per-group scale products and sums, the zero-point and minimum terms, the outlier mask and the
/// column-order gather are the same bytes on all three implementations.
#[test]
fn every_prequantised_fixture_is_the_same_on_all_three_implementations() {
    let mut names: Vec<(String, String)> = Vec::new();
    for (root, count) in [("tests/fixtures/hf-quant", 13), ("tests/fixtures/gguf", 11)] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(root);
        let mut here: Vec<String> =
            std::fs::read_dir(&dir).expect("fixtures").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
        here.sort();
        assert_eq!(here.len(), count, "{root}");
        names.extend(here.into_iter().map(|n| (root.to_string(), n)));
    }
    let mut failed = Vec::new();
    for (root, n) in &names {
        match run_in(root, n) {
            Ok(k) => eprintln!("{n:>22}: {k} positions, logits and every commit equal on reference, ref2 and exec"),
            Err(e) => {
                eprintln!("{n:>22}: {e}");
                failed.push(n.clone());
            }
        }
    }
    assert!(failed.is_empty(), "{failed:?}");
}

/// The LoRA candidates (`tests/lora.rs`) on all three implementations: the adapter path's
/// `i32 × i16` and `i16 × i16` products, the exact rational scale (`Mul`, rounded `Div`) and the
/// added narrowing are the same bytes everywhere.
#[test]
fn every_lora_candidate_is_the_same_on_all_three_implementations() {
    use misaka_palw_tir_lower::weights::Overlay;
    use misaka_palw_tir_lower::{hf_weights, hl, lora, lower};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut names: Vec<String> =
        std::fs::read_dir(root.join("hf-lora")).expect("fixtures").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    names.sort();
    assert!(names.len() >= 4);
    for name in names {
        let ad_dir = root.join("hf-lora").join(&name);
        let r: serde_json::Value = serde_json::from_slice(&std::fs::read(ad_dir.join("logits.json")).unwrap()).unwrap();
        let base_dir = root.join("hf").join(r["base"].as_str().unwrap());
        let mut spec = misaka_palw_tir_lower::parse_config_str(&std::fs::read_to_string(base_dir.join("config.json")).unwrap()).unwrap();
        lora::attach(&mut spec, &std::fs::read_to_string(ad_dir.join("adapter_config.json")).unwrap()).unwrap();
        let hlp = hl::build_program(&spec).unwrap();
        let bind = hf_weights::bind(&spec, &hlp).unwrap();
        let (ck, ad) = (Checkpoint::open(&base_dir).unwrap(), Checkpoint::open(&ad_dir.join("adapter_model.safetensors")).unwrap());
        let (params, _) = ParamStore::from_source(&hlp, &bind, &Overlay { base: &ck, over: &ad }).unwrap();
        let loader = Resident(Arc::new(params));
        let quiet = |_: usize, _: usize| {};
        let stats = fidelity::calibrate(&hlp, &loader, &fidelity::random_sequences(hlp.vocab, 4, 24, 11), &quiet).unwrap();
        let mut lw = lower::lower(&hlp, &LowerOpts { max_window: Some(64), ..LowerOpts::default() }).unwrap();
        lower::adapter_params_last(&mut lw).unwrap();
        let mat = materialise(&lw, &hlp, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap();
        let eval = fidelity::random_sequences(hlp.vocab, 2, 16, 97);
        match three_ways(&lw.program, &mat.params, &eval) {
            Ok(n) => eprintln!("{name}: {n} positions equal on reference, ref2 and exec"),
            Err(e) => panic!("{name}: {e}"),
        }
    }
}
