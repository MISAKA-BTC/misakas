//! **The scoring library and the two pipeline additions** (RFC-0004 §7.2–7.3, work item A7):
//!
//! * each scoring program is canonical, admissible, and equal to its plain-Rust reference over
//!   thousands of drawn cases (ExactMatch, RefLogLik, Judge, Pairwise);
//! * the `Decode` stage: a text stage that is not the output — the toy LM decodes, and an exact-match
//!   stage reads what it decoded (`TokenSource::Generated`); teacher-forced, a RefLogLik stage reads its
//!   consumed logits rows (`StageRows` over a decode stage start at `|prompt| − 1`); every rule of the
//!   additions refuses by name;
//! * `TokenSource::FinalizedOutput` and `TokenSource::Key`: a judge reads the prompt and a finalized
//!   generation, a pairwise judge two of them, and an exact match an item's key.

mod v2common;

use std::collections::BTreeMap;

use misaka_palw_tir::admit::{TirAdmitInputsV1, TirCeilingsV1};
use misaka_palw_tir::admit_v2::{TirJobCeilingsV1, tir_admit_pipeline_v1, tir_admit_program_v2};
use misaka_palw_tir::interp_v2::{InterpreterV2, MapInputs};
use misaka_palw_tir::pipeline::*;
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::scoring::*;
use misaka_palw_tir::{DType, MapParams, RunState, Tensor, TirErrorKind};
use v2common::*;

/// One position of a scoring program over `inputs` (in declaration order).
fn run1(p: &TirProgramV2, inputs: Vec<Tensor>) -> Tensor {
    let interp = InterpreterV2::new(p).expect("a valid program");
    let mut m = MapInputs::default();
    for (k, t) in inputs.into_iter().enumerate() {
        m.constant.insert(k as u16, t);
    }
    interp.step(&MapParams::default(), &m, &mut RunState::default(), 0).expect("runs").output
}

fn idx(v: &[u32], len: u32) -> Tensor {
    let mut data: Vec<i128> = v.iter().map(|x| *x as i128).collect();
    data.resize(len as usize, 0);
    Tensor::new(DType::Idx, vec![len as usize], data).unwrap()
}

fn scalar(dtype: DType, v: i128) -> Tensor {
    Tensor::scalar(dtype, v).unwrap()
}

#[test]
fn every_scoring_program_is_canonical_and_admissible() {
    let inputs = TirAdmitInputsV1 { tile_len: 4, h_chunk: 16, ceilings: TirCeilingsV1::legacy_court_v1() };
    let programs = [
        ("exact match", exact_match_v1(ExactMatchShapeV1 { gen_len: 8, key_len: 4, token_bound: 16 }).unwrap()),
        ("exact match, wide", exact_match_v1(ExactMatchShapeV1 { gen_len: 256, key_len: 32, token_bound: 151_936 }).unwrap()),
        ("ref logprob", ref_logprob_v1(&RefLogLikShapeV1 { rows: 12, row: vec![1, 16] }).unwrap()),
        ("ref logprob, a flat row", ref_logprob_v1(&RefLogLikShapeV1 { rows: 64, row: vec![1000] }).unwrap()),
        ("ref loglik sum", ref_loglik_sum_v1(64).unwrap()),
        ("judge", judge_v1(-(1 << 20), 1 << 20).unwrap()),
        ("pairwise", pairwise_v1().unwrap()),
    ];
    for (name, p) in &programs {
        assert_eq!(&TirProgramV2::decode_canonical(&p.encode()).unwrap_or_else(|e| panic!("{name}: {e}")), p, "{name}");
        assert!(p.params.is_empty(), "{name}: no weights — every param is an input");
        tir_admit_program_v2(p, &inputs).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    assert!(exact_match_v1(ExactMatchShapeV1 { gen_len: 0, key_len: 4, token_bound: 16 }).is_err());
    assert!(ref_logprob_v1(&RefLogLikShapeV1 { rows: 4, row: vec![16, 1] }).is_err(), "a row's last axis is its vocabulary");
    assert!(ref_loglik_sum_v1(0).is_err());
    assert!(judge_v1(1, 0).is_err());
}

/// A small deterministic generator.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

#[test]
fn exact_match_is_its_reference() {
    let (g, k, tb) = (8u32, 4u32, 12u32);
    let p = exact_match_v1(ExactMatchShapeV1 { gen_len: g, key_len: k, token_bound: tb }).unwrap();
    let mut rng = Lcg(7);
    let (mut passes, mut total) = (0, 0);
    for case in 0..3000 {
        let n = rng.below(g as u64 + 1) as usize;
        let mut generated: Vec<u32> = (0..n).map(|_| rng.below(tb as u64) as u32).collect();
        let delim = |rng: &mut Lcg| if rng.below(4) == 0 { -1 } else { rng.below(tb as u64) as i64 };
        let (open, close) = (delim(&mut rng), delim(&mut rng));
        let kn = rng.below(k as u64 + 1) as usize;
        let mut key: Vec<u32> = (0..kn).map(|_| rng.below(tb as u64) as u32).collect();
        // A third of the cases plant the key between the delimiters.
        if case % 3 == 0 && open >= 0 && close >= 0 && 2 + kn <= g as usize {
            key.retain(|t| *t as i64 != open && *t as i64 != close);
            let mut planted = vec![open as u32];
            planted.extend(&key);
            planted.push(close as u32);
            let at = rng.below((g as usize - planted.len() + 1) as u64) as usize;
            generated = (0..at).map(|_| rng.below(tb as u64) as u32).filter(|t| *t as i64 != open).collect();
            generated.extend(planted);
            generated.truncate(g as usize);
        }
        let want = exact_match_reference_v1(&generated, &key, open, close);
        let out = run1(
            &p,
            vec![
                idx(&generated, g),
                scalar(DType::Idx, generated.len() as i128),
                idx(&key, k),
                scalar(DType::Idx, key.len() as i128),
                scalar(DType::I32, open as i128),
                scalar(DType::I32, close as i128),
            ],
        );
        assert_eq!(out.data, vec![want as i128], "case {case}: {generated:?} {key:?} open {open} close {close}");
        passes += want as usize;
        total += 1;
    }
    assert!(passes * 10 > total, "the cases exercise both answers: {passes} of {total}");
}

#[test]
fn ref_loglik_is_its_reference_and_exact() {
    let (r, v) = (6u32, 10u32);
    let lp = ref_logprob_v1(&RefLogLikShapeV1 { rows: r, row: vec![1, v] }).unwrap();
    let sum = ref_loglik_sum_v1(r).unwrap();
    let mut rng = Lcg(11);
    for case in 0..300 {
        let spread = [1u64 << 4, 1 << 12, 1 << 20, 1 << 31][case % 4];
        let rows: Vec<Vec<i32>> = (0..r)
            .map(|_| {
                (0..v).map(|_| (rng.below(2 * spread) as i64 - spread as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32).collect()
            })
            .collect();
        let refs: Vec<u32> = (0..r).map(|_| rng.below(v as u64) as u32).collect();
        let n = rng.below(r as u64 + 1) as usize;
        let scale = [1i64, 1 << 10, 1 << 18, 1 << 24, i32::MAX as i64][case % 5];
        // Stage 1, position by position: the cursor walks the rows, the token is the reference id.
        let data: Vec<i128> = rows.iter().flatten().map(|x| *x as i128).collect();
        let interp = InterpreterV2::new(&lp).unwrap();
        let mut m = MapInputs::default();
        m.constant.insert(0, Tensor::new(DType::I32, vec![r as usize, 1, v as usize], data).unwrap());
        m.constant.insert(1, scalar(DType::I32, scale as i128));
        let mut state = RunState::default();
        let mut values = Vec::new();
        for (p, id) in refs.iter().enumerate().take(n) {
            let out = interp.step(&MapParams::default(), &m, &mut state, *id).unwrap().output;
            assert_eq!(out.data, vec![ref_logprob_reference_v1(&rows[p], *id, scale) as i128], "case {case} position {p}");
            values.push(out.data[0]);
        }
        // Stage 2: the exact sum of the first `n`.
        let mut padded = values.clone();
        padded.resize(r as usize, 0);
        let out = run1(&sum, vec![Tensor::new(DType::I32, vec![r as usize, 1], padded).unwrap(), scalar(DType::Idx, n as i128)]);
        let want = ref_loglik_reference_v1(&rows[..n], &refs[..n], scale);
        assert_eq!(out.shape, vec![2]);
        let (hi, lo) = (out.data[0] as i32, out.data[1] as i32);
        assert!((0..1i64 << 31).contains(&(lo as i64)), "lo in [0, 2^31)");
        assert_eq!(ref_loglik_join_v1(hi, lo), want, "case {case}");
        assert!(want <= 0, "a log-likelihood is at most 0");
    }
    // A row whose reference is its unique maximum at a steep scale: log p ≈ 0.
    let mut row = vec![0i32; v as usize];
    row[3] = 1000;
    let lp = ref_loglik_reference_v1(&[row], &[3], 1 << 24);
    assert!((-(1 << 14)..=0).contains(&lp), "log p within 2^-10 nats of 0: {lp}");
}

#[test]
fn judge_clamps_and_pairwise_orients_by_rs_order() {
    let j = judge_v1(-100, 100).unwrap();
    for (s, want) in [(-5000i128, -100i128), (-7, -7), (0, 0), (99, 99), (1 << 30, 100)] {
        assert_eq!(run1(&j, vec![Tensor::new(DType::I32, vec![1], vec![s]).unwrap()]).data, vec![want]);
    }
    let p = pairwise_v1().unwrap();
    for pref in [-50i32, -6, -5, -1, 0, 1, 5, 6, 50, i32::MIN, i32::MAX] {
        for order in [0, 1] {
            for margin in [0, 5, i32::MAX] {
                let out = run1(
                    &p,
                    vec![
                        Tensor::new(DType::I32, vec![1], vec![pref as i128]).unwrap(),
                        scalar(DType::I32, order as i128),
                        scalar(DType::I32, margin as i128),
                    ],
                );
                assert_eq!(out.data, vec![pairwise_reference_v1(pref, order, margin) as i128], "{pref} {order} {margin}");
            }
        }
    }
    assert_eq!(pairwise_reference_v1(10, 0, 5), 1, "A (the candidate) preferred");
    assert_eq!(pairwise_reference_v1(10, 1, 5), -1, "A is the parent: a loss for the candidate");
}

fn params_of(programs: &[TirProgramV2], seed: u64) -> ProgramParams {
    ProgramParams(programs.iter().enumerate().map(|(i, p)| materialize_v2(p, seed + i as u64)).collect())
}

#[test]
fn a_decode_stage_feeds_an_exact_match_of_what_it_decoded() {
    let (p, programs) = eval_exact_match_pipeline();
    let info = validate_pipeline(&p, &programs).unwrap();
    assert_eq!(info.externals[1].len(), 6);
    let params = params_of(&programs, 600);
    let random = GenRandom { seed: [0; 32], position: 0 };
    let job = PipelineJob { prompt: vec![3, 5, 7], scalars: vec![-1, -1], ..PipelineJob::default() };
    let (run, generated) = run_text_pipeline(&p, &programs, &params, &random, &job, &mut greedy(4)).unwrap();
    assert_eq!(generated.len(), 4);
    assert_eq!(run.stages[0].trip, 3 + 4 - 1, "the decode stage's stream");
    // No key ids and no delimiters: the whole output is the span, and it is not the empty key.
    assert_eq!(run.output.data, vec![0]);
    // The key is the whole output: a pass; a replay with the committed ids is the same run.
    let keyed = PipelineJob { key: generated.clone(), ..job.clone() };
    let (run, again) = run_text_pipeline(&p, &programs, &params, &random, &keyed, &mut greedy(4)).unwrap();
    assert_eq!(again, generated);
    assert_eq!(run.output.data, vec![1]);
    let replay =
        run_pipeline(&p, &programs, &params, &random, &PipelineJob { generated: generated.clone(), ..keyed.clone() }).unwrap();
    assert_eq!(replay, run, "the replay");
    // Delimited: the ids between the first and the last generated id.
    let delimited = PipelineJob {
        key: generated[1..3].to_vec(),
        scalars: vec![generated[0] as i64, generated[3] as i64],
        generated: generated.clone(),
        ..job.clone()
    };
    let want = exact_match_reference_v1(&generated, &generated[1..3], generated[0] as i64, generated[3] as i64);
    let out = run_pipeline(&p, &programs, &params, &random, &delimited).unwrap().output;
    assert_eq!(out.data, vec![want as i128]);
    // The facts a court derives: the generated ids and the key as job data.
    let facts = stage_job_facts(&p, &programs, &delimited).unwrap();
    assert_eq!(facts[1].inputs[&0].data[..4], generated.iter().map(|t| *t as i128).collect::<Vec<_>>()[..]);
    assert_eq!(facts[1].inputs[&1].data, vec![4]);
    assert_eq!(facts[1].inputs[&3].data, vec![2]);
    assert_eq!(facts[0].rows_from, 2, "a stream stage's rows start at |prompt| − 1");
    // Admission sizes the whole pipeline, the decode stage at its widest.
    let inputs = TirAdmitInputsV1 { tile_len: 4, h_chunk: 16, ceilings: TirCeilingsV1::legacy_court_v1() };
    let bytes: Vec<Vec<u8>> = programs.iter().map(|x| x.encode()).collect();
    tir_admit_pipeline_v1(&p.encode(), &bytes, &inputs, &TirJobCeilingsV1::open_v1()).unwrap();
}

#[test]
fn a_teacher_forced_decode_stage_feeds_ref_loglik_its_consumed_rows() {
    let (p, programs) = eval_ref_loglik_pipeline();
    validate_pipeline(&p, &programs).unwrap();
    let params = params_of(&programs, 700);
    let random = GenRandom { seed: [0; 32], position: 0 };
    let scale = 1i64 << 12;
    for (prompt, reference) in [(vec![3u32, 5], vec![7u32, 2, 9]), (vec![1], vec![4, 4, 4, 4, 4]), (vec![2, 3, 4, 5], vec![1])] {
        let job = PipelineJob { prompt: prompt.clone(), generated: reference.clone(), scalars: vec![scale], ..PipelineJob::default() };
        let run = run_pipeline(&p, &programs, &params, &random, &job).unwrap();
        let consumed: Vec<Vec<i32>> =
            run.stages[0].steps[prompt.len() - 1..].iter().map(|s| s.output.data.iter().map(|x| *x as i32).collect()).collect();
        assert_eq!(consumed.len(), reference.len(), "one consumed row per reference id");
        let want = ref_loglik_reference_v1(&consumed, &reference, scale);
        assert_eq!(ref_loglik_join_v1(run.output.data[0] as i32, run.output.data[1] as i32), want, "{prompt:?} {reference:?}");
        let facts = stage_job_facts(&p, &programs, &job).unwrap();
        assert_eq!(facts[1].trip, reference.len() as u32, "RefLogLik's first stage: one position per reference id");
        assert_eq!(facts[1].tokens, reference, "its tokens are the reference");
        assert_eq!(facts[2].inputs[&1].data, vec![reference.len() as i128], "StageRowCount: its rows");
        // Each position scored its own consumed row.
        for (p_, row) in consumed.iter().enumerate() {
            assert_eq!(run.stages[1].steps[p_].output.data, vec![ref_logprob_reference_v1(row, reference[p_], scale) as i128]);
        }
    }
}

#[test]
fn finalized_outputs_feed_a_judge_and_a_pairwise_judge() {
    let random = GenRandom { seed: [0; 32], position: 0 };
    let (a, b) = (vec![3u32, 9, 1, 1], vec![2u32, 2, 8]);
    // A judge over the prompt and claim 0's generation.
    let (p, programs) = eval_judge_pipeline();
    validate_pipeline(&p, &programs).unwrap();
    let params = params_of(&programs, 800);
    let job = PipelineJob { prompt: vec![4, 5], finalized: BTreeMap::from([((0, 0), a.clone())]), ..PipelineJob::default() };
    let run = run_pipeline(&p, &programs, &params, &random, &job).unwrap();
    assert_eq!(
        run.output.data,
        run.stages[0].steps[0].output.data.iter().map(|x| (*x).clamp(-(1 << 20), 1 << 20)).collect::<Vec<_>>()
    );
    let other = PipelineJob { finalized: BTreeMap::from([((0, 0), b.clone())]), ..job.clone() };
    assert_ne!(
        run_pipeline(&p, &programs, &params, &random, &other).unwrap().stages[0].steps,
        run.stages[0].steps,
        "the judge read it"
    );
    let missing = PipelineJob { finalized: BTreeMap::new(), ..job.clone() };
    assert_eq!(run_pipeline(&p, &programs, &params, &random, &missing).unwrap_err().kind, TirErrorKind::Missing);
    // A pairwise judge over claims 0 and 1, in R's order.
    let (p, programs) = eval_pairwise_pipeline();
    validate_pipeline(&p, &programs).unwrap();
    let params = params_of(&programs, 900);
    for order in [0i64, 1] {
        let job = PipelineJob {
            scalars: vec![order, 0],
            finalized: BTreeMap::from([((0, 0), a.clone()), ((1, 0), b.clone())]),
            ..PipelineJob::default()
        };
        let run = run_pipeline(&p, &programs, &params, &random, &job).unwrap();
        let pref = run.stages[0].steps[0].output.data[0] as i32;
        assert_eq!(run.output.data, vec![pairwise_reference_v1(pref, order as i32, 0) as i128]);
        let swapped = PipelineJob {
            finalized: BTreeMap::from([((0, 0), b.clone()), ((1, 0), a.clone())]),
            scalars: vec![1 - order, 0],
            ..job.clone()
        };
        assert_eq!(
            run_pipeline(&p, &programs, &params, &random, &swapped).unwrap().output,
            run.output,
            "the same outcome either way round"
        );
    }
}

#[test]
fn the_additions_refuse_by_name() {
    let (base, programs) = eval_exact_match_pipeline();
    let refused = |p: &TirPipelineV1| validate_pipeline(p, &programs).unwrap_err();
    // A decode stage is never the output; the text stage always is.
    let mut output = base.clone();
    output.output_stage = 0;
    assert_eq!(refused(&output).kind, TirErrorKind::NormalForm);
    let mut text = base.clone();
    text.stages[0].trip = TripRule::TextStream;
    assert_eq!(refused(&text).kind, TirErrorKind::NormalForm, "TextStream is only the output stage");
    // Generated ids are read only after the decode stage.
    let mut no_decode = base.clone();
    no_decode.stages[0].trip = TripRule::TokenCount;
    assert!(validate_pipeline(&no_decode, &programs).is_err());
    let mut first = base.clone();
    first.stages.swap(0, 1);
    first.output_stage = 0;
    assert!(validate_pipeline(&first, &programs).is_err(), "a stage before the decode stage reads nothing it decoded");
    // One stream stage.
    let (mut two, mut progs) = eval_exact_match_pipeline();
    progs.push(lm_program(false));
    two.stages.insert(1, StageDecl { name: "again".into(), program: 2, ..subject_stage() });
    assert!(validate_pipeline(&two, &progs).is_err());
    // A finalized claim is below the cap.
    let (mut judge, jprogs) = eval_judge_pipeline();
    if let Binding::JobTokens { rule } = &mut judge.stages[0].bind[2] {
        rule.source = TokenSource::FinalizedOutput { claim: MAX_FINALIZED_CLAIMS, stage: 0 };
    }
    assert!(validate_pipeline(&judge, &jprogs).is_err());
    // The encodings of the additions round-trip; the earlier tags do not move.
    for (p, progs) in [eval_exact_match_pipeline(), eval_ref_loglik_pipeline(), eval_judge_pipeline(), eval_pairwise_pipeline()] {
        assert_eq!(TirPipelineV1::decode_canonical(&p.encode(), &progs).unwrap(), p);
    }
    assert_eq!(borsh::to_vec(&TripRule::Decode).unwrap(), vec![4]);
    assert_eq!(borsh::to_vec(&TokenSource::Generated).unwrap(), vec![3]);
    assert_eq!(borsh::to_vec(&TokenSource::Key).unwrap(), vec![4]);
    assert_eq!(borsh::to_vec(&TokenSource::FinalizedOutput { claim: 1, stage: 2 }).unwrap(), vec![5, 1, 2]);
    assert_eq!(borsh::to_vec(&TokenSource::Source).unwrap(), vec![2]);
}
