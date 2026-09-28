//! **RFC-0003 Part II.4: vision towers against their Hugging Face fixtures.** Each tiny tower
//! (`tools/gen_hf_vision_fixtures.py`) is parsed, bound to its checkpoint, and checked in five
//! steps:
//! 1. its float reference against HF on the fixture's canonical images;
//! 2. calibration on other random images;
//! 3. lowering to the integer program, whose preprocessing, patch projection, layers and output
//!    run in one position;
//! 4. running as a version-2 program on `InterpreterV2`, the image a `u8` HWC input;
//! 5. admission.
//! The integer output is held against HF's by cosine (fidelity, not validity).

use misaka_palw_tir as tir;
use misaka_palw_tir_lower::encoder;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::vision::{self, VisionSpec};
use misaka_palw_tir_lower::lower::materialise;
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-vis").join(name)
}

pub struct Fixture {
    pub spec: VisionSpec,
    pub images: Vec<(Vec<u8>, serde_json::Value)>,
}

pub fn load(name: &str) -> Fixture {
    let dir = fixture_dir(name);
    let cfg = std::fs::read_to_string(dir.join("config.json")).expect("config");
    let o: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).expect("outputs")).expect("json");
    let arr = |v: &serde_json::Value| -> [f64; 3] { [v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap()] };
    let size = (o["size"][0].as_u64().unwrap() as u32, o["size"][1].as_u64().unwrap() as u32);
    let spec = vision::parse_vision(&cfg, Some(size), Some((arr(&o["mean"]), arr(&o["std"])))).expect("parse");
    let images = o["images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["hwc"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect(), r["outputs"].clone()))
        .collect();
    Fixture { spec, images }
}

fn rows_of(v: &serde_json::Value) -> Vec<Vec<f64>> {
    match v.as_array() {
        Some(a) if a.first().is_some_and(|x| x.is_array()) => a.iter().map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()).collect(),
        Some(a) => vec![a.iter().map(|x| x.as_f64().unwrap()).collect()],
        None => panic!("not an array"),
    }
}

fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let d: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    d / (a.iter().map(|x| x * x).sum::<f64>().sqrt() * b.iter().map(|x| x * x).sum::<f64>().sqrt())
}

fn rel(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt() / b.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Random canonical images for calibration (none of the fixture's).
fn calib_images(s: &VisionSpec, n: usize) -> Vec<Vec<u8>> {
    use rand::{Rng, SeedableRng};
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(99);
    (0..n).map(|_| (0..(s.h * s.w * 3) as usize).map(|_| rng.gen_range(0..=255u8)).collect()).collect()
}

/// Everything a tower's check produces: the version-2 program, its params, and the output's unit.
pub struct Built {
    pub program: tir::program_v2::TirProgramV2,
    pub params: misaka_palw_tir_lower::lower::IntParams,
    pub out_scale: f64,
}

pub fn check_tower(name: &str, key: &str) -> Built {
    let fx = load(name);
    let s = &fx.spec;
    let dir = fixture_dir(name);
    let (hl, binding) = vision::hl_program(s).expect("hl");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    // 1. The float reference is the HF tower.
    for (img, out) in &fx.images {
        let got = vision::float_forward(&hl, s, &params_f, img, None).expect("float");
        let want = rows_of(&out[key]);
        assert_eq!(got.len(), want.len(), "{name}: rows");
        let r = rel(&got.concat(), &want.concat());
        eprintln!("{name} float reference vs HF `{key}`: rel {r:.2e} ({} rows)", got.len());
        assert!(r < 1e-4, "{name}: float vs HF rel {r}");
    }
    // 2. Calibration.
    let mut stats = std::collections::BTreeMap::new();
    for img in calib_images(s, 6) {
        vision::float_forward(&hl, s, &params_f, &img, Some(&mut stats)).expect("calibration");
    }
    // 3. The integer program.
    let lw = vision::lower_vision(&hl, s).expect("lower");
    let loader = Resident(Arc::new(params_f));
    let quiet = |_: usize, _: usize| {};
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    // 4. Version 2 on InterpreterV2.
    let p2 = encoder::vision_v2(&lw).expect("v2");
    let params2 = encoder::lifted_params(&lw.program, &[vision::IMAGE_PARAM], &mat.params);
    let interp = tir::interp_v2::InterpreterV2::new(&p2).expect("interpreter v2");
    for (img, out) in &fx.images {
        let mut inputs = tir::interp_v2::MapInputs::default();
        let t = tir::Tensor::new(tir::DType::I16, vec![s.h as usize, s.w as usize, 3], img.iter().map(|v| *v as i128).collect()).unwrap();
        inputs.constant.insert(0, t);
        let run = interp.run_positions(&params2, &inputs, 1).expect("v2 run");
        let o = &run[0].output;
        let width = s.out_width();
        let got: Vec<Vec<f64>> = o.data.chunks(width).map(|r| r.iter().map(|c| *c as f64 * mat.logits_scale).collect()).collect();
        let want = rows_of(&out[key]);
        assert_eq!(got.len(), want.len(), "{name}: integer rows");
        let cos: Vec<f64> = got.iter().zip(&want).map(|(a, b)| cosine(a, b)).collect();
        let mean = cos.iter().sum::<f64>() / cos.len() as f64;
        let min = cos.iter().cloned().fold(1.0, f64::min);
        eprintln!("{name} integer vs HF `{key}`: cosine mean {mean:.6} min {min:.6}, rel {:.2e} ({} rows × {width})", rel(&got.concat(), &want.concat()), got.len());
        assert!(min > 0.999, "{name}: cosine min {min}");
    }
    // 5. Admission.
    let a = tir::admit_v2::tir_admit_program_v2(&p2, &misaka_palw_tir_lower::admission::default_inputs()).expect("tir_admit_v2");
    eprintln!(
        "{name} admitted: {} nodes, {} cones, {} params, position {:?}",
        p2.blocks.iter().map(|b| b.nodes.len()).sum::<usize>(),
        a.view.cones.len(),
        p2.params.len(),
        a.view.position
    );
    Built { program: p2, params: params2, out_scale: mat.logits_scale }
}

#[test]
fn clip_vision_with_projection_matches_its_hf_fixture() {
    check_tower("clip_vision", "image_embeds");
}

#[test]
fn siglip_vision_with_its_pooling_head_matches_its_hf_fixture() {
    check_tower("siglip_vision", "pooler_output");
}

#[test]
fn qwen2_vl_vision_with_its_merger_matches_its_hf_fixture() {
    check_tower("qwen2_vl_vision", "merged");
}

#[test]
fn qwen2_5_vl_vision_with_windows_and_merger_matches_its_hf_fixture() {
    check_tower("qwen2_5_vl_vision", "merged");
}

#[test]
fn llava_tower_and_projector_match_their_hf_fixture() {
    check_tower("llava", "image_rows");
}
