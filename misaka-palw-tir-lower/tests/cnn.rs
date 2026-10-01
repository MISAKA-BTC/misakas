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
        CnnOp::Conv(ConvOp { name: name.into(), cin, cout, k, stride, pad: k / 2, dilation: 1, groups: 1, bias: false, bn: bn(bnname.into()), act, ..ConvOp::default() })
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

/// **No param is read by two blocks but a window table.** A param of one name, dtype and shape is declared once and shared by
/// every block that asks for it (`decl`), so two blocks that name a value alike (`out.rq1`) silently used each other's
/// multiplier; only the pinned `Idx` tables of a window (pure geometry) are meant to be shared.
fn assert_blocks_share_no_params(name: &str, p: &tir::TirProgramV1) {
    let mut readers: std::collections::BTreeMap<u16, std::collections::BTreeSet<usize>> = Default::default();
    for (bi, blk) in p.blocks.iter().enumerate() {
        for nd in &blk.nodes {
            for r in &nd.inputs {
                if let tir::Ref::Param(j) = r {
                    readers.entry(*j).or_default().insert(bi);
                }
            }
        }
    }
    let shared: Vec<String> = readers.iter().filter(|(j, b)| b.len() > 1 && !p.params[**j as usize].name.starts_with("cnn.idx.")).map(|(j, b)| format!("{} (blocks {b:?})", p.params[*j as usize].name)).collect();
    assert!(shared.is_empty(), "{name}: params read by several blocks: {shared:?}");
}

/// The five steps on one ResNet fixture (the spec written out in Rust); the output rows are compared to `key` of the fixture.
fn check(name: &str, out: CnnOut, key: &str) -> usize {
    let fx = load(name);
    let spec = resnet_spec(&fx.cfg, fx.size, fx.ms, out);
    check_spec(name, &fx, spec, key, 0.995)
}

/// The same steps on a network a data adapter builds — there is no Rust oracle for it: transformers is the oracle, and the
/// float reference's agreement with it (step 1) is what says the adapter's structure and names are right. `map` asks for the
/// last feature map (the adapter's own output is the pooled vector).
fn check_adapter(name: &str, map: bool, key: &str) -> usize {
    check_adapter_min(name, map, key, 0.995)
}

/// [`check_adapter`] with the integer program's cosine floor stated: a network of many layers carries the weight codes' (int8,
/// per output row) rounding through every one of them — about 1% in cosine over MobileNetV2's fifty convolutions — so its floor is
/// lower than a ResNet's; the SAME network with its weights on the int8 grid (`*_grid` fixtures) has none of that noise and its
/// floor is 0.9995: what is left is the activations' 16-bit codes and the requantisations, the lowering's own error.
fn check_adapter_min(name: &str, map: bool, key: &str, min_cos: f64) -> usize {
    let fx = load(name);
    let text = std::fs::read_to_string(fixture_dir(name).join("config.json")).expect("config");
    let mut spec = cnn::parse_cnn(&text, Some(fx.size), None).unwrap_or_else(|e| panic!("{name}: the adapter: {e}"));
    assert_eq!((spec.mean, spec.std), fx.ms, "{name}: the adapter's normalisation is the one the fixture was made with");
    if map {
        if let CnnOut::GlobalAvgNorm { name: n, .. } = &spec.out {
            spec.ignored.push(format!("{n}."));
        }
        spec.out = CnnOut::Map;
    }
    check_spec(name, &fx, spec, key, min_cos)
}

fn check_spec(name: &str, fx: &Fixture, spec: CnnSpec, key: &str, min_cos: f64) -> usize {
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
    assert_blocks_share_no_params(name, &lw.program);
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
        assert!(min > min_cos, "{name}: cosine min {min} (floor {min_cos})");
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

/// **Two layer blocks**: the carry is read by one block and written by the next, and no site name is shared between blocks (a
/// requantisation's params are named after its site and `decl` shares a param of one name, dtype and shape — blocks that named
/// their boundary value alike used each other's multiplier, which only a network with two layer blocks shows).
#[test]
fn a_resnet_with_two_layer_blocks_matches_its_hf_fixture() {
    let blocks = check("resnet_deeper", CnnOut::Map, "last_hidden_state");
    assert!(blocks >= 4, "the network fits {blocks} blocks — it has no two layer blocks");
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

// ───────────────────────────── ConvNeXt and MobileNet: data adapters, no Rust structure ─────────────────────────────

/// **ConvNeXt as data** (`adapters/convnext.json`): a channel LayerNorm (the encoder's LayerNorm over rows), a 7x7 depthwise
/// convolution, pointwise convolutions as 1x1 ones with GELU as a table, the learned layer scale folded into the projection,
/// a 2x2 stride-2 downsampling between stages and the pooled vector through the final LayerNorm. transformers is the oracle for
/// both the last feature map and the pooled output; no primitive, no new lowering code path but the channel norm's wiring.
#[test]
fn a_convnext_matches_its_hf_fixture() {
    check_adapter("convnext", true, "last_hidden_state");
    check_adapter("convnext", false, "pooler_output");
}

/// A ConvNeXt deep enough for layer blocks: the carry crosses block boundaries between stages as a padded flat activation, and
/// every block has its own norm and layer-scale params (none shared).
#[test]
fn a_deep_convnext_crosses_block_boundaries_with_a_padded_carry() {
    let blocks = check_adapter("convnext_deep", false, "pooler_output");
    assert!(blocks >= 3, "the deep ConvNeXt fits pre and post only ({blocks} blocks) — the carry is not exercised");
}

/// **MobileNetV2 as data** (`adapters/mobilenet-v2.json`): inverted residuals with linear bottlenecks, ReLU6 (a narrowing into
/// its fixed unit), BN folded, TensorFlow "SAME" padding — asymmetric at the stride-2 layers over an even extent and symmetric
/// over an odd one (the 60x60 image's maps are 30, 15, 8, 4, 2 wide) — and the symmetric-padding variant without the expansion
/// layer in the stem.
#[test]
fn a_mobilenet_v2_matches_its_hf_fixture() {
    check_adapter_min("mobilenet_v2", true, "last_hidden_state", 0.98);
    check_adapter_min("mobilenet_v2", false, "pooler_output", 0.98);
    check_adapter_min("mobilenet_v2_sym", false, "pooler_output", 0.98);
}

/// **A deep network on the int8 grid is exact up to the activations' rounding**: MobileNetV2 (fifty convolutions, ReLU6, TF
/// padding, residual adds) and ConvNeXt with every weight row on the lowering's per-row int8 grid have no weight-quantisation
/// error, and the integer program is then within 0.9995 in cosine of transformers' float outputs — the distance in the plain
/// fixtures is the weight codes' rounding, not the lowering's.
#[test]
fn a_deep_network_with_weights_on_the_int8_grid_follows_transformers_to_activation_rounding() {
    check_adapter_min("mobilenet_v2_grid", true, "last_hidden_state", 0.9995);
    check_adapter_min("mobilenet_v2_grid", false, "pooler_output", 0.9995);
    check_adapter_min("convnext_grid", true, "last_hidden_state", 0.9995);
    check_adapter_min("convnext_grid", false, "pooler_output", 0.9995);
}

/// **MobileNetV1 as data** (`adapters/mobilenet-v1.json`): thirteen depthwise-separable pairs, no residuals, ReLU6 everywhere.
#[test]
fn a_mobilenet_v1_matches_its_hf_fixture() {
    check_adapter("mobilenet_v1", true, "last_hidden_state");
    check_adapter("mobilenet_v1", false, "pooler_output");
}

/// **The adapters read the REAL architectures**: for each real config (the hub's `config.json` of ConvNeXt tiny, base and large at
/// 384, MobileNetV2 at depth multipliers 1.0, 0.75 and 1.4, MobileNetV1 at 1.0 and 0.75) and the tensor names and shapes of the
/// model transformers' own code builds from it (`tools/gen_cnn_shapes.py`, the meta device: no weights), the architecture
/// report finds Level B, every tensor accounted for and every parameter at the shape the lowering needs. A channel count the
/// adapter computes differently from HF's `make_divisible` would be a shape error here.
#[test]
fn real_convnext_and_mobilenet_configs_account_for_every_tensor_at_its_real_shape() {
    use misaka_palw_tir_lower::hf_schema::{Level, ReadOptions, TensorIndex};
    use misaka_palw_tir_lower::model::{ReportResult, analyze};
    for name in REAL_CNN_FAMILIES {
        let cfg: Value = serde_json::from_str(&real(name)).unwrap();
        let shapes: std::collections::BTreeMap<String, Vec<usize>> = serde_json::from_str(&real(&format!("{name}.shapes"))).unwrap();
        let n = shapes.len();
        let r = analyze(&cfg, Some(&TensorIndex::from_shapes(shapes)), &ReadOptions::default());
        assert_eq!((r.level, &r.result), (Level::B, &ReportResult::Lowerable), "{name}: {}", r.render());
        assert!(r.unread_tensors.is_empty() && r.weight_errors.is_empty(), "{name}: {:?} {:?}", r.unread_tensors, r.weight_errors);
        assert!(!r.new_consensus_primitive_required && !r.new_court_kernel_required, "{name}");
        eprintln!("{name}: Level B, {n} tensors accounted for; features {}", r.features.iter().map(|f| f.id.as_str()).collect::<Vec<_>>().join(" "));
    }
}

const REAL_CNN_FAMILIES: [&str; 8] =
    ["convnext-tiny-224", "convnext-base-224", "convnext-large-384", "mobilenet_v2_1.0_224", "mobilenet_v2_0.75_160", "mobilenet_v2_1.4_224", "mobilenet_v1_1.0_224", "mobilenet_v1_0.75_192"];

/// **Admission at the real shapes** (no weights read): each real ConvNeXt and MobileNet lowers to a handful of blocks inside the
/// normal form's caps; its one-stage pipeline is admitted at the legacy court's ceilings, or refused by NAME of the ceiling it
/// exceeds (the verdicts are printed; a refusal is a finding about the class's ceilings, not a missing feature).
#[test]
fn real_convnexts_and_mobilenets_lower_and_are_admitted_or_refused_by_name() {
    let mut verdicts = Vec::new();
    for name in REAL_CNN_FAMILIES {
        let spec = cnn::parse_cnn(&real(name), None, None).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (hl, _) = cnn::hl_program(&spec).expect("hl");
        let lw = cnn::lower_cnn(&hl, &spec).unwrap_or_else(|e| panic!("{name}: lower: {e}"));
        let p2 = encoder::vision_v2(&lw).expect("v2");
        let nodes: usize = p2.blocks.iter().map(|b| b.nodes.len()).sum();
        let most = p2.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0);
        assert!(most <= 512 && p2.blocks.len() <= 16, "{name}: {} blocks, largest {most}", p2.blocks.len());
        let pipe = encoder::vision_pipeline();
        let v = match tir::admit_v2::tir_admit_pipeline_v1(&pipe.encode(), &[p2.encode()], &misaka_palw_tir_lower::admission::default_inputs(), &tir::admit_v2::TirJobCeilingsV1::open_v1()) {
            Ok(pa) => format!(
                "ADMITTED — {}x{} px, {} blocks, {nodes} nodes (largest {most}), job {:.3e} MACs, {} step leaves, cone work {}",
                spec.h,
                spec.w,
                p2.blocks.len(),
                pa.job_cost.macs as f64,
                pa.job_step_leaves,
                pa.cone_work
            ),
            Err(e) => format!("REFUSED — {}x{} px, {} blocks, {nodes} nodes: {e}", spec.h, spec.w, p2.blocks.len()),
        };
        eprintln!("{name}: {v}");
        verdicts.push((name, v));
    }
    for (name, v) in &verdicts {
        // Every refusal names a ceiling; none is "internal".
        assert!(v.starts_with("ADMITTED") || v.contains("ceiling") || v.contains("exceed") || v.contains("cap") || v.contains("budget") || v.contains("too"), "{name}: {v}");
    }
}

/// **The features a MobileNet-shaped network composes that transformers has no fixture for** — TensorFlow "SAME" padding at a
/// stride of 3 over odd extents, HardSwish and HardSigmoid as tables, ReLU6 fused, standalone and as a residual's activation, a
/// channel LayerNorm, a layer-scaled convolution and the pooled output through a LayerNorm — against the naive reference: the float
/// reference to 1e-5, the integer program by cosine, and the three implementations plus the court agree on the version-1 view.
#[test]
fn a_network_with_tf_padding_hard_activations_channel_norm_and_layer_scale_follows_a_naive_reference() {
    use rand::{Rng, SeedableRng};
    let bn = |n: &str| Some(BnOp { name: format!("{n}.bn"), eps: 1e-3 });
    let conv = |name: &str, cin, cout, k, stride, groups, bias, act: Option<Act>| ConvOp {
        name: name.into(),
        cin,
        cout,
        k,
        stride,
        groups,
        bias,
        bn: if bias { None } else { bn(name) },
        act,
        tf_same: true,
        ..ConvOp::default()
    };
    let mut scaled = conv("b4.scaled", 16, 16, 5, 3, 1, true, Some(Act::Relu6));
    scaled.layer_scale = Some("b4.ls".into());
    let ops = vec![
        CnnOp::Conv(conv("stem", 3, 8, 3, 2, 1, false, Some(Act::Relu6))), // 17 -> 9: odd extent, pad (1, 1)
        CnnOp::Conv(conv("b1.dw", 8, 8, 3, 1, 8, false, Some(Act::Relu6))),
        CnnOp::Conv(ConvOp { k: 1, tf_same: false, ..conv("b1.pw", 8, 16, 1, 1, 1, false, Some(Act::HardSwish)) }),
        CnnOp::Residual {
            main: vec![CnnOp::Conv(conv("b2.dw", 16, 16, 3, 1, 16, false, Some(Act::Relu6))), CnnOp::Conv(ConvOp { k: 1, tf_same: false, ..conv("b2.pw", 16, 16, 1, 1, 1, false, None) })],
            shortcut: vec![],
            act: Some(Act::Relu6),
        },
        CnnOp::Act(Act::HardSwish),
        CnnOp::Act(Act::Relu6),
        CnnOp::ChannelNorm { name: "b3.norm".into(), eps: 1e-6 },
        CnnOp::Conv(scaled), // 9 -> 3 at stride 3: pad (1, 1)
        CnnOp::Conv(conv("b5.dw", 16, 16, 3, 2, 16, false, Some(Act::HardSigmoid))), // 3 -> 2: odd extent at stride 2
    ];
    let spec = CnnSpec {
        architecture: "SyntheticMobileNetShaped".into(),
        h: 17,
        w: 17,
        mean: [0.5; 3],
        std: [0.5; 3],
        ops,
        out: CnnOut::GlobalAvgNorm { name: "head.norm".into(), eps: 1e-5 },
        ignored: vec![],
        aliases: vec![],
    };
    spec.validate().expect("valid");
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(23);
    let mut tensors = Vec::new();
    walk_convs(&spec.ops, &mut |c| tensors.extend(random_conv_tensors(&mut rng, c)));
    let vec_of = |rng: &mut rand_chacha::ChaCha8Rng, lo: f32, hi: f32| -> Vec<f32> { (0..16).map(|_| rng.gen_range(lo..hi)).collect() };
    tensors.push(("b3.norm.weight".into(), vec![16], vec_of(&mut rng, 0.6, 1.4)));
    tensors.push(("b3.norm.bias".into(), vec![16], vec_of(&mut rng, -0.3, 0.3)));
    tensors.push(("b4.ls".into(), vec![16], vec_of(&mut rng, 0.3, 1.2)));
    tensors.push(("head.norm.weight".into(), vec![16], vec_of(&mut rng, 0.6, 1.4)));
    tensors.push(("head.norm.bias".into(), vec![16], vec_of(&mut rng, -0.3, 0.3)));
    let dir = std::env::temp_dir().join(format!("cnn-mobile-shaped-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    write_safetensors(&dir.join("model.safetensors"), &tensors);
    let by_name: std::collections::BTreeMap<String, Vec<f32>> = tensors.iter().map(|(n, _, d)| (n.clone(), d.clone())).collect();
    let naive = Naive { t: &by_name };
    let (hl, binding) = cnn::hl_program(&spec).expect("hl");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, unused) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    assert!(unused.is_empty(), "{unused:?}");
    let images = calib_images(&spec, 3);
    let reference = |img: &[u8]| -> Vec<Vec<f64>> {
        let x: Vec<Vec<f64>> = (0..(spec.h * spec.w) as usize).map(|p| (0..3).map(|ch| (img[p * 3 + ch] as f64 / 255.0 - spec.mean[ch]) / spec.std[ch]).collect()).collect();
        let (m, hw) = naive.ops(&spec.ops, x, (spec.h as usize, spec.w as usize));
        assert_eq!(hw, (2, 2), "the maps shrink 17, 9, 9, 3, 2 under TensorFlow padding");
        let n = m.len() as f64;
        let mean: Vec<f64> = (0..16).map(|c| m.iter().map(|r| r[c]).sum::<f64>() / n).collect();
        vec![naive.layer_norm(&mean, "head.norm", 1e-5)]
    };
    for img in &images {
        let got = cnn::float_forward(&hl, &spec, &params_f, img, None).expect("float");
        let want = reference(img);
        assert_eq!((got.len(), got[0].len()), (1, 16), "shape");
        let r = rel(&got.concat(), &want.concat());
        eprintln!("mobile-shaped net: float reference vs the naive reference: rel {r:.2e}");
        assert!(r < 1e-5, "float vs naive rel {r}");
    }
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
        let t = tir::Tensor::new(tir::DType::I16, vec![17, 17, 3], img.iter().map(|v| *v as i128).collect()).unwrap();
        inputs.constant.insert(0, t);
        let run = interp.run_positions(&params2, &inputs, 1).expect("v2 run");
        let want = reference(img);
        let got: Vec<Vec<f64>> = run[0].output.data.chunks(16).map(|r| r.iter().map(|c| *c as f64 * mat.logits_scale).collect()).collect();
        let c = cosine(&got[0], &want[0]);
        eprintln!("mobile-shaped net: integer vs the naive reference: cosine {c:.6}, rel {:.2e}", rel(&got.concat(), &want.concat()));
        assert!(c > 0.99, "cosine {c}");
    }
    // The three implementations and the court on the version-1 view.
    let img = &images[0];
    let t = misaka_palw_tir_lower::lower::IntTensor::i16(vec![17, 17, 3], img.iter().map(|v| *v as i16).collect());
    let p6 = common::with_inputs(&lw.program, &mat.params, &[(cnn::IMAGE_PARAM, t)]);
    common::three_ways(&lw.program, &p6, &[vec![0]]).unwrap_or_else(|e| panic!("three implementations: {e}"));
    let c = common::court_coverage(&lw.program, &p6, &[0], &[0], &[1]).unwrap_or_else(|e| panic!("court: {e}"));
    eprintln!("mobile-shaped net COURT: {} commit points over {} primitives", c.commits, c.primitives.len());
    assert!(c.commits > 0);
    tir::admit_v2::tir_admit_program_v2(&p2, &misaka_palw_tir_lower::admission::default_inputs()).expect("admitted");
    let _ = std::fs::remove_dir_all(dir);
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
    let kw = c.kw.unwrap_or(c.k);
    let taps = c.cin / c.groups * c.k * kw;
    let scale = 1.7 / (taps as f32).sqrt();
    let mut v = vec![(format!("{}.weight", c.name), vec![c.cout, c.cin / c.groups, c.k, kw], (0..c.cout * taps).map(|_| rng.gen_range(-1.0f32..1.0) * scale).collect::<Vec<_>>())];
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
            Some(Act::Relu6) => x.clamp(0.0, 6.0),
            Some(Act::HardSwish) => x * (x + 3.0).clamp(0.0, 6.0) / 6.0,
            Some(Act::HardSigmoid) => (x + 3.0).clamp(0.0, 6.0) / 6.0,
            other => panic!("the naive reference has no {other:?}"),
        }
    }

    fn conv(&self, x: &[Vec<f64>], (h, w): (usize, usize), c: &ConvOp) -> (Vec<Vec<f64>>, (usize, usize)) {
        // Each axis has its own kernel, stride, padding and dilation (the width's default to the height's).
        let (kh, kw) = (c.k, c.kw.unwrap_or(c.k));
        let (sh, sw) = (c.stride, c.stride_w.unwrap_or(c.stride));
        let (ph, pw) = (c.pad, c.pad_w.unwrap_or(c.pad));
        let (dh, dw) = (c.dilation, c.dilation_w.unwrap_or(c.dilation));
        // TensorFlow "SAME": along an axis of extent n, kernel k, stride s the padding is max(k - s, 0) when n is a multiple of
        // s and max(k - n mod s, 0) otherwise, the smaller half before (written from the HF function, not from the lowering's).
        let tf = |n: usize, k: usize, s: usize| {
            let along = if n % s == 0 { k.saturating_sub(s) } else { k.saturating_sub(n % s) };
            (along / 2, along - along / 2)
        };
        let ((ph0, ph1), (pw0, pw1)) = if c.tf_same { (tf(h, kh, sh), tf(w, kw, sw)) } else { ((ph, ph), (pw, pw)) };
        let (ho, wo) = ((h + ph0 + ph1 - ((kh - 1) * dh + 1)) / sh + 1, (w + pw0 + pw1 - ((kw - 1) * dw + 1)) / sw + 1);
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
                        for ky in 0..kh {
                            for kx in 0..kw {
                                let (iy, ix) = ((oy * sh + ky * dh) as isize - ph0 as isize, (ox * sw + kx * dw) as isize - pw0 as isize);
                                if iy < 0 || ix < 0 || iy >= h as isize || ix >= w as isize {
                                    continue;
                                }
                                acc += x[iy as usize * w + ix as usize][g * cin_g + ci] * wt[((co * cin_g + ci) * kh + ky) * kw + kx] as f64;
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
                    if let Some(ls) = &c.layer_scale {
                        acc *= self.t[ls][co] as f64;
                    }
                    out[oy * wo + ox][co] = Self::act(c.act, acc);
                }
            }
        }
        (out, (ho, wo))
    }

    /// `(x − μ)/√(σ² + ε)·γ + β` over one row, γ and β the checkpoint's `{name}.weight` and `{name}.bias`.
    fn layer_norm(&self, r: &[f64], name: &str, eps: f64) -> Vec<f64> {
        let n = r.len() as f64;
        let mu = r.iter().sum::<f64>() / n;
        let var = r.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / n;
        let (g, b) = (&self.t[&format!("{name}.weight")], &self.t[&format!("{name}.bias")]);
        r.iter().enumerate().map(|(i, v)| (v - mu) / (var + eps).sqrt() * g[i] as f64 + b[i] as f64).collect()
    }

    fn ops(&self, ops: &[CnnOp], mut x: Vec<Vec<f64>>, mut hw: (usize, usize)) -> (Vec<Vec<f64>>, (usize, usize)) {
        for op in ops {
            match op {
                CnnOp::Conv(c) => (x, hw) = self.conv(&x, hw, c),
                CnnOp::Act(a) => x = x.iter().map(|r| r.iter().map(|v| Self::act(Some(*a), *v)).collect()).collect(),
                CnnOp::ChannelNorm { name, eps } => x = x.iter().map(|r| self.layer_norm(r, name, *eps)).collect(),
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
        ConvOp { name: name.into(), cin, cout, k, stride, pad, dilation, groups, bias, bn, act, ..ConvOp::default() }
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

/// **A 1-D convolution stack** (an audio front end's stem: `Conv1d` k3 s1 then k3 s2, a bias and a GELU each) is the same
/// lowering over a `[1, T]` map with a kernel only along the width: float against the naive direct convolution to 1e-7, the
/// integer program by cosine.
#[test]
fn a_one_dimensional_convolution_stack_follows_a_naive_reference() {
    use rand::SeedableRng;
    let mut c1 = ConvOp::conv1d("stem1", 3, 8, 3, 1, 1, true);
    let mut c2 = ConvOp::conv1d("stem2", 8, 8, 3, 2, 1, true);
    c1.act = Some(Act::Silu);
    c2.act = Some(Act::Silu);
    let spec = CnnSpec {
        architecture: "Synthetic1d".into(),
        h: 1,
        w: 32,
        mean: [0.5; 3],
        std: [0.25; 3],
        ops: vec![CnnOp::Conv(c1), CnnOp::Conv(c2)],
        out: CnnOut::Map,
        ignored: vec![],
        aliases: vec![],
    };
    spec.validate().expect("valid");
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(11);
    let mut tensors = Vec::new();
    walk_convs(&spec.ops, &mut |c| {
        let mut t = random_conv_tensors(&mut rng, c);
        // The checkpoint's Conv1d weight is [cout, cin, k]: the same flat bytes as [cout, cin, 1, k].
        t[0].1 = vec![c.cout, c.cin, c.kw.unwrap_or(c.k)];
        tensors.extend(t);
    });
    let dir = std::env::temp_dir().join(format!("cnn-1d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    write_safetensors(&dir.join("model.safetensors"), &tensors);
    let by_name: std::collections::BTreeMap<String, Vec<f32>> = tensors.iter().map(|(n, _, d)| (n.clone(), d.clone())).collect();
    let naive = Naive { t: &by_name };
    let (hl, binding) = cnn::hl_program(&spec).expect("hl");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, unused) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    assert!(unused.is_empty(), "{unused:?}");
    let images = calib_images(&spec, 3);
    let reference = |img: &[u8]| -> Vec<Vec<f64>> {
        let x: Vec<Vec<f64>> = (0..32).map(|p| (0..3).map(|ch| (img[p * 3 + ch] as f64 / 255.0 - spec.mean[ch]) / spec.std[ch]).collect()).collect();
        naive.ops(&spec.ops, x, (1, 32)).0
    };
    for img in &images {
        let got = cnn::float_forward(&hl, &spec, &params_f, img, None).expect("float");
        let want = reference(img);
        assert_eq!((got.len(), got[0].len()), (16, 8), "shape");
        let r = rel(&got.concat(), &want.concat());
        eprintln!("1-D stack: float reference vs the naive convolution: rel {r:.2e}");
        assert!(r < 1e-5, "float vs naive rel {r}");
    }
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
        let t = tir::Tensor::new(tir::DType::I16, vec![1, 32, 3], img.iter().map(|v| *v as i128).collect()).unwrap();
        inputs.constant.insert(0, t);
        let run = interp.run_positions(&params2, &inputs, 1).expect("v2 run");
        let want = reference(img);
        let got: Vec<Vec<f64>> = run[0].output.data.chunks(8).map(|r| r.iter().map(|c| *c as f64 * mat.logits_scale).collect()).collect();
        let cos: Vec<f64> = got.iter().zip(&want).map(|(a, b)| cosine(a, b)).collect();
        let min = cos.iter().cloned().fold(1.0, f64::min);
        eprintln!("1-D stack: integer vs the naive reference: cosine min {min:.6}, rel {:.2e}", rel(&got.concat(), &want.concat()));
        assert!(min > 0.99, "cosine min {min}");
    }
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
            CnnOp::Conv(ConvOp { name: "stem".into(), cin: 3, cout: 4, k: 1, stride: 1, pad: 0, dilation: 1, groups: 1, bias: false, bn: None, act: None, ..ConvOp::default() }),
            CnnOp::Conv(ConvOp { name: "g".into(), cin: 4, cout: 8, k: 3, stride: 1, pad: 1, dilation: 1, groups: 2, bias: false, bn: None, act: None, ..ConvOp::default() }),
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
        CnnOp::Conv(ConvOp { name: "c".into(), cin, cout, k, stride, pad, dilation, groups: 1, bias: false, bn: None, act: None, ..ConvOp::default() })
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
