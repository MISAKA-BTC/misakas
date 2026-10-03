//! **`ATTN_CROSS_V1`** (FR-21, Mllama's vision cross-attention): the text stage reads `rows` rows of the projected vision states (a
//! declared input; the tower is not computed) through its cross-attention layers. `tools/gen_mllama_cross_fixture.py` ran
//! transformers' `MllamaForConditionalGeneration` over the tiny checkpoint with seeded random states (`cross.json`).

mod common;

use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::cross::{self, STATES_PARAM, XKV_PARAM};
use misaka_palw_tir_lower::lower::{IntTensor, LowerOpts, lower, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::{fidelity};
use std::sync::Arc;
use misaka_palw_tir_lower::spec::CrossStatesSpec;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{hf_weights, hl, parse_config_str};
use std::path::Path;

pub fn dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf/mllama")
}

fn cross() -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir().join("cross.json")).expect("cross.json")).unwrap()
}

fn states_of(c: &serde_json::Value) -> Vec<Vec<f32>> {
    c["cross_states"].as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect()).collect()
}

/// The float reference with the states bound equals transformers over the whole prompt, every checkpoint tensor read (the cross
/// layers' included), and differs from the text-only stage (the layers matter).
#[test]
fn mllama_with_vision_states_matches_its_hf_fixture() {
    let c = cross();
    let cfg = std::fs::read_to_string(dir().join("config.json")).expect("config");
    let mut spec = parse_config_str(&cfg).expect("mllama reads");
    let rows = c["rows"].as_u64().unwrap() as usize;
    spec.cross_states = Some(CrossStatesSpec { rows });
    let prog = hl::build_program(&spec).expect("hl");
    assert_eq!(prog.schedule.len(), spec.layers.len(), "the cross layer runs");
    let binding = hf_weights::bind(&spec, &prog).expect("bind");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, unused) = ParamStore::from_source(&prog, &binding, &ck).expect("params");
    assert!(unused.is_empty(), "the cross layer's tensors are read: {unused:?}");
    let tokens: Vec<usize> = c["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let mut s = Session::new(&prog, &params);
    s.cross_states = Some(states_of(&c));
    let got = s.run(&tokens).expect("run");
    let want = c["logits_full"].as_array().unwrap();
    let (mut worst, mut scale) = (0f64, 1f64);
    for (g, w) in got.iter().zip(want) {
        for (a, b) in g.iter().zip(w.as_array().unwrap()) {
            let b = b.as_f64().unwrap();
            worst = worst.max((*a as f64 - b).abs());
            scale = scale.max(b.abs());
        }
    }
    assert!(worst <= 1e-4 * scale, "max |dlogit| {worst:e} against transformers (scale {scale})");
    // The text-only stage is another function: the states change the logits.
    spec.cross_states = None;
    let p0 = hl::build_program(&spec).expect("hl text-only");
    let b0 = hf_weights::bind(&spec, &p0).expect("bind");
    let (params0, _) = ParamStore::from_source(&p0, &b0, &ck).expect("params");
    let text = Session::new(&p0, &params0).run(&tokens).expect("run");
    let moved = got.iter().zip(&text).any(|(a, b)| a.iter().zip(b).any(|(x, y)| (x - y).abs() > 1e-3));
    assert!(moved, "the cross layer changed nothing");
}

/// The unit of the declared states: `2^-12` (the tower's rows are decoded integers at a fixed point).
const UNIT: f64 = 1.0 / 4096.0;

struct Stages {
    spec: misaka_palw_tir_lower::spec::ModelSpec,
    text: misaka_palw_tir_lower::lower::Lowered,
    text_params: misaka_palw_tir_lower::lower::IntParams,
    logits_scale: f64,
    kv: misaka_palw_tir_lower::lower::Lowered,
    kv_params: misaka_palw_tir_lower::lower::IntParams,
    states: IntTensor,
    states_f: Vec<Vec<f32>>,
    float_logits: Vec<Vec<f32>>,
    tokens: Vec<usize>,
    hl: misaka_palw_tir_lower::hl::HlProgram,
}

fn stages() -> Stages {
    let c = cross();
    let cfg = std::fs::read_to_string(dir().join("config.json")).expect("config");
    let mut spec = parse_config_str(&cfg).expect("mllama reads");
    let rows = c["rows"].as_u64().unwrap() as usize;
    spec.cross_states = Some(CrossStatesSpec { rows });
    let hl = hl::build_program(&spec).expect("hl");
    let binding = hf_weights::bind(&spec, &hl).expect("bind");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    let d = spec.hidden_size;
    // The declared states: decoded integers at `UNIT`; the float reference reads the same values.
    let raw = states_of(&c);
    let ints: Vec<i32> = raw.iter().flatten().map(|v| (*v as f64 / UNIT).round() as i32).collect();
    let states_f: Vec<Vec<f32>> = ints.chunks(d).map(|r| r.iter().map(|q| (*q as f64 * UNIT) as f32).collect()).collect();
    let tokens: Vec<usize> = c["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let float_logits = {
        let mut s = Session::new(&hl, &params);
        s.cross_states = Some(states_f.clone());
        s.run(&tokens).expect("float run")
    };
    // Calibration: the fixture's prompt and random ones, over the fixture's states and random rows of the same spread.
    let mut stats: std::collections::BTreeMap<String, misaka_palw_tir_lower::float_ref::SiteStat> = Default::default();
    let seqs = fidelity::random_sequences(hl.vocab, 4, 24, 11);
    let mut rng = 17u64;
    let mut rand_rows = || -> Vec<Vec<f32>> {
        (0..rows)
            .map(|_| {
                (0..d)
                    .map(|_| {
                        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                        (((rng >> 33) as f64 / (1u64 << 31) as f64) * 2.0 - 1.0) as f32 * 1.2
                    })
                    .collect()
            })
            .collect()
    };
    let mut runs: Vec<(Vec<usize>, Vec<Vec<f32>>)> = vec![(tokens.clone(), states_f.clone())];
    for s in &seqs {
        runs.push((s.clone(), rand_rows()));
    }
    for (seq, st) in runs {
        let mut s = Session::new(&hl, &params).with_site_stats();
        s.cross_states = Some(st);
        s.run(&seq).expect("calibration run");
        for (k, v) in s.sites.take().unwrap_or_default() {
            stats.entry(k).or_default().merge(&v);
        }
    }
    let quiet = |_: usize, _: usize| {};
    let opts = LowerOpts { max_window: Some(64), ..LowerOpts::default() };
    let text = lower(&hl, &opts).expect("lower the text stage");
    let loader = Resident(Arc::new(params));
    let mat = materialise(&text, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise the text stage");
    let (hl0, b0) = cross::hl_cross_kv(&spec).expect("stage 0 HL");
    let (params0, _) = ParamStore::from_source(&hl0, &b0, &ck).expect("stage 0 params");
    let kv = cross::lower_cross_kv(&hl0, &spec, UNIT).expect("lower stage 0");
    let mat0 = materialise(&kv, &hl0, &Resident(Arc::new(params0)), &stats, &QuantPolicy::default(), &quiet).expect("materialise stage 0");
    Stages {
        spec,
        text,
        text_params: mat.params,
        logits_scale: mat.logits_scale,
        kv,
        kv_params: mat0.params,
        states: IntTensor::i32(vec![rows, d], ints),
        states_f,
        float_logits,
        tokens,
        hl,
    }
}

/// Stage 0's stack through the reference evaluator: `i16 [Dc, 2, rows, inner]`.
fn run_stage0(s: &Stages) -> IntTensor {
    let p = common::with_inputs(&s.kv.program, &s.kv_params, &[(STATES_PARAM, s.states.clone())]);
    let interp = misaka_palw_tir::Interpreter::new(&s.kv.program).expect("interpreter");
    let out = interp.step(&p, &mut misaka_palw_tir::RunState::default(), 0).expect("stage 0 runs");
    let ck = cross::CrossKv::of(&s.spec).unwrap();
    IntTensor::i16(vec![ck.slots.len(), 2, ck.rows, ck.inner()], out.logits.data.iter().map(|v| *v as i16).collect())
}

/// **The two-stage integer program follows the float reference**: stage 0's stack from the declared states, the text stage reading it.
#[test]
fn the_integer_text_stage_follows_the_float_reference_over_the_declared_states() {
    let s = stages();
    let xkv = run_stage0(&s);
    let p = common::with_inputs(&s.text.program, &s.text_params, &[(XKV_PARAM, xkv)]);
    let il = fidelity::int_logits(&s.text.program, &p, &s.tokens, s.logits_scale, &|_| {}).expect("integer run");
    let m = fidelity::compare(&[s.float_logits.clone()], &[il], &[s.tokens.clone()]);
    eprintln!("mllama with states: top-1 {:.3} KL {:.5} (max {:.4})", m.top1_agreement, m.kl_mean, m.kl_max);
    assert!(m.top1_agreement >= 0.8 && m.kl_mean <= 0.02, "integer vs float: {m:?}");
    let _ = (&s.states_f, &s.hl);
}

/// The reference evaluator, the second implementation and the typed backend agree on stage 0 and on the text stage (every commit).
#[test]
fn the_three_implementations_agree_on_both_stages() {
    let s = stages();
    let p0 = common::with_inputs(&s.kv.program, &s.kv_params, &[(STATES_PARAM, s.states.clone())]);
    common::three_ways(&s.kv.program, &p0, &[vec![0]]).expect("stage 0");
    let xkv = run_stage0(&s);
    let p = common::with_inputs(&s.text.program, &s.text_params, &[(XKV_PARAM, xkv)]);
    common::three_ways(&s.text.program, &p, &[s.tokens.clone()]).expect("text stage");
}
