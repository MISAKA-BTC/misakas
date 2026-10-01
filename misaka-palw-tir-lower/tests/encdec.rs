//! **Encoder-decoder models as two-stage text pipelines** against their Hugging Face fixtures
//! (`tools/gen_hf_encdec_fixtures.py`): T5 (ReLU, tied and scaled; gated-GELU, untied), BART,
//! mBART, Marian and Pegasus. For each tiny model:
//! 1. the float encoder and decoder against HF: the encoder's last hidden rows, and the decoder's
//!    logits over HF's greedy stream and over a random stream (T5: the relative-position buckets
//!    against HF's own `_relative_position_bucket`);
//! 2. calibration on random sources and streams;
//! 3. the two programs, the encoder `Final` (every decoder layer's cross K/V) and the decoder the
//!    text stage, in one pipeline: the source bound by `JobTokens`/`JobTokenCount`, the K/V by
//!    `StageFinal`;
//! 4. `run_text_pipeline` with a greedy selector against HF's `generate`, the replay, and the
//!    teacher-forced logits against HF's (top-1, KL);
//! 5. admission.
//!
//! The job's source ids ride in `PipelineJob::negative` (`TokenSource::Negative`): the IR has no
//! second token list of its own (see hf-coverage §17).

mod common;

use common::{court_coverage, int_tensor, three_ways, with_inputs};
use misaka_palw_tir as tir;
use misaka_palw_tir_lower::encoder;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::encdec::{self, Stats};
use misaka_palw_tir_lower::lower::{IntTensor, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::{Checkpoint, TensorSource};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const LMAX: u32 = 16;
const MAX_TRIP: u32 = 32;
const WINDOW: u32 = 64;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-encdec").join(name)
}

fn rows_of(v: &serde_json::Value) -> Vec<Vec<f64>> {
    v.as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()).collect()
}

fn ids_of(v: &serde_json::Value) -> Vec<usize> {
    v.as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect()
}

fn rel(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt() / b.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// The lowest id among the largest values: HF's greedy `argmax`.
fn argmax<T: PartialOrd + Copy>(v: &[T]) -> usize {
    v.iter().enumerate().fold(0, |b, (i, x)| if *x > v[b] { i } else { b })
}

fn kl(p: &[f64], q: &[f64]) -> f64 {
    let lse = |v: &[f64]| {
        let m = v.iter().cloned().fold(f64::MIN, f64::max);
        m + v.iter().map(|x| (x - m).exp()).sum::<f64>().ln()
    };
    let (lp, lq) = (lse(p), lse(q));
    p.iter().zip(q).map(|(a, b)| (a - lp).exp() * ((a - lp) - (b - lq))).sum()
}

fn margin(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| b.partial_cmp(a).unwrap());
    s[0] - s[1]
}

struct ByProgram<'a>(Vec<&'a dyn tir::ParamSource>);
impl tir::pipeline::PipelineParams for ByProgram<'_> {
    fn params(&self, program: u16) -> &dyn tir::ParamSource {
        self.0[program as usize]
    }
}

struct NoDraws;
impl tir::pipeline::RandomSource for NoDraws {
    fn random(&self, _: u16, _: tir::program_v2::RandomDist, _: u32, _: &[u32]) -> Option<tir::Tensor> {
        None
    }
}

/// HF's top-2 margin at a row, over the integer stage's own RMS logit error there: below 2, the
/// row is a tie within the integer program's noise.
fn tie_ratio(row: &[f64], want: &[f64]) -> f64 {
    let noise = (row.iter().zip(want).map(|(a, b)| (a - b) * (a - b)).sum::<f64>() / row.len() as f64).sqrt();
    margin(want) / noise.max(1e-12)
}

/// Teacher-forced integer logits `[T][V]` against HF's: top-1 agreements, KL sum, the rows (as
/// `[T][V]` floats), and [`tie_ratio`] at every disagreement.
fn compare(name: &str, what: &str, out: &tir::Tensor, scale: f64, hf: &[Vec<f64>]) -> (usize, f64, Vec<Vec<f64>>, Vec<f64>) {
    assert_eq!(out.shape[0], hf.len(), "{name} {what}: positions");
    let v = out.shape[1];
    let all: Vec<f64> = out.data.iter().map(|c| *c as f64 * scale).collect();
    let r = rel(&all, &hf.concat());
    let (mut agree, mut kls, mut ties) = (0, 0f64, vec![]);
    for (row, want) in all.chunks(v).zip(hf) {
        kls += kl(want, row);
        if argmax(row) == argmax(want) {
            agree += 1;
        } else {
            ties.push(tie_ratio(row, want));
        }
    }
    eprintln!("{name} {what}: logits rel {r:.2e}, top-1 {agree}/{}, mean KL {:.2e}", hf.len(), kls / hf.len() as f64);
    assert!(r < 0.05, "{name} {what}: logits rel {r}");
    (agree, kls, all.chunks(v).map(<[f64]>::to_vec).collect(), ties)
}

fn check(name: &str) {
    use rand::{Rng, SeedableRng};
    use tir::pipeline::{PipelineJob, TextSelectV1, TokenPad, TokenRule, TokenSource};
    let dir = fixture_dir(name);
    let cfg = std::fs::read_to_string(dir.join("config.json")).expect("config");
    let o: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).expect("outputs")).expect("json");
    let s = encdec::parse_encdec(&cfg).expect("parse");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let names: std::collections::BTreeSet<String> = ck.names().into_iter().collect();
    let has = |n: &str| names.contains(n);
    let ((ehl, ebind), (dhl, dbind)) = encdec::hl_programs(&s, LMAX as usize, &has).expect("hl programs");
    let (ep, e_unread) = ParamStore::from_source(&ehl, &ebind, &ck).expect("encoder params");
    let (dp, d_unread) = ParamStore::from_source(&dhl, &dbind, &ck).expect("decoder params");
    let unread: Vec<&String> = e_unread.iter().filter(|n| d_unread.contains(n)).collect();
    assert!(unread.is_empty(), "{name}: checkpoint tensors neither stage reads: {unread:?}");
    // T5's buckets against HF's `_relative_position_bucket`.
    if let encdec::Positions::Relative { buckets, max_distance } = s.positions {
        for (key, bidi) in [("buckets_bidirectional", true), ("buckets_causal", false)] {
            let b = &o[key];
            let from = b["from"].as_i64().unwrap();
            for (i, want) in ids_of(&b["buckets"]).iter().enumerate() {
                let r = from + i as i64;
                assert_eq!(encdec::t5_bucket(r, bidi, buckets, max_distance), *want, "{name}: bucket of {r} ({key})");
            }
        }
    }
    // 1. The float stages are HF's.
    let recs = o["records"].as_array().unwrap();
    for rec in recs {
        let src = ids_of(&rec["input_ids"]);
        let enc = encdec::float_encoder(&ehl, &s, &ep, &src, None).expect("float encoder");
        let r = rel(&enc.hidden.concat(), &rows_of(&rec["encoder_hidden"]).concat());
        assert!(r < 1e-4, "{name}: float encoder vs HF rel {r}");
        for (ids_key, logits_key) in [("decoder_input_ids", "logits"), ("random_decoder_ids", "random_logits")] {
            let lg = encdec::float_decoder(&dhl, &s, &dp, &enc, &ids_of(&rec[ids_key]), None).expect("float decoder");
            let r2 = rel(&lg.concat(), &rows_of(&rec[logits_key]).concat());
            eprintln!("{name} float vs HF ({} source ids): encoder rows rel {r:.2e}, decoder `{logits_key}` rel {r2:.2e}", src.len());
            assert!(r2 < 1e-4, "{name}: float decoder vs HF `{logits_key}` rel {r2}");
        }
    }
    // 2. Calibration: random sources and decoder streams.
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);
    let (mut es, mut ds) = (Stats::new(), Stats::new());
    for _ in 0..12 {
        let n = rng.gen_range(3..=LMAX as usize);
        let src: Vec<usize> = (0..n).map(|_| rng.gen_range(0..s.vocab)).collect();
        let enc = encdec::float_encoder(&ehl, &s, &ep, &src, Some(&mut es)).expect("calibration encoder");
        let m = rng.gen_range(4..=16usize);
        let stream: Vec<usize> = std::iter::once(s.decoder_start as usize).chain((1..m).map(|_| rng.gen_range(0..s.vocab))).collect();
        encdec::float_decoder(&dhl, &s, &dp, &enc, &stream, Some(&mut ds)).expect("calibration decoder");
    }
    // 3. The two programs and the pipeline.
    let quiet = |_: usize, _: usize| {};
    let elw = encdec::lower_encoder(&ehl, &s, LMAX).expect("lower encoder");
    let dlw = encdec::lower_decoder(&dhl, &s, LMAX, WINDOW).expect("lower decoder");
    let emat = materialise(&elw, &ehl, &Resident(Arc::new(ep)), &es, &QuantPolicy::default(), &quiet).expect("materialise encoder");
    let dmat = materialise(&dlw, &dhl, &Resident(Arc::new(dp)), &ds, &QuantPolicy::default(), &quiet).expect("materialise decoder");
    let e2 = encoder::encdec_encoder_v2(&elw, s.vocab as u32, LMAX).expect("encoder v2");
    let d2 = encoder::encdec_decoder_v2(&dlw, LMAX).expect("decoder v2");
    let e_params = encoder::lifted_params(&elw.program, &[encdec::IDS_PARAM, encdec::COUNT_PARAM], &emat.params);
    let d_params = encoder::lifted_params(&dlw.program, &[encdec::XKV_PARAM, encdec::ENC_COUNT_PARAM], &dmat.params);
    let rule = TokenRule { prefix: vec![], source: TokenSource::Negative, suffix: vec![], pad: Some(TokenPad { id: 0, to_len: LMAX }) };
    let pipe = encoder::encdec_pipeline(rule, MAX_TRIP);
    let programs = [e2.clone(), d2.clone()];
    let params = ByProgram(vec![&e_params, &d_params]);
    if std::env::var_os("ENCDEC_DIAG").is_some() {
        diagnose(name, &s, &ck, &Stages { ehl: &ehl, ebind: &ebind, dhl: &dhl, dbind: &dbind, e2: &e2, d2: &d2, e_params: &e_params, d_params: &d_params, dlw: &dlw, elw: &elw, es: &es, emat: &emat, ds: &ds, dmat: &dmat }, &recs[0]);
    }
    // 4. Generate, replay, and the teacher-forced logits.
    let (mut same, mut total, mut agree, mut t_all, mut kl_sum, mut ties) = (0, 0, 0, 0, 0f64, Vec::new());
    for rec in recs {
        let src: Vec<u32> = ids_of(&rec["input_ids"]).iter().map(|t| *t as u32).collect();
        let want = ids_of(&rec["generated"]);
        let job = PipelineJob { prompt: vec![s.decoder_start], negative: src, ..Default::default() };
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
        let replay = tir::pipeline::run_pipeline(&pipe, &programs, &params, &NoDraws, &PipelineJob { generated: got.iter().map(|t| *t as u32).collect(), ..job.clone() })
            .expect("replay");
        assert_eq!(replay.output, run.output, "{name}: the replay differs from the generating run");
        let hf = rows_of(&rec["logits"]);
        let tf = tir::pipeline::run_pipeline(&pipe, &programs, &params, &NoDraws, &PipelineJob { generated: want.iter().map(|t| *t as u32).collect(), ..job.clone() })
            .expect("teacher-forced run");
        let (a, kls, rows, t) = compare(name, "teacher-forced on HF's stream", &tf.output, dmat.logits_scale, &hf);
        // Where the generation departs, its stream is HF's up to there: the teacher-forced row.
        if prefix < want.len() {
            ties.push(tie_ratio(&rows[prefix], &hf[prefix]));
        }
        eprintln!("{name} text pipeline: generated {got:?} vs HF {want:?}: {prefix}/{} identical", want.len());
        let rnd = ids_of(&rec["random_decoder_ids"]);
        let mut fed: Vec<u32> = rnd[1..].iter().map(|t| *t as u32).collect();
        fed.push(0);
        let rr = tir::pipeline::run_pipeline(&pipe, &programs, &params, &NoDraws, &PipelineJob { generated: fed, ..job.clone() }).expect("random stream");
        let (a2, kls2, _, t2) = compare(name, "on a random stream", &rr.output, dmat.logits_scale, &rows_of(&rec["random_logits"]));
        agree += a + a2;
        kl_sum += kls + kls2;
        t_all += hf.len() + rnd.len();
        ties.extend(t);
        ties.extend(t2);
        same += prefix;
        total += want.len();
    }
    let kl_mean = kl_sum / t_all as f64;
    let worst = ties.iter().cloned().fold(0f64, f64::max);
    // 5. Admission.
    let pa = tir::admit_v2::tir_admit_pipeline_v1(
        &pipe.encode(),
        &[e2.encode(), d2.encode()],
        &misaka_palw_tir_lower::admission::default_inputs(),
        &tir::admit_v2::TirJobCeilingsV1::open_v1(),
    )
    .expect("tir_admit_pipeline_v1");
    let nodes = |p: &tir::program_v2::TirProgramV2| -> (usize, usize) {
        (p.blocks.iter().map(|b| b.nodes.len()).sum(), p.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0))
    };
    let ((en, em), (dn, dm)) = (nodes(&e2), nodes(&d2));
    eprintln!(
        "{name} SUMMARY: greedy ids {same}/{total}; top-1 {agree}/{t_all}, mean KL {kl_mean:.2e}; where they differ, HF's top-2 margin ≤ {worst:.2}× the integer logits' RMS error; \
         admitted: job {:?}, {} step leaves; encoder {en} nodes (max {em}/block), decoder {dn} nodes (max {dm}/block)",
        pa.job_cost, pa.job_step_leaves
    );
    assert!(agree as f64 / t_all as f64 >= 0.85 && kl_mean < 0.05, "{name}: top-1 {agree}/{t_all}, KL {kl_mean}");
    // 6. The three implementations and the court, on both stage programs of the first record. The lowering declares a
    //    stage's inputs as params (the version-2 stage lifts them); the second implementation, the typed backend and the
    //    court's demand evaluator see that version-1 view with the inputs as leaves — the decoder's `xkv` is the INTEGER
    //    encoder's own output.
    {
        let rec = &recs[0];
        let src: Vec<u32> = ids_of(&rec["input_ids"]).iter().map(|t| *t as u32).collect();
        let mut padded = src.clone();
        padded.resize(LMAX as usize, 0);
        let ids_t = IntTensor::idx(vec![LMAX as usize], padded);
        let count_t = IntTensor::idx(vec![], vec![src.len() as u32]);
        let einterp = tir::interp_v2::InterpreterV2::new(&e2).expect("encoder interpreter");
        let mut inputs = tir::interp_v2::MapInputs::default();
        inputs.constant.insert(0, ids_t.to_tir());
        inputs.constant.insert(1, count_t.to_tir());
        let xkv = einterp.run_positions(&e_params, &inputs, 1).expect("the integer encoder").remove(0).output;
        let ep6 = with_inputs(&elw.program, &emat.params, &[(encdec::IDS_PARAM, ids_t), (encdec::COUNT_PARAM, count_t.clone())]);
        let dp6 = with_inputs(&dlw.program, &dmat.params, &[(encdec::XKV_PARAM, int_tensor(&xkv)), (encdec::ENC_COUNT_PARAM, count_t)]);
        let stream: Vec<u32> = ids_of(&rec["decoder_input_ids"]).iter().map(|t| *t as u32).collect();
        let last = stream.len() as u32 - 1;
        let n3 = three_ways(&elw.program, &ep6, &[vec![0]]).unwrap_or_else(|e| panic!("{name}: encoder, three implementations: {e}"));
        let n3d = three_ways(&dlw.program, &dp6, &[stream.iter().map(|t| *t as usize).collect()]).unwrap_or_else(|e| panic!("{name}: decoder, three implementations: {e}"));
        let ce = court_coverage(&elw.program, &ep6, &[0], &[0], &[1]).unwrap_or_else(|e| panic!("{name}: encoder, court: {e}"));
        let cd = court_coverage(&dlw.program, &dp6, &stream, &[0, 1, last], &[1, 3]).unwrap_or_else(|e| panic!("{name}: decoder, court: {e}"));
        let mut prims: std::collections::BTreeSet<&str> = ce.primitives.keys().copied().collect();
        prims.extend(cd.primitives.keys().copied());
        eprintln!(
            "{name} COURT: three implementations equal at {n3} + {n3d} positions; the court replays {} + {} commit points ({} + {} nodes, {} + {} elements) over {} primitives",
            ce.commits,
            cd.commits,
            ce.nodes,
            cd.nodes,
            ce.elements,
            cd.elements,
            prims.len()
        );
        assert!(ce.commits > 0 && cd.commits > 0);
    }
    assert!(worst < 2.0, "{name}: the integer stage differs from HF where HF's top-2 margin is {worst}× its logit error, not a tie within its noise");
}

#[test]
fn t5_relu_tied_generates_through_the_two_stage_pipeline() {
    check("t5");
}

#[test]
fn t5_gated_untied_generates_through_the_two_stage_pipeline() {
    check("t5_gated");
}

#[test]
fn bart_generates_through_the_two_stage_pipeline() {
    check("bart");
}

#[test]
fn mbart_generates_through_the_two_stage_pipeline() {
    check("mbart");
}

#[test]
fn marian_generates_through_the_two_stage_pipeline() {
    check("marian");
}

#[test]
fn pegasus_generates_through_the_two_stage_pipeline() {
    check("pegasus");
}

/// **`T5EncoderModel`** — an encoder alone (the text encoder of Flux, SD3, PixArt, Wan, Sana): the adapter `t5-encoder` is data
/// only; the encoder's final-normed rows are the program's output (`i32` rows at a calibrated power-of-two unit). The float
/// encoder against HF, calibration, the integer rows against HF's by cosine, the three implementations and the court, admission.
#[test]
fn t5_encoder_rows_match_hf() {
    use misaka_palw_tir_lower::hf_schema::{AdapterSource, ReadOptions, read_encdec};
    use rand::{Rng, SeedableRng};
    let dir = fixture_dir("t5_encoder");
    let cfg: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).expect("config")).expect("json");
    let o: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).expect("outputs")).expect("json");
    let read = read_encdec(&cfg, &ReadOptions::default()).unwrap_or_else(|e| panic!("the adapter: {e}"));
    assert!(matches!(&read.adapter, AdapterSource::BuiltIn { id, .. } if id == "t5-encoder"), "{:?}", read.adapter);
    let s = read.spec;
    assert!(s.encoder_only());
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let names: std::collections::BTreeSet<String> = ck.names().into_iter().collect();
    let has = |n: &str| names.contains(n);
    let (ehl, ebind) = encdec::hl_encoder(&s, LMAX as usize, &has).expect("hl encoder");
    let (ep, unread) = ParamStore::from_source(&ehl, &ebind, &ck).expect("encoder params");
    assert!(unread.is_empty(), "checkpoint tensors the encoder never reads: {unread:?}");
    // T5's buckets against HF's own.
    if let encdec::Positions::Relative { buckets, max_distance } = s.positions {
        let b = &o["buckets_bidirectional"];
        let from = b["from"].as_i64().unwrap();
        for (i, want) in ids_of(&b["buckets"]).iter().enumerate() {
            assert_eq!(encdec::t5_bucket(from + i as i64, true, buckets, max_distance), *want, "bucket of {}", from + i as i64);
        }
    }
    // 1. The float encoder is HF's.
    let recs = o["records"].as_array().unwrap();
    for rec in recs {
        let src = ids_of(&rec["input_ids"]);
        let enc = encdec::float_encoder(&ehl, &s, &ep, &src, None).expect("float encoder");
        let r = rel(&enc.hidden.concat(), &rows_of(&rec["encoder_hidden"]).concat());
        eprintln!("t5_encoder float vs HF rows: rel {r:.2e}");
        assert!(r < 1e-4, "float encoder vs HF rel {r}");
    }
    // 2. Calibration, 3. the program.
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);
    let mut es = Stats::new();
    for _ in 0..12 {
        let n = rng.gen_range(3..=LMAX as usize);
        let src: Vec<usize> = (0..n).map(|_| rng.gen_range(0..s.vocab)).collect();
        encdec::float_encoder(&ehl, &s, &ep, &src, Some(&mut es)).expect("calibration");
    }
    let quiet = |_: usize, _: usize| {};
    let elw = encdec::lower_encoder(&ehl, &s, LMAX).expect("lower encoder");
    let emat = materialise(&elw, &ehl, &Resident(Arc::new(ep)), &es, &QuantPolicy::default(), &quiet).expect("materialise");
    let e2 = encoder::encdec_encoder_v2(&elw, s.vocab as u32, LMAX).expect("encoder v2");
    let e_params = encoder::lifted_params(&elw.program, &[encdec::IDS_PARAM, encdec::COUNT_PARAM], &emat.params);
    let interp = tir::interp_v2::InterpreterV2::new(&e2).expect("interpreter");
    for rec in recs {
        let src: Vec<u32> = ids_of(&rec["input_ids"]).iter().map(|t| *t as u32).collect();
        let mut padded = src.clone();
        padded.resize(LMAX as usize, 0);
        let mut inputs = tir::interp_v2::MapInputs::default();
        inputs.constant.insert(0, IntTensor::idx(vec![LMAX as usize], padded).to_tir());
        inputs.constant.insert(1, IntTensor::idx(vec![], vec![src.len() as u32]).to_tir());
        let out = interp.run_positions(&e_params, &inputs, 1).expect("the integer encoder").remove(0).output;
        let d = s.d;
        let got: Vec<f64> = out.data[..src.len() * d].iter().map(|c| *c as f64 * emat.logits_scale).collect();
        let want = rows_of(&rec["encoder_hidden"]).concat();
        let r = rel(&got, &want);
        let cos = got.iter().zip(&want).map(|(a, b)| a * b).sum::<f64>() / (got.iter().map(|a| a * a).sum::<f64>().sqrt() * want.iter().map(|a| a * a).sum::<f64>().sqrt());
        eprintln!("t5_encoder integer rows vs HF ({} real of {LMAX}): cosine {cos:.6}, rel {r:.2e}", src.len());
        assert!(cos > 0.999, "cosine {cos}, rel {r}");
    }
    // 4. The three implementations and the court (the ids and count are input params in the version-1 view).
    {
        let src: Vec<u32> = ids_of(&recs[0]["input_ids"]).iter().map(|t| *t as u32).collect();
        let mut padded = src.clone();
        padded.resize(LMAX as usize, 0);
        let p6 = with_inputs(
            &elw.program,
            &emat.params,
            &[(encdec::IDS_PARAM, IntTensor::idx(vec![LMAX as usize], padded)), (encdec::COUNT_PARAM, IntTensor::idx(vec![], vec![src.len() as u32]))],
        );
        let n3 = three_ways(&elw.program, &p6, &[vec![0]]).unwrap_or_else(|e| panic!("three implementations: {e}"));
        let c = court_coverage(&elw.program, &p6, &[0], &[0], &[1]).unwrap_or_else(|e| panic!("court: {e}"));
        eprintln!("t5_encoder COURT: three implementations equal ({n3} position); the court replays {} commit points ({} nodes)", c.commits, c.nodes);
        assert!(c.commits > 0);
    }
    // 5. Admission.
    tir::admit_v2::tir_admit_program_v2(&e2, &misaka_palw_tir_lower::admission::default_inputs()).expect("tir_admit_v2");
}

/// What the diagnosis reads of one model's lowering.
struct Stages<'a> {
    ehl: &'a misaka_palw_tir_lower::hl::HlProgram,
    ebind: &'a misaka_palw_tir_lower::weights::Binding,
    dhl: &'a misaka_palw_tir_lower::hl::HlProgram,
    dbind: &'a misaka_palw_tir_lower::weights::Binding,
    e2: &'a tir::program_v2::TirProgramV2,
    d2: &'a tir::program_v2::TirProgramV2,
    e_params: &'a misaka_palw_tir_lower::lower::IntParams,
    d_params: &'a misaka_palw_tir_lower::lower::IntParams,
    dlw: &'a misaka_palw_tir_lower::lower::Lowered,
    elw: &'a misaka_palw_tir_lower::lower::Lowered,
    es: &'a Stats,
    emat: &'a misaka_palw_tir_lower::lower::Materialised,
    ds: &'a Stats,
    dmat: &'a misaka_palw_tir_lower::lower::Materialised,
}

/// `ENCDEC_DIAG=1`: the encoder's cross K/V against the float encoder's (cosine per decoder layer),
/// and the decoder's per-site errors along one record's teacher-forced stream.
fn diagnose(name: &str, s: &encdec::EncDecSpec, ck: &Checkpoint, st: &Stages<'_>, rec: &serde_json::Value) {
    use misaka_palw_tir_lower::lower::FillCtx;
    let src = ids_of(&rec["input_ids"]);
    let (ep, _) = ParamStore::from_source(st.ehl, st.ebind, ck).expect("encoder params");
    let (dp, _) = ParamStore::from_source(st.dhl, st.dbind, ck).expect("decoder params");
    let fe = encdec::float_encoder(st.ehl, s, &ep, &src, None).expect("float encoder");
    // The encoder stage alone.
    let mut ids = src.clone();
    ids.resize(LMAX as usize, 0);
    let mut ein = tir::interp_v2::MapInputs::default();
    ein.constant.insert(0, tir::Tensor::new(tir::DType::Idx, vec![LMAX as usize], ids.iter().map(|t| *t as i128).collect()).unwrap());
    ein.constant.insert(1, tir::Tensor::scalar(tir::DType::Idx, src.len() as i128).unwrap());
    let xkv = tir::interp_v2::InterpreterV2::new(st.e2).unwrap().run_positions(st.e_params, &ein, 1).unwrap().remove(0).output;
    let (l, di) = (LMAX as usize, s.dec_inner());
    for layer in 0..s.dec_layers {
        for (kv, f) in [&fe.xk[layer], &fe.xv[layer]].into_iter().enumerate() {
            let (mut dot, mut na, mut nb) = (0f64, 0f64, 0f64);
            for (r, row) in f.iter().enumerate() {
                for (j, fv) in row.iter().enumerate() {
                    let c = xkv.data[((layer * 2 + kv) * l + r) * di + j] as f64;
                    dot += c * fv;
                    na += c * c;
                    nb += fv * fv;
                }
            }
            eprintln!("DIAG {name} cross {} of decoder layer {layer}: cosine {:.6}", ["k", "v"][kv], dot / (na.sqrt() * nb.sqrt()));
        }
    }
    // The encoder's committed row sites (`[L, n]`, the first `count` rows) against the float.
    let mut etrace = encdec::Trace::new();
    encdec::float_encoder_traced(st.ehl, s, &ep, &src, &mut etrace).unwrap();
    let estep = tir::interp_v2::InterpreterV2::new(st.e2).unwrap().run_positions(st.e_params, &ein, 1).unwrap().remove(0);
    let ep_prog = &st.elw.program;
    for c in &estep.commits {
        let Some((site, key, len)) = st.elw.site_nodes.get(&(c.block, c.node)) else { continue };
        let prefix = match c.block {
            b if b == ep_prog.schedule.pre => "pre.".to_string(),
            b if b == ep_prog.schedule.post => "post.".to_string(),
            _ => format!("L{}.", c.layer.unwrap_or(0)),
        };
        let k = format!("{prefix}{site}");
        let Some(rows) = etrace.get(&k) else { continue };
        if c.value.data.len() != l * len || rows.first().map(Vec::len) != Some(*len) {
            continue;
        }
        let empty = ParamStore::default();
        let sc = FillCtx::for_scales(st.ehl, &empty, c.layer.map(|l| l as usize), &prefix, st.es, st.emat.resid_scale, &QuantPolicy::default()).scale(key).unwrap();
        let (mut dd, mut bb) = (0f64, 0f64);
        for (r, row) in rows.iter().enumerate() {
            for (j, fv) in row.iter().enumerate() {
                dd += (c.value.data[r * len + j] as f64 * sc - fv).powi(2);
                bb += fv * fv;
            }
        }
        eprintln!("DIAG {name} encoder site {k}: rel {:.2e}", (dd / bb.max(1e-300)).sqrt());
    }
    // The decoder stage along the teacher-forced stream, every committed site against the float.
    let mut din = tir::interp_v2::MapInputs::default();
    din.constant.insert(0, xkv);
    din.constant.insert(1, tir::Tensor::scalar(tir::DType::Idx, src.len() as i128).unwrap());
    let stream = ids_of(&rec["decoder_input_ids"]);
    let toks: Vec<u32> = stream.iter().map(|t| *t as u32).collect();
    let steps = tir::interp_v2::InterpreterV2::new(st.d2).unwrap().run(st.d_params, &din, &toks).unwrap();
    let mut trace = encdec::Trace::new();
    encdec::float_decoder_traced(st.dhl, s, &dp, &fe, &stream, &mut trace).unwrap();
    let (empty, policy, p) = (ParamStore::default(), QuantPolicy::default(), &st.dlw.program);
    let mut acc: std::collections::BTreeMap<String, (f64, f64)> = Default::default();
    for (pos, step) in steps.iter().enumerate() {
        for c in &step.commits {
            let Some((site, key, _)) = st.dlw.site_nodes.get(&(c.block, c.node)) else { continue };
            let prefix = match c.block {
                b if b == p.schedule.pre => "pre.".to_string(),
                b if b == p.schedule.post => "post.".to_string(),
                _ => format!("L{}.", c.layer.unwrap_or(0)),
            };
            let k = format!("{prefix}{site}");
            let Some(f) = trace.get(&k).map(|rows| &rows[pos]) else { continue };
            if f.len() != c.value.data.len() {
                continue;
            }
            let sc = FillCtx::for_scales(st.dhl, &empty, c.layer.map(|l| l as usize), &prefix, st.ds, st.dmat.resid_scale, &policy).scale(key).unwrap();
            let e = acc.entry(k).or_insert((0.0, 0.0));
            for (iv, fv) in c.value.data.iter().zip(f) {
                e.0 += (*iv as f64 * sc - fv).powi(2);
                e.1 += fv * fv;
            }
        }
    }
    for (k, (dd, bb)) in &acc {
        eprintln!("DIAG {name} decoder site {k}: rel {:.2e}", (dd / bb.max(1e-300)).sqrt());
    }
}

/// **Real configurations** (hand-written from the hub, `tests/configs/encdec/`), no weights: both
/// stages lower at a class's shape — a 512-id source, a 512-position stream — and the pipeline is
/// admitted. The numbers are the program's, whatever the weights.
#[test]
fn real_encoder_decoders_lower_and_are_admitted() {
    use tir::pipeline::{TokenPad, TokenRule, TokenSource};
    const L: u32 = 512;
    for name in ["t5-small", "flan-t5-base", "bart-large-cnn", "mbart-large-50", "opus-mt-en-de", "pegasus-xsum"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/encdec").join(format!("{name}.json"));
        let s = encdec::parse_encdec(&std::fs::read_to_string(path).expect("config")).expect("parse");
        let ((ehl, _), (dhl, _)) = encdec::hl_programs(&s, L as usize, &|_| false).expect("hl programs");
        let elw = encdec::lower_encoder(&ehl, &s, L).expect("lower encoder");
        let dlw = encdec::lower_decoder(&dhl, &s, L, L).expect("lower decoder");
        let e2 = encoder::encdec_encoder_v2(&elw, s.vocab as u32, L).expect("encoder v2");
        let d2 = encoder::encdec_decoder_v2(&dlw, L).expect("decoder v2");
        let rule = TokenRule { prefix: vec![], source: TokenSource::Negative, suffix: vec![], pad: Some(TokenPad { id: 0, to_len: L }) };
        let pipe = encoder::encdec_pipeline(rule, L);
        let pa = tir::admit_v2::tir_admit_pipeline_v1(
            &pipe.encode(),
            &[e2.encode(), d2.encode()],
            &misaka_palw_tir_lower::admission::default_inputs(),
            &tir::admit_v2::TirJobCeilingsV1::open_v1(),
        )
        .unwrap_or_else(|e| panic!("{name}: tir_admit_pipeline_v1: {e}"));
        let nodes = |p: &tir::program_v2::TirProgramV2| -> (usize, usize) {
            (p.blocks.iter().map(|b| b.nodes.len()).sum(), p.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0))
        };
        let ((en, em), (dn, dm)) = (nodes(&e2), nodes(&d2));
        eprintln!(
            "{name}: encoder {en} nodes (max {em}/block), decoder {dn} nodes (max {dm}/block); admitted at L = {L}, max_trip {L}: job {:?}, {} step leaves",
            pa.job_cost, pa.job_step_leaves
        );
    }
}
