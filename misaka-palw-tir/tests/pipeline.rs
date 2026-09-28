//! **Pipelines of programs** (RFC-0003 §I.2.3, spec 04b §15.6): the toy text-to-image pipeline
//! runs and reproduces; every structural rule refuses by name; job facts outside their domains are
//! refused at run time.

mod v2common;

use misaka_palw_tir::TirErrorKind;
use misaka_palw_tir::pipeline::*;
use misaka_palw_tir::program_v2::*;
use v2common::*;

fn params_for(programs: &[TirProgramV2]) -> ProgramParams {
    ProgramParams(programs.iter().enumerate().map(|(i, p)| materialize_v2(p, 100 + i as u64)).collect())
}

fn run(job: &PipelineJob, seed: [u8; 32], position: u32) -> Result<PipelineRun, TirErrorKind> {
    let (p, programs) = toy_pipeline();
    run_pipeline(&p, &programs, &params_for(&programs), &GenRandom { seed, position }, job).map_err(|e| e.kind)
}

#[test]
fn the_toy_pipeline_runs_and_reproduces() {
    let job = toy_job();
    let a = run(&job, [0x2a; 32], 0).unwrap();
    assert_eq!(a.stages.iter().map(|s| s.trip).collect::<Vec<_>>(), vec![5, 3, 1], "[1] ‖ 3 prompt ids ‖ [2]; 3 steps; one decode");
    assert_eq!(a.stages[0].tokens, vec![1, 5, 6, 7, 2]);
    assert_eq!(a.output.shape, vec![2, 2, 3]);
    assert!(a.output.data.iter().all(|v| (0..=255).contains(v)), "pixels");
    // The same job, seed and item index: the same bytes, every stage, every commit point.
    assert_eq!(run(&job, [0x2a; 32], 0).unwrap(), a);
    // Another seed, or another image of the same seed: another image.
    assert_ne!(run(&job, [0x2b; 32], 0).unwrap().output, a.output);
    assert_ne!(run(&job, [0x2a; 32], 1).unwrap().output, a.output);
    // The encoder's rows reach the denoiser: another prompt, another image.
    let other = PipelineJob { prompt: vec![5, 6, 8], ..job.clone() };
    assert_ne!(run(&other, [0x2a; 32], 0).unwrap().stages[1].steps, a.stages[1].steps);
}

#[test]
fn run_time_job_facts_outside_their_domains_are_refused() {
    let job = toy_job();
    assert_eq!(run(&PipelineJob { steps: 0, ..job.clone() }, [1; 32], 0).unwrap_err(), TirErrorKind::Position, "zero steps");
    assert_eq!(
        run(&PipelineJob { steps: 5, ..job.clone() }, [1; 32], 0).unwrap_err(),
        TirErrorKind::Position,
        "more steps than max_trip"
    );
    assert_eq!(
        run(&PipelineJob { prompt: vec![3; 5], ..job.clone() }, [1; 32], 0).unwrap_err(),
        TirErrorKind::Position,
        "a prompt past max_trip"
    );
    assert_eq!(
        run(&PipelineJob { prompt: vec![16], ..job.clone() }, [1; 32], 0).unwrap_err(),
        TirErrorKind::Operand,
        "a token ≥ token_bound"
    );
    assert_eq!(
        run(&PipelineJob { scalars: vec![161, 1], ..job.clone() }, [1; 32], 0).unwrap_err(),
        TirErrorKind::Operand,
        "guidance above 10.0"
    );
    assert_eq!(
        run(&PipelineJob { scalars: vec![24, 2], ..job.clone() }, [1; 32], 0).unwrap_err(),
        TirErrorKind::Operand,
        "no third step set"
    );
    assert_eq!(
        run(&PipelineJob { scalars: vec![24], ..job.clone() }, [1; 32], 0).unwrap_err(),
        TirErrorKind::Missing,
        "a missing scalar"
    );
    // An empty prompt is a valid job: the template alone.
    assert_eq!(run(&PipelineJob { prompt: vec![], ..job }, [1; 32], 0).unwrap().stages[0].trip, 2);
}

#[test]
fn every_pipeline_rule_refuses_by_name() {
    let (base, programs) = toy_pipeline();
    assert!(validate_pipeline(&base, &programs).is_ok());
    let mut cases: Vec<(&str, TirPipelineV1, Vec<TirProgramV2>)> = Vec::new();
    let mut push = |what: &'static str, f: &dyn Fn(&mut TirPipelineV1, &mut Vec<TirProgramV2>)| {
        let (mut p, mut progs) = (base.clone(), programs.clone());
        f(&mut p, &mut progs);
        cases.push((what, p, progs));
    };
    push("version", &|p, _| p.version = 2);
    push("no stage", &|p, _| p.stages.clear());
    push("17 stages", &|p, _| {
        let s = p.stages[2].clone();
        for i in 0..14 {
            p.stages.push(StageDecl { name: format!("x{i}"), ..s.clone() });
        }
    });
    push("no output stage", &|p, _| p.output_stage = 3);
    push("two stages with one name", &|p, _| p.stages[1].name = "encode".into());
    push("no program", &|p, _| p.stages[2].program = 3);
    push("max_trip 0", &|p, _| p.stages[1].max_trip = 0);
    push("Fixed with another max_trip", &|p, _| p.stages[2].max_trip = 2);
    push("a token-reading stage without its rule", &|p, _| p.stages[0].tokens = None);
    push("a token rule on a stage that reads no token", &|p, _| p.stages[1].tokens = p.stages[0].tokens.clone());
    push("a token-reading stage on JobSteps", &|p, _| p.stages[0].trip = TripRule::JobSteps);
    push("a template token past token_bound", &|p, _| p.stages[0].tokens.as_mut().unwrap().prefix = vec![16]);
    push("a binding short", &|p, _| {
        p.stages[1].bind.pop();
    });
    push("an edge from a later stage", &|p, _| p.stages[1].bind[0] = Binding::StageRows { stage: 2, drop: 1, pad_to: 5 });
    push("an edge from itself", &|p, _| p.stages[1].bind[0] = Binding::StageRows { stage: 1, drop: 1, pad_to: 5 });
    push("StageRows on a Final stage", &|p, _| p.stages[2].bind[0] = Binding::StageRows { stage: 1, drop: 0, pad_to: 12 });
    push("StageRows of the wrong length", &|p, _| p.stages[1].bind[0] = Binding::StageRows { stage: 0, drop: 0, pad_to: 5 });
    push("rows the pad cannot hold", &|p, progs| {
        p.stages[0].max_trip = 7;
        let _ = progs;
    });
    push("rows outside the input's interval", &|_, progs| {
        progs[1].inputs[IN_COND as usize].source = InputSource::External { lo: -100, hi: 100 };
    });
    push("StageFinal on a Rows stage", &|p, _| p.stages[1].bind[0] = Binding::StageFinal { stage: 0 });
    push("a row count the input's interval cannot hold", &|_, progs| {
        progs[1].inputs[IN_COND_LEN as usize].source = InputSource::External { lo: 0, hi: 4 };
    });
    push("a job scalar bound to a tensor", &|p, _| p.stages[1].bind[0] = Binding::JobScalar { index: 0 });
    push("a job scalar index past 16", &|p, _| p.stages[1].bind[2] = Binding::JobScalar { index: 16 });
    push("one domain in two stages", &|_, progs| {
        progs[2].inputs.push(InputDecl {
            name: "dec.noise".into(),
            dtype: misaka_palw_tir::DType::I32,
            shape: vec![1],
            source: InputSource::Random { domain: 1, dist: RandomDist::Normal, per_step: false },
        });
    });
    push("a Logits output stage", &|_, progs| {
        progs[2].output = OutputDecl::Logits { node: progs[2].output.node(), scheme_id: [0; 64] };
    });
    push("a dead stage", &|p, _| p.output_stage = 1);
    for (what, p, progs) in &cases {
        let e = validate_pipeline(p, progs).err().unwrap_or_else(|| panic!("{what}: accepted"));
        assert_eq!(e.kind, TirErrorKind::NormalForm, "{what}: {e}");
    }
}

#[test]
fn a_token_count_masks_by_length_whatever_the_pad_id() {
    let job = PipelineJob { prompt: vec![5, 6], ..toy_job() };
    let run_with = |pad_id: u32| {
        let (p, progs) = bidirectional_pipeline(pad_id);
        run_pipeline(&p, &progs, &params_for(&progs), &GenRandom { seed: [0; 32], position: 0 }, &job).unwrap()
    };
    let (a, b) = (run_with(0), run_with(7));
    assert_eq!(a.output, b.output, "the mask is the count, not the pad id");
    // The output is the sum of the embeddings of [1, 5, 6, 2] — four admitted rows.
    let (_, progs) = bidirectional_pipeline(0);
    let embed = &params_for(&progs).0[0].tensors[&(0, None)];
    let want: Vec<i128> = (0..D as usize).map(|c| [1usize, 5, 6, 2].iter().map(|t| embed.data[t * D as usize + c]).sum()).collect();
    assert_eq!(a.output.data, want);
    // A template longer than its pad is refused at run time.
    let (p, progs) = bidirectional_pipeline(0);
    let long = PipelineJob { prompt: vec![5; 5], ..toy_job() };
    let e = run_pipeline(&p, &progs, &params_for(&progs), &GenRandom { seed: [0; 32], position: 0 }, &long).unwrap_err();
    assert_eq!(e.kind, TirErrorKind::Operand);
}

#[test]
fn token_count_rules_refuse_by_name() {
    let (base, progs) = bidirectional_pipeline(0);
    assert!(validate_pipeline(&base, &progs).is_ok());
    let mut no_pad = base.clone();
    if let Binding::JobTokenCount { rule } = &mut no_pad.stages[0].bind[1] {
        rule.pad = None;
    }
    assert_eq!(validate_pipeline(&no_pad, &progs).unwrap_err().kind, TirErrorKind::NormalForm, "a count of an unpadded template");
    let mut narrow = progs.clone();
    narrow[0].inputs[1].source = InputSource::External { lo: 0, hi: 5 };
    assert_eq!(validate_pipeline(&base, &narrow).unwrap_err().kind, TirErrorKind::NormalForm, "[0, 6] does not fit [0, 5]");
    let mut ranked = base.clone();
    ranked.stages[0].bind[1] = Binding::JobTokenCount {
        rule: TokenRule { prefix: vec![], source: TokenSource::Prompt, suffix: vec![], pad: Some(TokenPad { id: 0, to_len: 6 }) },
    };
    ranked.stages[0].bind.swap(0, 1);
    assert_eq!(validate_pipeline(&ranked, &progs).unwrap_err().kind, TirErrorKind::NormalForm, "a count bound to the token tensor");
}

#[test]
fn a_pipeline_round_trips_its_canonical_bytes() {
    let (p, progs) = toy_pipeline();
    let bytes = p.encode();
    assert_eq!(TirPipelineV1::decode_canonical(&bytes, &progs).unwrap(), p);
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(TirPipelineV1::decode_canonical(&trailing, &progs).unwrap_err().kind, TirErrorKind::Encoding);
    assert_eq!(TirPipelineV1::decode_canonical(&bytes[..bytes.len() - 1], &progs).unwrap_err().kind, TirErrorKind::Encoding);
    let mut bad = p.clone();
    bad.output_stage = 9;
    assert_eq!(TirPipelineV1::decode_canonical(&bad.encode(), &progs).unwrap_err().kind, TirErrorKind::NormalForm);
    assert_eq!(TirPipelineV1::decode_canonical(&vec![0u8; MAX_PIPELINE_BYTES + 1], &progs).unwrap_err().kind, TirErrorKind::Encoding);
}

/// The court's derived inputs other than `R`: what a job fixes of each stage is what the run used.
#[test]
fn stage_job_facts_are_what_the_run_used() {
    let (p, programs) = toy_pipeline();
    let job = toy_job();
    let facts = stage_job_facts(&p, &programs, &job).unwrap();
    let a = run(&job, [0x2a; 32], 0).unwrap();
    for (f, s) in facts.iter().zip(&a.stages) {
        assert_eq!((f.trip, &f.tokens), (s.trip, &s.tokens));
    }
    let den = &facts[1].inputs;
    assert_eq!(den.keys().copied().collect::<Vec<_>>(), vec![IN_COND_LEN, IN_GUIDANCE, IN_STEPS_IDX], "no edge, no random input");
    assert_eq!(den[&IN_COND_LEN].data, vec![4], "the encoder's 5 rows less the one dropped");
    assert_eq!((den[&IN_GUIDANCE].data[0], den[&IN_STEPS_IDX].data[0]), (24, 1));
    assert!(facts[0].inputs.is_empty() && facts[2].inputs.is_empty(), "the encoder reads tokens only; the decoder an edge");
    // Refused as the run refuses.
    let short = PipelineJob { scalars: vec![24], ..job.clone() };
    assert_eq!(stage_job_facts(&p, &programs, &short).unwrap_err().kind, TirErrorKind::Missing);
    let long = PipelineJob { prompt: vec![5; 5], ..job.clone() };
    assert_eq!(stage_job_facts(&p, &programs, &long).unwrap_err().kind, TirErrorKind::Position);
    let none = PipelineJob { steps: 0, ..job };
    assert_eq!(stage_job_facts(&p, &programs, &none).unwrap_err().kind, TirErrorKind::Position);
}
