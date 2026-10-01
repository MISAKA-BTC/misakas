//! **FR-19, convolutional networks (`CONV_2D_V1`, `BN_FOLD_V1`, `POOL_MAX_2D_V1`, `RESIDUAL_ADD_ACT_V1`) against their
//! Hugging Face fixtures.** Each tiny ResNet (`tools/gen_hf_vision_fixtures.py`, random weights and batch-norm statistics,
//! rounded to bfloat16) is read into a `CnnSpec`, bound to its checkpoint and checked in the five steps of `tests/vision.rs`:
//! its float reference against transformers on the fixture's canonical images, calibration on other random images, the
//! lowering to an integer program (a convolution is an im2col gather and one matmul; the batch norm is folded), the version-2
//! program on `InterpreterV2` (the image a `u8` HWC input), and admission. The integer output is held against HF's by cosine.

mod common;

use misaka_palw_tir as tir;
use misaka_palw_tir_lower::encoder;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::cnn::{self, BnOp, CnnOp, CnnOut, CnnSpec, ConvOp};
use misaka_palw_tir_lower::lower::materialise;
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::spec::Act;
use misaka_palw_tir_lower::weights::Checkpoint;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-vis").join(name)
}

/// ResNet's structure from its config, written out in Rust: the oracle the data adapter (`adapters/resnet.json`) must equal.
fn resnet_spec(cfg: &Value, size: (u32, u32), ms: ([f64; 3], [f64; 3]), out: CnnOut) -> CnnSpec {
    let usz = |k: &str| cfg[k].as_u64().unwrap() as usize;
    let emb = usz("embedding_size");
    let sizes: Vec<usize> = cfg["hidden_sizes"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect();
    let depths: Vec<usize> = cfg["depths"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect();
    let bottleneck = cfg["layer_type"] == "bottleneck";
    let first = cfg["downsample_in_first_stage"].as_bool().unwrap_or(false);
    let in_bottleneck = cfg["downsample_in_bottleneck"].as_bool().unwrap_or(false);
    let bn = |name: String| Some(BnOp { name, eps: 1e-5 });
    let conv = |name: &str, cin, cout, k, stride, act: Option<Act>, bnname: &str| {
        CnnOp::Conv(ConvOp { name: name.into(), cin, cout, k, stride, pad: k / 2, dilation: 1, groups: 1, bias: false, bn: bn(bnname.into()), act })
    };
    let mut ops = vec![conv("embedder.embedder.convolution", 3, emb, 7, 2, Some(Act::Relu), "embedder.embedder.normalization"), CnnOp::MaxPool { k: 3, stride: 2, pad: 1 }];
    let mut in_ch = emb;
    for (s, (out_ch, depth)) in sizes.iter().zip(&depths).enumerate() {
        let stage_stride = if s == 0 && !first { 1 } else { 2 };
        for l in 0..*depth {
            let (cin, stride) = if l == 0 { (in_ch, stage_stride) } else { (*out_ch, 1) };
            let p = format!("encoder.stages.{s}.layers.{l}");
            let c = |j: usize, cin, cout, k, stride, act| conv(&format!("{p}.layer.{j}.convolution"), cin, cout, k, stride, act, &format!("{p}.layer.{j}.normalization"));
            let main = if bottleneck {
                let red = out_ch / 4;
                vec![
                    c(0, cin, red, 1, if in_bottleneck { stride } else { 1 }, Some(Act::Relu)),
                    c(1, red, red, 3, if in_bottleneck { 1 } else { stride }, Some(Act::Relu)),
                    c(2, red, *out_ch, 1, 1, None),
                ]
            } else {
                vec![c(0, cin, *out_ch, 3, stride, Some(Act::Relu)), c(1, *out_ch, *out_ch, 3, 1, None)]
            };
            let shortcut = if cin != *out_ch || stride != 1 {
                vec![conv(&format!("{p}.shortcut.convolution"), cin, *out_ch, 1, stride, None, &format!("{p}.shortcut.normalization"))]
            } else {
                vec![]
            };
            ops.push(CnnOp::Residual { main, shortcut, act: Some(Act::Relu) });
        }
        in_ch = *out_ch;
    }
    CnnSpec { architecture: cfg["architectures"][0].as_str().unwrap().into(), h: size.0, w: size.1, mean: ms.0, std: ms.1, ops, out, ignored: vec![], aliases: vec![] }
}

struct Fixture {
    cfg: Value,
    size: (u32, u32),
    ms: ([f64; 3], [f64; 3]),
    images: Vec<(Vec<u8>, Value)>,
}

fn load(name: &str) -> Fixture {
    let dir = fixture_dir(name);
    let cfg: Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).expect("config")).expect("json");
    let o: Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).expect("outputs")).expect("json");
    let arr = |v: &Value| -> [f64; 3] { [v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap()] };
    let size = (o["size"][0].as_u64().unwrap() as u32, o["size"][1].as_u64().unwrap() as u32);
    let images = o["images"].as_array().unwrap().iter().map(|r| (r["hwc"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect(), r["outputs"].clone())).collect();
    Fixture { cfg, size, ms: (arr(&o["mean"]), arr(&o["std"])), images }
}

fn rows_of(v: &Value) -> Vec<Vec<f64>> {
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

fn calib_images(spec: &CnnSpec, n: usize) -> Vec<Vec<u8>> {
    use rand::{Rng, SeedableRng};
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(99);
    (0..n).map(|_| (0..(spec.h * spec.w * 3) as usize).map(|_| rng.gen_range(0..=255u8)).collect()).collect()
}

/// The five steps on one fixture; the output rows are compared to `key` of the fixture.
fn check(name: &str, out: CnnOut, key: &str) -> usize {
    let fx = load(name);
    let spec = resnet_spec(&fx.cfg, fx.size, fx.ms, out);
    let dir = fixture_dir(name);
    let (hl, binding) = cnn::hl_program(&spec).expect("hl");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, unused) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    assert!(unused.is_empty(), "{name}: checkpoint tensors the network never reads: {unused:?}");
    // 1. The float reference is the HF network.
    for (img, o) in &fx.images {
        let got = cnn::float_forward(&hl, &spec, &params_f, img, None).expect("float");
        let want = rows_of(&o[key]);
        assert_eq!((got.len(), got[0].len()), (want.len(), want[0].len()), "{name}: shape");
        let r = rel(&got.concat(), &want.concat());
        eprintln!("{name} float reference vs HF `{key}`: rel {r:.2e} ({} rows × {})", got.len(), got[0].len());
        assert!(r < 1e-4, "{name}: float vs HF rel {r}");
    }
    // 2. Calibration.
    let mut stats = std::collections::BTreeMap::new();
    for img in calib_images(&spec, 6) {
        cnn::float_forward(&hl, &spec, &params_f, &img, Some(&mut stats)).expect("calibration");
    }
    // 3. The integer program.
    let lw = cnn::lower_cnn(&hl, &spec).expect("lower");
    let loader = Resident(Arc::new(params_f));
    let quiet = |_: usize, _: usize| {};
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    // 4. Version 2 on InterpreterV2.
    let p2 = encoder::vision_v2(&lw).expect("v2");
    let params2 = encoder::lifted_params(&lw.program, &[cnn::IMAGE_PARAM], &mat.params);
    let interp = tir::interp_v2::InterpreterV2::new(&p2).expect("interpreter v2");
    for (img, o) in &fx.images {
        let mut inputs = tir::interp_v2::MapInputs::default();
        let t = tir::Tensor::new(tir::DType::I16, vec![spec.h as usize, spec.w as usize, 3], img.iter().map(|v| *v as i128).collect()).unwrap();
        inputs.constant.insert(0, t);
        let run = interp.run_positions(&params2, &inputs, 1).expect("v2 run");
        let want = rows_of(&o[key]);
        let width = want[0].len();
        let got: Vec<Vec<f64>> = run[0].output.data.chunks(width).map(|r| r.iter().map(|c| *c as f64 * mat.logits_scale).collect()).collect();
        assert_eq!(got.len(), want.len(), "{name}: integer rows");
        let cos: Vec<f64> = got.iter().zip(&want).map(|(a, b)| cosine(a, b)).collect();
        let (mean, min) = (cos.iter().sum::<f64>() / cos.len() as f64, cos.iter().cloned().fold(1.0, f64::min));
        eprintln!("{name} integer vs HF `{key}`: cosine mean {mean:.6} min {min:.6}, rel {:.2e} ({} rows × {width})", rel(&got.concat(), &want.concat()), got.len());
        assert!(min > 0.995, "{name}: cosine min {min}");
    }
    // 5. The class's one-stage pipeline: the image bound by JobImage must give the standalone program's bytes.
    let pipe = encoder::vision_pipeline();
    let info = tir::pipeline::validate_pipeline(&pipe, std::slice::from_ref(&p2)).expect("pipeline normal form");
    assert_eq!(info.images, vec![[spec.h, spec.w]], "{name}: one image slot of the network's size");
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
        let job = tir::pipeline::PipelineJob { images: vec![tir::pipeline::JobImageV1 { h: spec.h, w: spec.w, rgb: img.clone() }], ..Default::default() };
        let pr = tir::pipeline::run_pipeline(&pipe, std::slice::from_ref(&p2), &One(&params2), &NoRandom, &job).expect("pipeline run");
        let mut inputs = tir::interp_v2::MapInputs::default();
        let t = tir::Tensor::new(tir::DType::I16, vec![spec.h as usize, spec.w as usize, 3], img.iter().map(|v| *v as i128).collect()).unwrap();
        inputs.constant.insert(0, t);
        let direct = interp.run_positions(&params2, &inputs, 1).expect("v2 run").remove(0).output;
        assert_eq!(pr.output, direct, "{name}: the pipeline differs from the program");
    }
    let pa = tir::admit_v2::tir_admit_pipeline_v1(&pipe.encode(), &[p2.encode()], &misaka_palw_tir_lower::admission::default_inputs(), &tir::admit_v2::TirJobCeilingsV1::open_v1())
        .expect("tir_admit_pipeline_v1");
    eprintln!("{name} pipeline (JobImage) admitted: job {:?}, {} step leaves", pa.job_cost, pa.job_step_leaves);
    // 6. The three implementations and the court on the network's version-1 view (the image an input param): every layer
    //    block's carry is a committed leaf the next block's cones start from.
    {
        let img = &fx.images[0].0;
        let t = misaka_palw_tir_lower::lower::IntTensor::i16(vec![spec.h as usize, spec.w as usize, 3], img.iter().map(|v| *v as i16).collect());
        let p6 = common::with_inputs(&lw.program, &mat.params, &[(cnn::IMAGE_PARAM, t)]);
        let n3 = common::three_ways(&lw.program, &p6, &[vec![0]]).unwrap_or_else(|e| panic!("{name}: three implementations: {e}"));
        let c = common::court_coverage(&lw.program, &p6, &[0], &[0], &[1]).unwrap_or_else(|e| panic!("{name}: court: {e}"));
        eprintln!("{name} COURT: three implementations equal ({n3} position); the court replays {} commit points ({} nodes, {} elements) over {} primitives", c.commits, c.nodes, c.elements, c.primitives.len());
        assert!(c.commits > 0);
    }
    // 7. Admission of the program alone.
    let a = tir::admit_v2::tir_admit_program_v2(&p2, &misaka_palw_tir_lower::admission::default_inputs()).expect("tir_admit_v2");
    eprintln!("{name} admitted: {} nodes in {} blocks, {} cones, {} params", p2.blocks.iter().map(|b| b.nodes.len()).sum::<usize>(), p2.blocks.len(), a.view.cones.len(), p2.params.len());
    p2.blocks.len()
}

#[test]
fn resnet_basic_feature_map_matches_its_hf_fixture() {
    assert_eq!(check("resnet", CnnOut::Map, "last_hidden_state"), 2, "pre and post only");
}

fn real(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/cnn").join(format!("{name}.json"))).expect("config")
}

/// **The adapter is the Rust structure's data form**: `adapters/resnet.json` instantiates the same `CnnSpec` (every op,
/// every name) as the ResNet written out in Rust, on both fixtures and on the real ResNet-18, 50 and 152.
#[test]
fn the_resnet_adapter_equals_the_rust_structure() {
    let ms = ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]);
    let mut cases: Vec<(String, String)> = ["resnet", "resnet_bottleneck"].iter().map(|n| (n.to_string(), std::fs::read_to_string(fixture_dir(n).join("config.json")).unwrap())).collect();
    cases.extend(["resnet-18", "resnet-50", "resnet-152"].iter().map(|n| (n.to_string(), real(n))));
    for (name, text) in cases {
        let cfg: Value = serde_json::from_str(&text).unwrap();
        for (size, out) in [((32u32, 32u32), CnnOut::GlobalAvg), ((224, 224), CnnOut::GlobalAvg)] {
            let mut want = resnet_spec(&cfg, size, ms, out);
            want.ignored = vec!["classifier.".into()];
            want.aliases = vec![["".into(), "resnet.".into()]];
            let got = cnn::parse_cnn(&text, Some(size), None).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(serde_json::to_value(&got).unwrap() == serde_json::to_value(&want).unwrap(), "{name} at {size:?}: the adapter's spec differs from the Rust structure");
        }
    }
}


#[test]
fn resnet_bottleneck_feature_map_matches_its_hf_fixture() {
    check("resnet_bottleneck", CnnOut::Map, "last_hidden_state");
}

/// The class's other output choice: the pooled vector (global average pooling, ResNet's `pooler_output`).
#[test]
fn resnet_pooled_vector_matches_its_hf_fixture() {
    check("resnet", CnnOut::GlobalAvg, "pooler_output");
    check("resnet_bottleneck", CnnOut::GlobalAvg, "pooler_output");
}

/// **Admission at the real shapes** (no weights read): ResNet-18, -50 and -152 at 224×224 lower to a handful of blocks inside the
/// normal form's caps and their one-stage pipelines are admitted at the legacy court's ceilings.
#[test]
fn real_resnets_lower_and_are_admitted_at_224() {
    for name in ["resnet-18", "resnet-50", "resnet-152"] {
        let spec = cnn::parse_cnn(&real(name), Some((224, 224)), None).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (hl, _) = cnn::hl_program(&spec).expect("hl");
        let lw = cnn::lower_cnn(&hl, &spec).unwrap_or_else(|e| panic!("{name}: lower: {e}"));
        let p2 = encoder::vision_v2(&lw).expect("v2");
        let nodes: usize = p2.blocks.iter().map(|b| b.nodes.len()).sum();
        let most = p2.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0);
        assert!(most <= 512 && p2.blocks.len() <= 16);
        let pipe = encoder::vision_pipeline();
        let pa = tir::admit_v2::tir_admit_pipeline_v1(&pipe.encode(), &[p2.encode()], &misaka_palw_tir_lower::admission::default_inputs(), &tir::admit_v2::TirJobCeilingsV1::open_v1())
            .unwrap_or_else(|e| panic!("{name}: tir_admit_pipeline_v1: {e}"));
        eprintln!(
            "{name} at 224: ADMITTED — {} blocks, {nodes} nodes (largest {most}), job {:.3e} MACs, {} step leaves, cone work {}",
            p2.blocks.len(),
            pa.job_cost.macs as f64,
            pa.job_step_leaves,
            pa.cone_work
        );
        assert!(pa.cone_work <= 1 << 16, "{name}: cone work {} past testnet-12's 65,536", pa.cone_work);
    }
}

/// **A network that needs layer blocks**: the carry between them changes shape (the feature map halves each stage and the
/// channels grow), yet a program has one carry signature — the flattened, zero-padded activation. The deep fixture's program has
/// a layer block, and its output still matches the transformers feature map and its pooled vector.
#[test]
fn a_deep_resnet_crosses_a_block_boundary_with_a_padded_carry() {
    let blocks = check("resnet_deep", CnnOut::Map, "last_hidden_state");
    assert!(blocks >= 3, "the deep network fits pre and post only ({blocks} blocks) — the carry is not exercised");
    check("resnet_deep", CnnOut::GlobalAvg, "pooler_output");
}

// ───────────────────────────── networks transformers has no fixture for ─────────────────────────────

/// A minimal safetensors file of f32 tensors, in the order given.
fn write_safetensors(path: &Path, tensors: &[(String, Vec<usize>, Vec<f32>)]) {
    let mut header = serde_json::Map::new();
    let mut at = 0usize;
    for (name, shape, data) in tensors {
        let n = data.len() * 4;
        header.insert(name.clone(), serde_json::json!({"dtype": "F32", "shape": shape, "data_offsets": [at, at + n]}));
        at += n;
    }
    let h = serde_json::to_vec(&header).expect("header");
    let mut out = (h.len() as u64).to_le_bytes().to_vec();
    out.extend(h);
    for (.., data) in tensors {
        for v in data {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::write(path, out).expect("write");
}

/// The tensors a convolution (and its batch norm) reads, random but well-scaled: `[cout, cin/groups, k, k]` weights of fan-in
/// scale, batch-norm statistics near 1.
fn random_conv_tensors(rng: &mut rand_chacha::ChaCha8Rng, c: &ConvOp) -> Vec<(String, Vec<usize>, Vec<f32>)> {
    use rand::Rng;
    let taps = c.cin / c.groups * c.k * c.k;
    let scale = 1.7 / (taps as f32).sqrt();
    let mut v = vec![(format!("{}.weight", c.name), vec![c.cout, c.cin / c.groups, c.k, c.k], (0..c.cout * taps).map(|_| rng.gen_range(-1.0f32..1.0) * scale).collect::<Vec<_>>())];
    if c.bias {
        v.push((format!("{}.bias", c.name), vec![c.cout], (0..c.cout).map(|_| rng.gen_range(-0.3f32..0.3)).collect()));
    }
    if let Some(bn) = &c.bn {
        let mut vec_of = |lo: f32, hi: f32| -> Vec<f32> { (0..c.cout).map(|_| rng.gen_range(lo..hi)).collect() };
        v.push((format!("{}.weight", bn.name), vec![c.cout], vec_of(0.5, 1.5)));
        v.push((format!("{}.bias", bn.name), vec![c.cout], vec_of(-0.3, 0.3)));
        v.push((format!("{}.running_mean", bn.name), vec![c.cout], vec_of(-0.2, 0.2)));
        v.push((format!("{}.running_var", bn.name), vec![c.cout], vec_of(0.5, 1.5)));
    }
    v
}

fn walk_convs<'a>(ops: &'a [CnnOp], f: &mut impl FnMut(&'a ConvOp)) {
    for op in ops {
        match op {
            CnnOp::Conv(c) => f(c),
            CnnOp::Residual { main, shortcut, .. } => {
                walk_convs(main, f);
                walk_convs(shortcut, f);
            }
            _ => {}
        }
    }
}

/// The reference for a network transformers has no fixture for: a direct, naive convolution over `[H·W, C]` rows in f64 (groups
/// and dilation included), with its batch norm and activation — no im2col, no folding, nothing the lowering shares.
struct Naive<'a> {
    t: &'a std::collections::BTreeMap<String, Vec<f32>>,
}

impl Naive<'_> {
    fn act(a: Option<Act>, x: f64) -> f64 {
        match a {
            Some(Act::Relu) => x.max(0.0),
            None | Some(Act::Identity) => x,
            Some(Act::Silu) => x / (1.0 + (-x).exp()),
            other => panic!("the naive reference has no {other:?}"),
        }
    }

    fn conv(&self, x: &[Vec<f64>], (h, w): (usize, usize), c: &ConvOp) -> (Vec<Vec<f64>>, (usize, usize)) {
        let eff = (c.k - 1) * c.dilation + 1;
        let (ho, wo) = ((h + 2 * c.pad - eff) / c.stride + 1, (w + 2 * c.pad - eff) / c.stride + 1);
        let wt = &self.t[&format!("{}.weight", c.name)];
        let cin_g = c.cin / c.groups;
        let per = c.cout / c.groups;
        let mut out = vec![vec![0.0; c.cout]; ho * wo];
        for oy in 0..ho {
            for ox in 0..wo {
                for co in 0..c.cout {
                    let g = co / per;
                    let mut acc = if c.bias { self.t[&format!("{}.bias", c.name)][co] as f64 } else { 0.0 };
                    for ci in 0..cin_g {
                        for ky in 0..c.k {
                            for kx in 0..c.k {
                                let (iy, ix) = ((oy * c.stride + ky * c.dilation) as isize - c.pad as isize, (ox * c.stride + kx * c.dilation) as isize - c.pad as isize);
                                if iy < 0 || ix < 0 || iy >= h as isize || ix >= w as isize {
                                    continue;
                                }
                                acc += x[iy as usize * w + ix as usize][g * cin_g + ci] * wt[((co * cin_g + ci) * c.k + ky) * c.k + kx] as f64;
                            }
                        }
                    }
                    if let Some(bn) = &c.bn {
                        let (gm, bt, mu, var) = (
                            self.t[&format!("{}.weight", bn.name)][co] as f64,
                            self.t[&format!("{}.bias", bn.name)][co] as f64,
                            self.t[&format!("{}.running_mean", bn.name)][co] as f64,
                            self.t[&format!("{}.running_var", bn.name)][co] as f64,
                        );
                        acc = gm * (acc - mu) / (var + bn.eps).sqrt() + bt;
                    }
                    out[oy * wo + ox][co] = Self::act(c.act, acc);
                }
            }
        }
        (out, (ho, wo))
    }

    fn ops(&self, ops: &[CnnOp], mut x: Vec<Vec<f64>>, mut hw: (usize, usize)) -> (Vec<Vec<f64>>, (usize, usize)) {
        for op in ops {
            match op {
                CnnOp::Conv(c) => (x, hw) = self.conv(&x, hw, c),
                CnnOp::Act(a) => x = x.iter().map(|r| r.iter().map(|v| Self::act(Some(*a), *v)).collect()).collect(),
                CnnOp::Residual { main, shortcut, act } => {
                    let (m, mhw) = self.ops(main, x.clone(), hw);
                    let (s, _) = if shortcut.is_empty() { (x.clone(), hw) } else { self.ops(shortcut, x.clone(), hw) };
                    x = m.iter().zip(&s).map(|(a, b)| a.iter().zip(b).map(|(p, q)| Self::act(*act, p + q)).collect()).collect();
                    hw = mhw;
                }
                other => panic!("the naive reference has no {other:?}"),
            }
        }
        (x, hw)
    }
}

/// A MobileNet-shaped network written by hand — depthwise-separable convolutions, a dilated depthwise convolution, a
/// depthwise convolution with a bias and no batch norm, an identity residual, SiLU as a fused and as a standalone table
/// activation — against the naive reference, for what transformers has no tiny fixture. The integer program follows it by cosine.
#[test]
fn a_depthwise_separable_network_with_dilation_follows_a_naive_reference() {
    use rand::SeedableRng;
    let bn = |n: &str| Some(BnOp { name: format!("{n}.bn"), eps: 1e-5 });
    let conv = |name: &str, cin, cout, k, stride, pad, dilation, groups, bias, act: Option<Act>| {
        let bn = if bias { None } else { bn(name) };
        ConvOp { name: name.into(), cin, cout, k, stride, pad, dilation, groups, bias, bn, act }
    };
    let ops = vec![
        CnnOp::Conv(conv("stem", 3, 8, 3, 2, 1, 1, 1, false, Some(Act::Relu))),
        CnnOp::Conv(conv("b1.dw", 8, 8, 3, 1, 1, 1, 8, false, Some(Act::Relu))),
        CnnOp::Conv(conv("b1.pw", 8, 16, 1, 1, 0, 1, 1, false, Some(Act::Silu))),
        CnnOp::Residual {
            main: vec![CnnOp::Conv(conv("b2.dw", 16, 16, 3, 1, 2, 2, 16, false, Some(Act::Relu))), CnnOp::Conv(conv("b2.pw", 16, 16, 1, 1, 0, 1, 1, false, None))],
            shortcut: vec![],
            act: Some(Act::Relu),
        },
        CnnOp::Act(Act::Silu),
        CnnOp::Conv(conv("b3.dw", 16, 16, 3, 2, 1, 1, 16, true, None)),
    ];
    let spec = CnnSpec {
        architecture: "SyntheticDepthwiseNet".into(),
        h: 16,
        w: 16,
        mean: [0.5, 0.5, 0.5],
        std: [0.25, 0.25, 0.25],
        ops,
        out: CnnOut::Map,
        ignored: vec![],
        aliases: vec![],
    };
    spec.validate().expect("valid");
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);
    let mut tensors = Vec::new();
    walk_convs(&spec.ops, &mut |c| tensors.extend(random_conv_tensors(&mut rng, c)));
    let dir = std::env::temp_dir().join(format!("cnn-depthwise-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    write_safetensors(&dir.join("model.safetensors"), &tensors);
    let by_name: std::collections::BTreeMap<String, Vec<f32>> = tensors.iter().map(|(n, _, d)| (n.clone(), d.clone())).collect();
    let naive = Naive { t: &by_name };
    // The float reference against the naive one.
    let (hl, binding) = cnn::hl_program(&spec).expect("hl");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, unused) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    assert!(unused.is_empty(), "{unused:?}");
    let images = calib_images(&spec, 3);
    let reference = |img: &[u8]| -> Vec<Vec<f64>> {
        let x: Vec<Vec<f64>> = (0..(spec.h * spec.w) as usize).map(|p| (0..3).map(|ch| (img[p * 3 + ch] as f64 / 255.0 - spec.mean[ch]) / spec.std[ch]).collect()).collect();
        naive.ops(&spec.ops, x, (spec.h as usize, spec.w as usize)).0
    };
    for img in &images {
        let got = cnn::float_forward(&hl, &spec, &params_f, img, None).expect("float");
        let want = reference(img);
        assert_eq!((got.len(), got[0].len()), (want.len(), want[0].len()), "shape");
        let r = rel(&got.concat(), &want.concat());
        eprintln!("depthwise net: float reference vs the naive convolution: rel {r:.2e} ({} rows × {})", got.len(), got[0].len());
        assert!(r < 1e-5, "float vs naive rel {r}");
    }
    // Calibrate on other images, lower, run the version-2 program, admit.
    let mut stats = std::collections::BTreeMap::new();
    for img in calib_images(&spec, 6).iter().rev() {
        cnn::float_forward(&hl, &spec, &params_f, img, Some(&mut stats)).expect("calibration");
    }
    let lw = cnn::lower_cnn(&hl, &spec).expect("lower");
    let quiet = |_: usize, _: usize| {};
    let mat = materialise(&lw, &hl, &Resident(Arc::new(params_f)), &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let p2 = encoder::vision_v2(&lw).expect("v2");
    let params2 = encoder::lifted_params(&lw.program, &[cnn::IMAGE_PARAM], &mat.params);
    let interp = tir::interp_v2::InterpreterV2::new(&p2).expect("interpreter v2");
    for img in &images {
        let mut inputs = tir::interp_v2::MapInputs::default();
        let t = tir::Tensor::new(tir::DType::I16, vec![spec.h as usize, spec.w as usize, 3], img.iter().map(|v| *v as i128).collect()).unwrap();
        inputs.constant.insert(0, t);
        let run = interp.run_positions(&params2, &inputs, 1).expect("v2 run");
        let want = reference(img);
        let width = want[0].len();
        let got: Vec<Vec<f64>> = run[0].output.data.chunks(width).map(|r| r.iter().map(|c| *c as f64 * mat.logits_scale).collect()).collect();
        let cos: Vec<f64> = got.iter().zip(&want).map(|(a, b)| cosine(a, b)).collect();
        let (mean, min) = (cos.iter().sum::<f64>() / cos.len() as f64, cos.iter().cloned().fold(1.0, f64::min));
        eprintln!("depthwise net: integer vs the naive reference: cosine mean {mean:.6} min {min:.6}, rel {:.2e}", rel(&got.concat(), &want.concat()));
        assert!(min > 0.99, "cosine min {min}");
    }
    tir::admit_v2::tir_admit_program_v2(&p2, &misaka_palw_tir_lower::admission::default_inputs()).expect("admitted");
    let _ = std::fs::remove_dir_all(dir);
}

/// A grouping other than 1 and depthwise is refused by name, before anything is sized on it.
#[test]
fn a_grouped_convolution_that_is_not_depthwise_is_refused_by_name() {
    let spec = CnnSpec {
        architecture: "Grouped".into(),
        h: 8,
        w: 8,
        mean: [0.0; 3],
        std: [1.0; 3],
        ops: vec![
            CnnOp::Conv(ConvOp { name: "stem".into(), cin: 3, cout: 4, k: 1, stride: 1, pad: 0, dilation: 1, groups: 1, bias: false, bn: None, act: None }),
            CnnOp::Conv(ConvOp { name: "g".into(), cin: 4, cout: 8, k: 3, stride: 1, pad: 1, dilation: 1, groups: 2, bias: false, bn: None, act: None }),
        ],
        out: CnnOut::Map,
        ignored: vec![],
        aliases: vec![],
    };
    let e = spec.validate().unwrap_err().to_string();
    assert!(e.contains("groups = 2") && e.contains("only 1 and depthwise"), "{e}");
}

/// **What `palw-class check-architecture` prints for a convolutional network**: the adapter that read it, the features it uses
/// (each in the registry, each implemented), the assumption made about the input size, and its checkpoint's tensors accounted
/// for (a wrapped classifier's `classifier.` head is the adapter's to ignore, not a missing feature).
#[test]
fn the_architecture_report_names_a_resnets_features_and_checks_its_checkpoint() {
    use misaka_palw_tir_lower::hf_schema::{AdapterSource, Level, ReadOptions, TensorIndex};
    use misaka_palw_tir_lower::model::{FeatureStatus, ReportResult, analyze};
    let dir = fixture_dir("resnet_bottleneck");
    let cfg: Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap();
    let tensors = TensorIndex::from_checkpoint_path(&dir).expect("tensor names");
    let r = analyze(&cfg, Some(&tensors), &ReadOptions::default());
    assert_eq!(r.level, Level::B, "{}", r.render());
    assert!(matches!(&r.adapter, AdapterSource::BuiltIn { id, .. } if id == "resnet"), "{:?}", r.adapter);
    assert_eq!(r.result, ReportResult::Lowerable, "{}", r.render());
    assert!(r.unread_tensors.is_empty() && r.weight_errors.is_empty(), "{:?} {:?}", r.unread_tensors, r.weight_errors);
    assert!(!r.new_consensus_primitive_required && !r.new_court_kernel_required);
    let ids: Vec<&str> = r.features.iter().map(|f| f.id.as_str()).collect();
    for want in ["CNN_FROM_SPEC_V1", "CONV_DENSE_V1", "BN_FOLD_V1", "POOL_MAX_2D_V1", "RESIDUAL_ADD_ACT_V1", "POOL_AVG_GLOBAL_V1"] {
        assert!(ids.contains(&want), "{want} missing from {ids:?}");
    }
    assert!(!ids.contains(&"CONV_DEPTHWISE_2D_V1") && !ids.contains(&"ACT_TABLE_V1"), "{ids:?}");
    assert!(r.features.iter().all(|f| f.status == FeatureStatus::Supported), "{:#?}", r.features);
    assert!(r.assumed_defaults.iter().any(|d| d.contains("input size 224x224")), "{:?}", r.assumed_defaults);
    // A checkpoint with a tensor nobody accounts for is a missing feature, named — not silently dropped.
    let mut names: Vec<String> = tensors.names().map(str::to_string).collect();
    names.push("encoder.stages.0.layers.0.surprise.weight".into());
    let extra = TensorIndex::from_names(names);
    let r = analyze(&cfg, Some(&extra), &ReadOptions::default());
    assert_eq!(r.level, Level::C);
    assert!(r.unread_tensors.iter().any(|t| t.contains("surprise")), "{:?}", r.unread_tensors);
    // The real configs read without any tensor.
    for name in ["resnet-18", "resnet-50", "resnet-152"] {
        let cfg: Value = serde_json::from_str(&real(name)).unwrap();
        let r = analyze(&cfg, None, &ReadOptions::default());
        assert_eq!((r.level, &r.result), (Level::B, &ReportResult::Lowerable), "{name}: {}", r.render());
    }
}

/// **Hostile numbers.** A spec is untrusted input (an adapter built it from a config): every size it declares is bounded by
/// arithmetic BEFORE anything is sized on it, the refusal says which bound, and nothing is allocated for what a number says.
#[test]
fn hostile_numbers_in_a_spec_are_refused_by_arithmetic_not_allocated() {
    let conv = |k: usize, stride: usize, pad: usize, dilation: usize, cin: usize, cout: usize| {
        CnnOp::Conv(ConvOp { name: "c".into(), cin, cout, k, stride, pad, dilation, groups: 1, bias: false, bn: None, act: None })
    };
    let net = |h: u32, w: u32, ops: Vec<CnnOp>| CnnSpec { architecture: "Hostile".into(), h, w, mean: [0.0; 3], std: [1.0; 3], ops, out: CnnOut::Map, ignored: vec![], aliases: vec![] };
    let e12 = 1_000_000_000_000usize;
    let cases: Vec<(&str, CnnSpec)> = vec![
        ("a kernel of 10^9", net(32, 32, vec![conv(1_000_000_000, 1, 0, 1, 3, 4)])),
        ("a stride of 10^9", net(32, 32, vec![conv(3, 1_000_000_000, 1, 1, 3, 4)])),
        ("a padding of 10^12", net(32, 32, vec![conv(3, 1, e12, 1, 3, 4)])),
        ("a dilation of 10^9", net(32, 32, vec![conv(3, 1, 1, 1_000_000_000, 3, 4)])),
        ("10^12 output channels", net(32, 32, vec![conv(3, 1, 1, 1, 3, e12)])),
        ("a 16384 x 16384 input", net(16384, 16384, vec![conv(1, 1, 0, 1, 3, 3)])),
        ("a 4096 x 4096 map of 64 channels", net(4096, 4096, vec![conv(1, 1, 0, 1, 3, 64)])),
        ("a window table of 2^24 positions and 3,969 taps", net(4096, 4096, vec![conv(63, 1, 31, 1, 3, 1)])),
        ("5,000 activations", net(8, 8, (0..5000).map(|_| CnnOp::Act(Act::Relu)).collect())),
        ("a max pool of 10^9", net(32, 32, vec![CnnOp::MaxPool { k: 1_000_000_000, stride: 1, pad: 0 }])),
        ("a max pool of stride 0", net(32, 32, vec![CnnOp::MaxPool { k: 3, stride: 0, pad: 0 }])),
        ("a convolution of channels that overflow the taps", net(8, 8, vec![conv(3, 1, 1, 1, 3, 1 << 20), conv(63, 1, 31, 1, 1 << 20, 1)])),
    ];
    for (what, spec) in cases {
        let t0 = std::time::Instant::now();
        let v = std::panic::catch_unwind(|| (spec.validate(), cnn::hl_program(&spec).map(|_| ())));
        let (v, h) = v.unwrap_or_else(|_| panic!("{what}: PANICS"));
        assert!(v.is_err() && h.is_err(), "{what}: not refused (validate {v:?}, hl {h:?})");
        assert!(t0.elapsed().as_secs() < 2, "{what}: took {:?} to refuse", t0.elapsed());
        eprintln!("{what}: {}", v.unwrap_err());
    }
}
