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
    // 5. The class's one-stage pipeline: the image bound by JobImage (rfc3/impl 4bd8f3b2b). It
    // must give the standalone program's bytes.
    let pipe = encoder::vision_pipeline();
    let info = tir::pipeline::validate_pipeline(&pipe, std::slice::from_ref(&p2)).expect("pipeline normal form");
    assert_eq!(info.images, vec![[s.h, s.w]], "{name}: one image slot of the tower's size");
    struct One<'a>(&'a dyn tir::ParamSource);
    impl tir::pipeline::PipelineParams for One<'_> {
        fn params(&self, _: u16) -> &dyn tir::ParamSource {
            self.0
        }
    }
    struct NoRandom;
    impl tir::pipeline::RandomSource for NoRandom {
        fn random(&self, _: u16, _: tir::program_v2::RandomDist, _: u32, _: &[u32]) -> Option<tir::Tensor> {
            None
        }
    }
    for (img, _) in &fx.images {
        let job = tir::pipeline::PipelineJob {
            images: vec![tir::pipeline::JobImageV1 { h: s.h, w: s.w, rgb: img.clone() }],
            ..Default::default()
        };
        let pr = tir::pipeline::run_pipeline(&pipe, std::slice::from_ref(&p2), &One(&params2), &NoRandom, &job).expect("pipeline run");
        let mut inputs = tir::interp_v2::MapInputs::default();
        let t = tir::Tensor::new(tir::DType::I16, vec![s.h as usize, s.w as usize, 3], img.iter().map(|v| *v as i128).collect()).unwrap();
        inputs.constant.insert(0, t);
        let direct = interp.run_positions(&params2, &inputs, 1).expect("v2 run").remove(0).output;
        assert_eq!(pr.output, direct, "{name}: the pipeline differs from the program");
    }
    let pa = tir::admit_v2::tir_admit_pipeline_v1(
        &pipe.encode(),
        &[p2.encode()],
        &misaka_palw_tir_lower::admission::default_inputs(),
        &tir::admit_v2::TirJobCeilingsV1::open_v1(),
    )
    .expect("tir_admit_pipeline_v1");
    eprintln!("{name} pipeline (JobImage) admitted: job {:?}, {} step leaves", pa.job_cost, pa.job_step_leaves);
    // 6. Admission of the program alone.
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

/// **The tiny VLM, image + text → logits, off-chain.** LLaVA's tower and projector run as their own
/// version-2 program. Their integer rows are handed, in the harness, to the LM's `input.image_rows`
/// with `input.image_start` = 2, and the LM (a version-2 program with a `Logits` output) runs over
/// the prompt with the image's placeholder tokens. The logits are held against
/// `LlavaForConditionalGeneration`'s.
///
/// In a class, JobImage (rfc3/impl, in progress) binds the image, and the LM stage belongs to the
/// FP job version RFC-0003 II.2 plans. A pipeline cannot carry a `Logits` stage today.
#[test]
fn llava_image_and_text_to_logits_matches_hf_off_chain() {
    use misaka_palw_tir_lower::lower::{ImageRows, LowerOpts, IMAGE_ROWS_PARAM, IMAGE_START_PARAM};
    let vis = check_tower("llava", "image_rows");
    let fx = load("llava");
    let dir = fixture_dir("llava");
    let root: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap()).unwrap();
    // The LM: LLaVA's text config as the Llama decoder it is, over the checkpoint's language_model.*.
    let mut tc = root["text_config"].clone();
    tc["architectures"] = serde_json::json!(["LlamaForCausalLM"]);
    let mut spec = misaka_palw_tir_lower::parse_config_str(&tc.to_string()).expect("text spec");
    spec.hf.prefix_aliases.push(("model.".into(), "language_model.model.".into()));
    spec.hf.prefix_aliases.push(("lm_head".into(), "language_model.lm_head".into()));
    spec.hf.ignored_prefixes.extend(["vision_tower.".to_string(), "multi_modal_projector.".to_string()]);
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("hl");
    let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).expect("bind");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    let rows = vis.program.blocks[vis.program.schedule.post as usize].nodes[vis.program.output.node() as usize].out.shape.clone();
    let (n, width) = match rows.as_slice() {
        [tir::Dim::Fixed(n), tir::Dim::Fixed(w)] => (*n as usize, *w as usize),
        s => panic!("image rows of shape {s:?}"),
    };
    let rec = &fx.images[0].1;
    let ids: Vec<usize> = rec["input_ids"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let start = rec["image_start"].as_u64().unwrap() as usize;
    let hf_rows = rows_of(&rec["image_rows"]);
    let hf_logits = rows_of(&rec["logits"]);
    let overrides = |r: &[Vec<f64>]| -> std::collections::BTreeMap<usize, Vec<f32>> {
        r.iter().enumerate().map(|(i, row)| (start + i, row.iter().map(|x| *x as f32).collect())).collect()
    };
    // 1. The float LM with HF's image rows in place is HF's VLM.
    let mut sess = misaka_palw_tir_lower::float_ref::Session::new(&hl, &params_f);
    sess.overrides = overrides(&hf_rows);
    let fl = sess.run(&ids).expect("float LM");
    let r = rel(&fl.concat().iter().map(|x| *x as f64).collect::<Vec<_>>(), &hf_logits.concat());
    eprintln!("llava float LM with HF's image rows vs HF logits: rel {r:.2e}");
    assert!(r < 1e-4, "float LM vs HF: rel {r}");
    // 2. Calibrate the LM: the prompt with the float tower's rows for random images, and plain text.
    let (vhl, vbind) = vision::hl_program(&fx.spec).expect("tower hl");
    let (vparams, _) = ParamStore::from_source(&vhl, &vbind, &ck).expect("tower params");
    let mut stats: std::collections::BTreeMap<String, misaka_palw_tir_lower::float_ref::SiteStat> = Default::default();
    let mut run_stats = |ov: std::collections::BTreeMap<usize, Vec<f32>>, toks: &[usize]| {
        let mut s = misaka_palw_tir_lower::float_ref::Session::new(&hl, &params_f).with_site_stats();
        s.overrides = ov;
        s.run(toks).expect("calibration run");
        for (k, v) in s.sites.take().unwrap_or_default() {
            stats.entry(k).or_default().merge(&v);
        }
    };
    for img in calib_images(&fx.spec, 4) {
        let r = vision::float_forward(&vhl, &fx.spec, &vparams, &img, None).expect("tower float");
        run_stats(overrides(&r), &ids);
    }
    for seq in misaka_palw_tir_lower::fidelity::random_sequences(62, 4, ids.len(), 5) {
        run_stats(Default::default(), &seq);
    }
    // 3. The integer LM with the image-row placement, as a version-2 program.
    let opts = LowerOpts { max_window: Some(64), image_rows: Some(ImageRows { rows: n, width, unit: vis.out_scale }), ..LowerOpts::default() };
    let lw = misaka_palw_tir_lower::lower::lower(&hl, &opts).expect("lower LM");
    let loader = Resident(Arc::new(params_f));
    let quiet = |_: usize, _: usize| {};
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise LM");
    let lm2 = encoder::lift_v2(
        &lw,
        &[
            (IMAGE_ROWS_PARAM, tir::program_v2::InputSource::External { lo: i32::MIN as i64, hi: i32::MAX as i64 }),
            (IMAGE_START_PARAM, tir::program_v2::InputSource::External { lo: 0, hi: 64 }),
        ],
        tir::program_v2::OutputDecl::Logits { node: lw.program.logits, scheme_id: [0u8; 64] },
    )
    .expect("LM v2");
    let lm_params = encoder::lifted_params(&lw.program, &[IMAGE_ROWS_PARAM, IMAGE_START_PARAM], &mat.params);
    // 4. Off-chain chaining: the tower's integer rows into the LM.
    let (img, _) = &fx.images[0];
    let vint = tir::interp_v2::InterpreterV2::new(&vis.program).expect("tower v2");
    let mut vin = tir::interp_v2::MapInputs::default();
    vin.constant.insert(0, tir::Tensor::new(tir::DType::I16, vec![fx.spec.h as usize, fx.spec.w as usize, 3], img.iter().map(|v| *v as i128).collect()).unwrap());
    let vout = vint.run_positions(&vis.params, &vin, 1).expect("tower run").remove(0).output;
    let lint = tir::interp_v2::InterpreterV2::new(&lm2).expect("LM v2 interpreter");
    let mut lin = tir::interp_v2::MapInputs::default();
    lin.constant.insert(0, tir::Tensor::new(tir::DType::I32, vec![n, width], vout.data.clone()).unwrap());
    lin.constant.insert(1, tir::Tensor::scalar(tir::DType::Idx, start as i128).unwrap());
    let toks: Vec<u32> = ids.iter().map(|t| *t as u32).collect();
    let steps = lint.run(&lm_params, &lin, &toks).expect("LM run");
    // 5. Against HF.
    let (mut agree, mut kl) = (0usize, 0f64);
    for (st, want) in steps.iter().zip(&hf_logits) {
        let got: Vec<f64> = st.output.data.iter().map(|c| *c as f64 * mat.logits_scale).collect();
        let lse = |v: &[f64]| {
            let m = v.iter().cloned().fold(f64::MIN, f64::max);
            m + v.iter().map(|x| (x - m).exp()).sum::<f64>().ln()
        };
        let (lp, lq) = (lse(want), lse(&got));
        kl += want.iter().zip(&got).map(|(p, q)| (p - lp).exp() * ((p - lp) - (q - lq))).sum::<f64>();
        let am = |v: &[f64]| v.iter().enumerate().fold((0, f64::MIN), |b, (i, x)| if *x > b.1 { (i, *x) } else { b }).0;
        agree += usize::from(am(&got) == am(want));
    }
    let (t, kl) = (steps.len(), kl / steps.len() as f64);
    eprintln!("llava image+text → logits (integer tower → integer LM) vs HF: top-1 {agree}/{t}, mean KL {kl:.5}");
    assert!(agree as f64 / t as f64 >= 0.85 && kl < 0.05, "top-1 {agree}/{t}, KL {kl}");
    tir::admit_v2::tir_admit_program_v2(&lm2, &misaka_palw_tir_lower::admission::default_inputs()).expect("LM tir_admit_v2");
}

/// The fixed-ratio downscale stage: a 56×56 canonical image through the box filter of ratio 2
/// (two `MatMul`s with pinned 0/1 matrices and an exact rounded `Div`) into CLIP's 28×28 tower.
/// Each fixture pixel is replicated into a 2×2 block, so the exact downscale is the fixture image,
/// and the integer output must match HF's output on that image.
#[test]
fn a_fixed_ratio_downscale_stage_feeds_the_tower_exactly() {
    let mut fx = load("clip_vision");
    let s = {
        let mut s = fx.spec.clone();
        s.h *= 2;
        s.w *= 2;
        s.downscale = 2;
        s
    };
    let dir = fixture_dir("clip_vision");
    let (hl, binding) = vision::hl_program(&s).expect("hl");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    let up = |img: &[u8]| -> Vec<u8> {
        let (h, w) = (fx.spec.h as usize, fx.spec.w as usize);
        let mut o = vec![0u8; 4 * h * w * 3];
        for i in 0..2 * h {
            for j in 0..2 * w {
                for c in 0..3 {
                    o[(i * 2 * w + j) * 3 + c] = img[((i / 2) * w + j / 2) * 3 + c];
                }
            }
        }
        o
    };
    let mut stats = std::collections::BTreeMap::new();
    for img in calib_images(&s, 6) {
        vision::float_forward(&hl, &s, &params_f, &img, Some(&mut stats)).expect("calibration");
    }
    let lw = vision::lower_vision(&hl, &s).expect("lower");
    let loader = Resident(Arc::new(params_f));
    let quiet = |_: usize, _: usize| {};
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let p2 = encoder::vision_v2(&lw).expect("v2");
    let params2 = encoder::lifted_params(&lw.program, &[vision::IMAGE_PARAM], &mat.params);
    let interp = tir::interp_v2::InterpreterV2::new(&p2).expect("interpreter v2");
    for (img, out) in std::mem::take(&mut fx.images) {
        let big = up(&img);
        let mut inputs = tir::interp_v2::MapInputs::default();
        inputs.constant.insert(0, tir::Tensor::new(tir::DType::I16, vec![s.h as usize, s.w as usize, 3], big.iter().map(|v| *v as i128).collect()).unwrap());
        let o = interp.run_positions(&params2, &inputs, 1).expect("run").remove(0).output;
        let got: Vec<f64> = o.data.iter().map(|c| *c as f64 * mat.logits_scale).collect();
        let want = rows_of(&out["image_embeds"]).concat();
        let c = cosine(&got, &want);
        eprintln!("clip_vision behind a ×2 box downscale vs HF on the fixture image: cosine {c:.6}");
        assert!(c > 0.999, "cosine {c}");
    }
    tir::admit_v2::tir_admit_program_v2(&p2, &misaka_palw_tir_lower::admission::default_inputs()).expect("tir_admit_v2");
}
