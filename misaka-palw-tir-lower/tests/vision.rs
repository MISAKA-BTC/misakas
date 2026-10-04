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
//!
//! The whole tiny VLMs (LLaVA, Qwen2-VL, Qwen2.5-VL) then run as a two-stage text pipeline — the
//! tower over the job image, the LM as the text stage — against HF's greedy `generate`
//! ([`text_stage`]).

mod common;

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
    check_tower_with(name, key, None)
}

/// [`check_tower`] with the class's output choice overridden (a ViT's rows instead of its class embedding).
pub fn check_tower_with(name: &str, key: &str, out: Option<vision::VisionOut>) -> Built {
    let mut fx = load(name);
    if let Some(o) = out {
        fx.spec.out = o;
    }
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
    // 6. The three implementations and the court, on the tower's version-1 view (the image an input param).
    {
        let img = &fx.images[0].0;
        let t = misaka_palw_tir_lower::lower::IntTensor::i16(vec![s.h as usize, s.w as usize, 3], img.iter().map(|v| *v as i16).collect());
        let p6 = common::with_inputs(&lw.program, &mat.params, &[(vision::IMAGE_PARAM, t)]);
        let n3 = common::three_ways(&lw.program, &p6, &[vec![0]]).unwrap_or_else(|e| panic!("{name}: three implementations: {e}"));
        let c = common::court_coverage(&lw.program, &p6, &[0], &[0], &[1]).unwrap_or_else(|e| panic!("{name}: court: {e}"));
        eprintln!("{name} COURT: three implementations equal ({n3} position); the court replays {} commit points ({} nodes, {} elements) over {} primitives", c.commits, c.nodes, c.elements, c.primitives.len());
        assert!(c.commits > 0);
    }
    // 7. Admission of the program alone.
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

/// FR-19: a ViT from data only (no Rust reader of this family): the class embedding, and, as the class's other output
/// choice, every row through the final norm.
#[test]
fn vit_class_embedding_and_rows_match_their_hf_fixture() {
    check_tower("vit", "cls");
    check_tower_with("vit", "last_hidden_state", Some(vision::VisionOut::Rows));
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

/// Each program's params, by index (a two-stage pipeline's artifact).
struct ByProgram<'a>(Vec<&'a dyn tir::ParamSource>);
impl tir::pipeline::PipelineParams for ByProgram<'_> {
    fn params(&self, program: u16) -> &dyn tir::ParamSource {
        self.0[program as usize]
    }
}

/// No stage draws.
struct NoDraws;
impl tir::pipeline::RandomSource for NoDraws {
    fn random(&self, _: u16, _: tir::program_v2::RandomDist, _: u32, _: &[u32]) -> Option<tir::Tensor> {
        None
    }
}

/// The lowest id among the largest values: HF's greedy `argmax`.
fn argmax<T: PartialOrd + Copy>(v: &[T]) -> usize {
    v.iter().enumerate().fold(0, |b, (i, x)| if *x > v[b] { i } else { b })
}

/// `KL(p ‖ q)` of two logit rows.
fn kl(p: &[f64], q: &[f64]) -> f64 {
    let lse = |v: &[f64]| {
        let m = v.iter().cloned().fold(f64::MIN, f64::max);
        m + v.iter().map(|x| (x - m).exp()).sum::<f64>().ln()
    };
    let (lp, lq) = (lse(p), lse(q));
    p.iter().zip(q).map(|(a, b)| (a - lp).exp() * ((a - lp) - (b - lq))).sum()
}

/// The reference placement of [`ImageRows`] over a stream: the rows at the placeholder positions
/// while rows remain (the float LM's overrides), and — with M-RoPE — every position's `(t, h, w)`
/// as `get_rope_index` gives them for one image.
fn placed(
    stream: &[usize],
    img: &misaka_palw_tir_lower::lower::ImageRows,
    rows: &[Vec<f32>],
) -> (std::collections::BTreeMap<usize, Vec<f32>>, std::collections::BTreeMap<usize, [usize; 3]>) {
    let (mut ov, mut mp) = (std::collections::BTreeMap::new(), std::collections::BTreeMap::new());
    let mut c = 0usize;
    for (p, t) in stream.iter().enumerate() {
        let is_img = *t == img.placeholder as usize && c < img.rows;
        if let Some((gh, gw)) = img.mrope {
            let (gh, gw) = (gh as usize, gw as usize);
            let tri = if is_img {
                let s = p - c;
                [s, s + c / gw, s + c % gw]
            } else {
                [p - if c >= img.rows { img.rows - gh.max(gw) } else { 0 }; 3]
            };
            mp.insert(p, tri);
        }
        if is_img {
            ov.insert(p, rows[c].clone());
            c += 1;
        }
    }
    (ov, mp)
}

/// **A VLM's text stage, end to end** (RFC-0003 §II.2.1). The tower (checked by [`check_tower`]) is
/// stage 0 over the job's image; the LM, lowered with [`ImageRows`] — its rows placed at the prompt's
/// placeholder ids by a cursor, and for Qwen2-VL its M-RoPE positions from the same cursor — is the
/// text stage (a `Logits` program over `TextStream`) reading the tower's `Final` rows. Checked:
/// 1. the float LM with HF's image rows (and HF's M-RoPE positions) is HF's VLM over the stream;
/// 2. `run_text_pipeline` with a greedy selector generates HF's greedy ids (`generate`);
/// 3. replaying those ids (`run_pipeline`, `job.generated`) gives the generating run exactly;
/// 4. teacher-forced over HF's stream, the integer logits against HF's (top-1, KL);
/// 5. the two-stage pipeline's admission.
fn text_stage(name: &str, stream_key: &str) {
    use misaka_palw_tir_lower::lower::{ImageRows, LowerOpts, IMAGE_ROWS_PARAM};
    use tir::pipeline::{Binding, JobImageV1, PipelineJob, StageDecl, TextSelectV1, TirPipelineV1, TripRule};
    let vis = check_tower(name, "image_rows");
    let fx = load(name);
    let dir = fixture_dir(name);
    let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap();
    let root: serde_json::Value = serde_json::from_str(&cfg).unwrap();
    // The LM: the VLM's text config over the checkpoint's language-model names (`vlm_text`).
    let spec = misaka_palw_tir_lower::parse_config_str(&cfg).expect("text spec");
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("hl");
    let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).expect("bind");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    let rows = vis.program.blocks[vis.program.schedule.post as usize].nodes[vis.program.output.node() as usize].out.shape.clone();
    let (n, width) = match rows.as_slice() {
        [tir::Dim::Fixed(n), tir::Dim::Fixed(w)] => (*n as usize, *w as usize),
        s => panic!("image rows of shape {s:?}"),
    };
    let placeholder = root.get("image_token_id").or_else(|| root.get("image_token_index")).and_then(|v| v.as_u64()).expect("image token") as u32;
    // Qwen2-VL: the merged grid of the class's one image size.
    let mrope = fx.images[0].1.get("grid").map(|g| {
        let m = root["vision_config"]["spatial_merge_size"].as_u64().unwrap_or(2) as u32;
        (g[1].as_u64().unwrap() as u32 / m, g[2].as_u64().unwrap() as u32 / m)
    });
    if let Some((gh, gw)) = mrope {
        assert_eq!((gh * gw) as usize, n, "{name}: the merged grid is the tower's rows");
    }
    let img = ImageRows { rows: n, width, unit: vis.out_scale, placeholder, mrope };
    let ids_of = |v: &serde_json::Value| -> Vec<usize> { v.as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect() };
    let f32s = |r: &[Vec<f64>]| -> Vec<Vec<f32>> { r.iter().map(|row| row.iter().map(|x| *x as f32).collect()).collect() };
    // 1. The float LM with HF's image rows in place is HF's VLM, over the whole stream.
    for (_, rec) in &fx.images {
        let (ids, hf_gen) = (ids_of(&rec["input_ids"]), ids_of(&rec["generated"]));
        let stream: Vec<usize> = ids.iter().chain(&hf_gen[..hf_gen.len() - 1]).copied().collect();
        let (ov, mp) = placed(&stream, &img, &f32s(&rows_of(&rec["image_rows"])));
        assert_eq!(ov.len(), n, "{name}: every image row placed");
        if let Some(hp) = rec.get("mrope_positions") {
            let hp: Vec<Vec<usize>> = hp.as_array().unwrap().iter().map(ids_of).collect();
            for (p, tri) in &mp {
                assert_eq!(*tri, [hp[0][*p], hp[1][*p], hp[2][*p]], "{name}: M-RoPE position {p} differs from get_rope_index");
            }
        }
        let control = !mp.is_empty();
        let mut sess = misaka_palw_tir_lower::float_ref::Session::new(&hl, &params_f);
        sess.overrides = ov.clone();
        sess.mrope_pos = mp;
        let fl = sess.run(&stream).expect("float LM");
        let r = rel(&fl.concat().iter().map(|x| *x as f64).collect::<Vec<_>>(), &rows_of(&rec[stream_key]).concat());
        eprintln!("{name} float LM with HF's image rows vs HF over the stream ({} positions): rel {r:.2e}", stream.len());
        assert!(r < 1e-4, "{name}: float LM vs HF: rel {r}");
        // The control: the same LM with 1-D positions is not HF's — the M-RoPE positions matter.
        if control {
            let mut sess = misaka_palw_tir_lower::float_ref::Session::new(&hl, &params_f);
            sess.overrides = ov;
            let fl = sess.run(&stream).expect("float LM, 1-D positions");
            let r1 = rel(&fl.concat().iter().map(|x| *x as f64).collect::<Vec<_>>(), &rows_of(&rec[stream_key]).concat());
            eprintln!("{name} control: the float LM with 1-D positions vs HF: rel {r1:.2e}");
            assert!(r1 > 0.05, "{name}: the fixture barely exercises M-RoPE (rel {r1} with 1-D positions)");
        }
    }
    // 2. Calibration: the prompt with the float tower's rows for random images and a random
    // continuation, and plain text.
    let (ids0, gen0) = (ids_of(&fx.images[0].1["input_ids"]), ids_of(&fx.images[0].1["generated"]));
    let vocab = hl.vocab;
    let (vhl, vbind) = vision::hl_program(&fx.spec).expect("tower hl");
    let (vparams, _) = ParamStore::from_source(&vhl, &vbind, &ck).expect("tower params");
    let mut stats: std::collections::BTreeMap<String, misaka_palw_tir_lower::float_ref::SiteStat> = Default::default();
    let mut run_stats = |ov: std::collections::BTreeMap<usize, Vec<f32>>, mp: std::collections::BTreeMap<usize, [usize; 3]>, toks: &[usize]| {
        let mut s = misaka_palw_tir_lower::float_ref::Session::new(&hl, &params_f).with_site_stats();
        s.overrides = ov;
        s.mrope_pos = mp;
        s.run(toks).expect("calibration run");
        for (k, v) in s.sites.take().unwrap_or_default() {
            stats.entry(k).or_default().merge(&v);
        }
    };
    let conts = misaka_palw_tir_lower::fidelity::random_sequences(vocab - 4, 4, gen0.len(), 17);
    for (im, cont) in calib_images(&fx.spec, 4).iter().zip(&conts) {
        let r = vision::float_forward(&vhl, &fx.spec, &vparams, im, None).expect("tower float");
        let stream: Vec<usize> = ids0.iter().chain(cont).copied().collect();
        let (ov, mp) = placed(&stream, &img, &f32s(&r));
        run_stats(ov, mp, &stream);
    }
    for seq in misaka_palw_tir_lower::fidelity::random_sequences(vocab - 4, 4, ids0.len() + gen0.len(), 5) {
        run_stats(Default::default(), Default::default(), &seq);
    }
    // 3. The integer LM as the text stage: its image rows an input bound to the tower's output.
    let opts = LowerOpts { max_window: Some(64), image_rows: Some(img), ..LowerOpts::default() };
    let lw = misaka_palw_tir_lower::lower::lower(&hl, &opts).expect("lower LM");
    let loader = Resident(Arc::new(params_f));
    let quiet = |_: usize, _: usize| {};
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise LM");
    let iv = tir::interval_v2::output_interval_v2(&vis.program).expect("tower output interval");
    let lm2 = encoder::lift_v2(
        &lw,
        &[(IMAGE_ROWS_PARAM, tir::program_v2::InputSource::External { lo: iv.lo as i64, hi: iv.hi as i64 })],
        tir::program_v2::OutputDecl::Logits { node: lw.program.logits, scheme_id: lw.program.logits_scheme_id },
    )
    .expect("LM v2");
    let lm_params = encoder::lifted_params(&lw.program, &[IMAGE_ROWS_PARAM], &mat.params);
    let max_trip = 64u32;
    let pipe = TirPipelineV1 {
        version: 1,
        stages: vec![
            StageDecl {
                name: "vision".into(),
                program: 0,
                trip: TripRule::Fixed { n: 1 },
                max_trip: 1,
                tokens: None,
                bind: vec![Binding::JobImage { index: 0 }],
            },
            StageDecl {
                name: "text".into(),
                program: 1,
                trip: TripRule::TextStream,
                max_trip,
                tokens: None,
                bind: vec![Binding::StageFinal { stage: 0 }],
            },
        ],
        output_stage: 1,
    };
    let programs = [vis.program.clone(), lm2.clone()];
    let params = ByProgram(vec![&vis.params, &lm_params]);
    let (mut same, mut total, mut agree, mut t_all, mut kl_sum) = (0usize, 0usize, 0usize, 0usize, 0f64);
    // HF's own top-2 margin wherever the integer stage picks another id: every such place must be
    // a near-tie (a precision matter, not a wrong placement).
    let mut tie_margins: Vec<f64> = vec![];
    for (im, rec) in &fx.images {
        let (ids, want) = (ids_of(&rec["input_ids"]), ids_of(&rec["generated"]));
        let job = PipelineJob {
            prompt: ids.iter().map(|t| *t as u32).collect(),
            images: vec![JobImageV1 { h: fx.spec.h, w: fx.spec.w, rgb: im.clone() }],
            ..Default::default()
        };
        // 4. Generate greedily: HF's `generate(do_sample=False)` for as many ids as HF produced.
        let mut k = 0usize;
        let budget = want.len();
        let mut select = |_: u32, logits: &tir::Tensor| {
            k += 1;
            let id = argmax(&logits.data) as u32;
            if k == budget { TextSelectV1::Last(id) } else { TextSelectV1::Next(id) }
        };
        let (run, got) = tir::pipeline::run_text_pipeline(&pipe, &programs, &params, &NoDraws, &job, &mut select).expect("text pipeline");
        let got: Vec<usize> = got.iter().map(|t| *t as usize).collect();
        let prefix = got.iter().zip(&want).take_while(|(a, b)| a == b).count();
        // 5. The committed ids replay the run exactly.
        let replay = tir::pipeline::run_pipeline(&pipe, &programs, &params, &NoDraws, &PipelineJob { generated: got.iter().map(|t| *t as u32).collect(), ..job.clone() })
            .expect("replay");
        assert_eq!(replay.output, run.output, "{name}: the replay differs from the generating run");
        // 6. Teacher-forced over HF's stream: the integer logits against HF's.
        let tf = tir::pipeline::run_pipeline(&pipe, &programs, &params, &NoDraws, &PipelineJob { generated: want.iter().map(|t| *t as u32).collect(), ..job.clone() })
            .expect("teacher-forced run");
        let hf = rows_of(&rec[stream_key]);
        let v = tf.output.shape[1];
        assert_eq!(tf.output.shape[0], hf.len(), "{name}: the stream's positions");
        let mut margins = vec![];
        let all: Vec<f64> = tf.output.data.iter().map(|c| *c as f64 * mat.logits_scale).collect();
        let r = rel(&all, &hf.concat());
        eprintln!("{name} integer text stage vs HF, teacher-forced: logits rel {r:.2e}");
        // Well inside the 1-D-position control's error: the integer positions are HF's.
        assert!(r < 0.03, "{name}: integer logits rel {r}");
        for (row, want_row) in tf.output.data.chunks(v).zip(&hf) {
            let got_row: Vec<f64> = row.iter().map(|c| *c as f64 * mat.logits_scale).collect();
            kl_sum += kl(want_row, &got_row);
            let mut s = want_row.clone();
            s.sort_by(|a, b| b.partial_cmp(a).unwrap());
            margins.push(s[0] - s[1]);
            if argmax(&got_row) == argmax(want_row) {
                agree += 1;
            } else {
                tie_margins.push(s[0] - s[1]);
            }
        }
        t_all += hf.len();
        // HF's own margin where the generation first departs (a near-tie is a precision matter).
        let at = ids.len() - 1 + prefix;
        let note = if prefix < want.len() {
            tie_margins.push(margins[at]);
            format!(", departs at id {prefix} where HF's top-2 margin is {:.4}", margins[at])
        } else {
            String::new()
        };
        eprintln!("{name} text pipeline: generated {got:?} vs HF {want:?}: {prefix}/{} identical{note}", want.len());
        same += prefix;
        total += want.len();
    }
    let kl_mean = kl_sum / t_all as f64;
    let worst = tie_margins.iter().cloned().fold(0f64, f64::max);
    eprintln!(
        "{name} text stage vs HF, teacher-forced over HF's streams: top-1 {agree}/{t_all}, mean KL {kl_mean:.5}; greedy ids {same}/{total}; \
         HF's largest top-2 margin where the integer stage differs {worst:.4}"
    );
    // 7. Admission: the pipeline, and the text stage's program on its own.
    let pa = tir::admit_v2::tir_admit_pipeline_v1(
        &pipe.encode(),
        &[vis.program.encode(), lm2.encode()],
        &misaka_palw_tir_lower::admission::default_inputs(),
        &tir::admit_v2::TirJobCeilingsV1::open_v1(),
    )
    .expect("tir_admit_pipeline_v1");
    let a = tir::admit_v2::tir_admit_program_v2(&lm2, &misaka_palw_tir_lower::admission::default_inputs()).expect("LM tir_admit_v2");
    let nodes = |p: &tir::program_v2::TirProgramV2| -> (usize, usize) {
        (p.blocks.iter().map(|b| b.nodes.len()).sum(), p.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0))
    };
    let (tn, tm) = nodes(&vis.program);
    let (ln, lm) = nodes(&lm2);
    eprintln!(
        "{name} pipeline admitted: job {:?}, {} step leaves; tower {tn} nodes (max {tm}/block), LM {ln} nodes (max {lm}/block, {} blocks, {} states), LM cones {}",
        pa.job_cost,
        pa.job_step_leaves,
        lm2.blocks.len(),
        lm2.states.len(),
        a.view.cones.len()
    );
    assert!(agree as f64 / t_all as f64 >= 0.85 && kl_mean < 0.05, "{name}: top-1 {agree}/{t_all}, KL {kl_mean}");
    assert!(worst < 0.05, "{name}: the integer stage differs from HF where HF's top-2 margin is {worst}, not a near-tie");
}

#[test]
fn llava_generates_hf_s_greedy_ids_through_the_text_pipeline() {
    text_stage("llava", "stream_logits");
}

#[test]
fn qwen2_vl_generates_hf_s_greedy_ids_through_the_text_pipeline() {
    text_stage("qwen2_vl", "logits");
}

#[test]
fn qwen2_5_vl_generates_hf_s_greedy_ids_through_the_text_pipeline() {
    text_stage("qwen2_5_vl", "logits");
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

/// **Real-size towers** (hub dimensions, no weights) are admitted at their usual input: CLIP
/// ViT-B/16 and SigLIP-B/16 at 224×224, Qwen2-VL-2B's tower at 224×224. Past the tile ceilings the
/// attention's softmax is split at commit points and the patch rows a pre-norm reads are committed
/// (`lower::bidir::split_softmax`, `resid_commit_needed`); the tiny fixtures stay below both.
#[test]
fn real_size_towers_are_admitted() {
    let clip_b16 = serde_json::json!({"architectures": ["CLIPVisionModelWithProjection"], "hidden_act": "quick_gelu", "hidden_size": 768,
        "image_size": 224, "intermediate_size": 3072, "layer_norm_eps": 1e-5, "model_type": "clip_vision_model", "num_attention_heads": 12,
        "num_channels": 3, "num_hidden_layers": 12, "patch_size": 16, "projection_dim": 512});
    let siglip_b16 = serde_json::json!({"architectures": ["SiglipVisionModel"], "hidden_act": "gelu_pytorch_tanh", "hidden_size": 768,
        "image_size": 224, "intermediate_size": 3072, "layer_norm_eps": 1e-6, "model_type": "siglip_vision_model", "num_attention_heads": 12,
        "num_channels": 3, "num_hidden_layers": 12, "patch_size": 16, "vision_use_head": true});
    let qwen2_vl_2b = serde_json::json!({"architectures": ["Qwen2VLForConditionalGeneration"], "vision_config": {"depth": 32, "embed_dim": 1280,
        "hidden_size": 1536, "hidden_act": "quick_gelu", "mlp_ratio": 4, "num_heads": 16, "in_channels": 3, "patch_size": 14,
        "spatial_merge_size": 2, "temporal_patch_size": 2}});
    for (name, cfg) in [("CLIP ViT-B/16", clip_b16), ("SigLIP-B/16", siglip_b16), ("Qwen2-VL-2B tower", qwen2_vl_2b)] {
        let s = vision::parse_vision(&cfg.to_string(), Some((224, 224)), None).expect("parse");
        let (hl, _) = vision::hl_program(&s).expect("hl");
        let lw = vision::lower_vision(&hl, &s).expect("lower");
        let p2 = encoder::vision_v2(&lw).expect("v2");
        let a = tir::admit_v2::tir_admit_program_v2(&p2, &misaka_palw_tir_lower::admission::default_inputs())
            .unwrap_or_else(|e| panic!("{name}: tir_admit_v2: {e}"));
        let nodes: usize = p2.blocks.iter().map(|b| b.nodes.len()).sum();
        let most = p2.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0);
        eprintln!(
            "{name} at 224×224 ({} rows): admitted — {nodes} nodes (max {most}/block), {:.3e} MACs, {} step leaves",
            s.rows(),
            a.view.position.cost.macs as f64,
            a.view.position.step_leaves
        );
    }
}

/// **The adapters of kind `vision` are the Rust reader's data form**: CLIP's and SigLIP's adapters instantiate the same
/// `VisionSpec` (every field, every name) as `parse_vision_rust` on the fixtures' configurations.
#[test]
fn the_vision_adapters_agree_with_the_rust_reader() {
    for name in ["clip_vision", "siglip_vision"] {
        let dir = fixture_dir(name);
        let cfg = std::fs::read_to_string(dir.join("config.json")).expect("config");
        let o: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).expect("outputs")).expect("json");
        let arr = |v: &serde_json::Value| -> [f64; 3] { [v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap()] };
        let size = (o["size"][0].as_u64().unwrap() as u32, o["size"][1].as_u64().unwrap() as u32);
        let ms = Some((arr(&o["mean"]), arr(&o["std"])));
        let a = vision::parse_vision(&cfg, Some(size), ms).expect("adapter");
        let r = vision::parse_vision_rust(&cfg, Some(size), ms).expect("rust");
        assert_eq!(serde_json::to_value(&a).unwrap(), serde_json::to_value(&r).unwrap(), "{name}");
        // And without the processor's numbers: each family's default normalisation.
        let a = vision::parse_vision(&cfg, None, None).expect("adapter");
        let r = vision::parse_vision_rust(&cfg, None, None).expect("rust");
        assert_eq!(serde_json::to_value(&a).unwrap(), serde_json::to_value(&r).unwrap(), "{name} defaults");
    }
}

/// **The out-major tower computes the same integers** (`lower_vision_with(.., true)`: `[out, in]` weights, `[1,out,in]·[L,in,1]`):
/// on every tower fixture, both lowerings materialised from the same calibration give byte-identical outputs on the fixture images,
/// and the out-major program passes the three implementations and the court.
#[test]
fn the_out_major_tower_computes_the_default_tower_s_integers() {
    for name in ["clip_vision", "siglip_vision", "qwen2_vl", "qwen2_5_vl", "llava"] {
        let fx = load(name);
        let s = &fx.spec;
        let dir = fixture_dir(name);
        let (hl, binding) = vision::hl_program(s).expect("hl");
        let ck = Checkpoint::open(&dir).expect("checkpoint");
        let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
        let mut stats = std::collections::BTreeMap::new();
        for img in calib_images(s, 4) {
            vision::float_forward(&hl, s, &params_f, &img, Some(&mut stats)).expect("calibration");
        }
        let loader = Resident(Arc::new(params_f));
        let quiet = |_: usize, _: usize| {};
        let mut outs = Vec::new();
        for out_major in [false, true] {
            let lw = vision::lower_vision_with(&hl, s, out_major).expect("lower");
            let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
            let p2 = encoder::vision_v2(&lw).expect("v2");
            let params2 = encoder::lifted_params(&lw.program, &[vision::IMAGE_PARAM], &mat.params);
            let interp = tir::interp_v2::InterpreterV2::new(&p2).expect("interpreter v2");
            let mut per_image = Vec::new();
            for (img, _) in &fx.images {
                let mut inputs = tir::interp_v2::MapInputs::default();
                let t = tir::Tensor::new(tir::DType::I16, vec![s.h as usize, s.w as usize, 3], img.iter().map(|v| *v as i128).collect()).unwrap();
                inputs.constant.insert(0, t);
                per_image.push(interp.run_positions(&params2, &inputs, 1).expect("v2 run").remove(0).output);
            }
            if out_major {
                let img = &fx.images[0].0;
                let t = misaka_palw_tir_lower::lower::IntTensor::i16(vec![s.h as usize, s.w as usize, 3], img.iter().map(|v| *v as i16).collect());
                let p6 = common::with_inputs(&lw.program, &mat.params, &[(vision::IMAGE_PARAM, t)]);
                common::three_ways(&lw.program, &p6, &[vec![0]]).unwrap_or_else(|e| panic!("{name} out-major: three implementations: {e}"));
                let c = common::court_coverage(&lw.program, &p6, &[0], &[0], &[1]).unwrap_or_else(|e| panic!("{name} out-major: court: {e}"));
                assert!(c.commits > 0);
            }
            outs.push((per_image, mat.logits_scale));
        }
        assert_eq!(outs[0].1, outs[1].1, "{name}: the same output scale");
        assert_eq!(outs[0].0, outs[1].0, "{name}: the out-major tower's integers are the default tower's");
        eprintln!("{name}: out-major tower = default tower on {} images", fx.images.len());
    }
}
