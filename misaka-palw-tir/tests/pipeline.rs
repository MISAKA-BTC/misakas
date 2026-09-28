//! **Pipelines of programs** (RFC-0003 §I.2.3, spec 04b §15.6): the toy text-to-image pipeline
//! runs and reproduces; every structural rule refuses by name; job facts outside their domains are
//! refused at run time.

mod v2common;

use misaka_palw_tir::pipeline::*;
use misaka_palw_tir::program_v2::*;
use misaka_palw_tir::{DType, TirErrorKind};
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

/// **The source (RFC-0003 §II.2.2)**: `TokenSource::Source` reads the job's own second list — not the
/// prompt, not the negative prompt — with every template rule a prompt's has, and encodes as tag 2.
#[test]
fn a_source_is_the_jobs_own_token_list() {
    let (p, progs) = bidirectional_pipeline(0);
    let mut over_source = p.clone();
    for b in &mut over_source.stages[0].bind {
        if let Binding::JobTokens { rule } | Binding::JobTokenCount { rule } = b {
            rule.source = TokenSource::Source;
        }
    }
    validate_pipeline(&over_source, &progs).unwrap();
    let bytes = over_source.encode();
    assert_eq!(TirPipelineV1::decode_canonical(&bytes, &progs).unwrap(), over_source, "the canonical bytes round-trip");
    assert_ne!(bytes, p.encode(), "tag 2, not the prompt's 0");
    let (params, random) = (params_for(&progs), GenRandom { seed: [0; 32], position: 0 });
    let run = |p: &TirPipelineV1, job: &PipelineJob| run_pipeline(p, &progs, &params, &random, job);
    let from_prompt = run(&p, &PipelineJob { prompt: vec![5, 6], ..toy_job() }).unwrap();
    let from_source = run(&over_source, &PipelineJob { prompt: vec![9], source: vec![5, 6], ..toy_job() }).unwrap();
    assert_eq!(from_source.output, from_prompt.output, "the same ids, the same encoding, whichever list carries them");
    let other_prompt = run(&over_source, &PipelineJob { prompt: vec![3, 4, 8], source: vec![5, 6], ..toy_job() }).unwrap();
    assert_eq!(other_prompt.output, from_source.output, "the prompt is not read");
    let facts = stage_job_facts(&over_source, &progs, &PipelineJob { source: vec![5, 6], ..toy_job() }).unwrap();
    assert_eq!(facts[0].inputs[&0].data, vec![1, 5, 6, 2, 0, 0], "the template over the source, padded");
    assert_eq!(facts[0].inputs[&1].data, vec![4], "its count");
    // A source past its pad is refused as a prompt is; an empty one is the template alone.
    let long = run(&over_source, &PipelineJob { source: vec![5; 5], ..toy_job() }).unwrap_err();
    assert_eq!(long.kind, TirErrorKind::Operand);
    assert!(run(&over_source, &PipelineJob { source: vec![], ..toy_job() }).is_ok());
}

/// **The toy encoder–decoder** (RFC-0003 §II.2.2): the encoder over the source, the text stage over
/// a prompt that starts with the forced start id; the committed ids replay; a different source is a
/// different run of the text stage, and the source never enters the text stream.
#[test]
fn an_encoder_decoder_reads_its_source_through_the_edge() {
    let (p, progs) = encdec_pipeline();
    validate_pipeline(&p, &progs).unwrap();
    let params = ProgramParams(progs.iter().enumerate().map(|(i, prog)| materialize_v2(prog, 500 + i as u64)).collect());
    let random = GenRandom { seed: [0; 32], position: 0 };
    let job = encdec_job();
    let (run, generated) = run_text_pipeline(&p, &progs, &params, &random, &job, &mut greedy(4)).unwrap();
    assert_eq!(generated.len(), 4);
    let committed = PipelineJob { generated: generated.clone(), ..job.clone() };
    assert_eq!(run_pipeline(&p, &progs, &params, &random, &committed).unwrap(), run, "the replay");
    assert_eq!(run.stages[1].tokens[..4], job.prompt[..], "the stream is the prompt, then the ids");
    assert_eq!(run.stages[1].tokens[0], ENCDEC_START);
    let other = PipelineJob { source: vec![3, 4, 13], generated, ..job.clone() };
    let replay = run_pipeline(&p, &progs, &params, &random, &other).unwrap();
    assert_ne!(replay.stages[0].steps, run.stages[0].steps, "the encoder read the source");
    assert_ne!(replay.stages[1].steps, run.stages[1].steps, "and the decoder read the encoder");
    assert_eq!(replay.stages[1].tokens, run.stages[1].tokens, "the stream is not the source");
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

/// **A job image** (RFC-0003 §II.4, NF-P10): bound as `i16 [h, w, 3]` over `[0, 255]`, run from the
/// job's pixels, and never one of the job facts a court is handed (it opens the image's tiles).
#[test]
fn a_job_image_is_bound_run_and_refused_by_name() {
    let (p, programs) = vision_pipeline();
    let info = validate_pipeline(&p, &programs).unwrap();
    assert_eq!(info.images, vec![[VIS_H, VIS_W]], "one image slot, 2 × 3");
    let params = params_for(&programs);
    let job = vision_job();
    let random = GenRandom { seed: [0; 32], position: 0 };
    let run = run_pipeline(&p, &programs, &params, &random, &job).unwrap();
    assert_eq!(run.output.shape, vec![1, D as usize]);
    // Another pixel, another embedding.
    let mut other = job.clone();
    other.images[0].rgb[4] ^= 0x40;
    assert_ne!(run_pipeline(&p, &programs, &params, &random, &other).unwrap().output, run.output);
    // The job's image at another size, or bytes that are not h · w · 3, is refused; a missing one too.
    for (what, img) in [
        ("3 × 2", JobImageV1 { h: 3, w: 2, rgb: job.images[0].rgb.clone() }),
        ("a byte short", JobImageV1 { rgb: job.images[0].rgb[1..].to_vec(), ..job.images[0].clone() }),
    ] {
        let bad = PipelineJob { images: vec![img], ..job.clone() };
        assert_eq!(run_pipeline(&p, &programs, &params, &random, &bad).unwrap_err().kind, TirErrorKind::Operand, "{what}");
    }
    let none = PipelineJob { images: vec![], ..job.clone() };
    assert!(run_pipeline(&p, &programs, &params, &random, &none).is_err(), "no image");
    // A court is not handed the pixels: the image is no job fact.
    let facts = stage_job_facts(&p, &programs, &none).unwrap();
    assert!(facts[0].inputs.is_empty());

    // NF-P10, each by name.
    let refused = |p: &TirPipelineV1, progs: &[TirProgramV2]| validate_pipeline(p, progs).unwrap_err().kind;
    let with_input = |source: InputSource, dtype: DType, shape: Vec<u32>| {
        let mut prog = vision_program();
        prog.inputs[0].source = source;
        prog.inputs[0].dtype = dtype;
        prog.inputs[0].shape = shape;
        vec![prog]
    };
    let ext = |lo, hi| InputSource::External { lo, hi };
    assert_eq!(
        refused(&p, &with_input(ext(0, 254), DType::I16, vec![VIS_H, VIS_W, 3])),
        TirErrorKind::NormalForm,
        "[0, 255] ⊄ [0, 254]"
    );
    assert_eq!(
        refused(&p, &with_input(ext(1, 255), DType::I16, vec![VIS_H, VIS_W, 3])),
        TirErrorKind::NormalForm,
        "[0, 255] ⊄ [1, 255]"
    );
    let mut far = p.clone();
    far.stages[0].bind = vec![Binding::JobImage { index: MAX_JOB_IMAGES as u8 }];
    assert_eq!(refused(&far, &programs), TirErrorKind::NormalForm, "an index past the cap");
    let mut gap = p.clone();
    gap.stages[0].bind = vec![Binding::JobImage { index: 1 }];
    assert_eq!(refused(&gap, &programs), TirErrorKind::NormalForm, "image 0 unbound while image 1 is");
    // An image input whose type is not i16 [h, w, 3] (the program's own normal form may refuse it
    // first; either way it is refused).
    for (dtype, shape) in
        [(DType::I32, vec![VIS_H, VIS_W, 3]), (DType::I16, vec![VIS_H * VIS_W, 3]), (DType::I16, vec![VIS_H, VIS_W, 4])]
    {
        assert!(validate_pipeline(&p, &with_input(ext(0, 255), dtype, shape.clone())).is_err(), "{} {shape:?}", dtype.name());
    }
    // Two images at their own sizes; one image at two sizes is refused.
    let two = two_image_program();
    let stage = |bind| TirPipelineV1 {
        version: TIR_PIPELINE_VERSION_V1,
        stages: vec![StageDecl { name: "two".into(), program: 0, trip: TripRule::Fixed { n: 1 }, max_trip: 1, tokens: None, bind }],
        output_stage: 0,
    };
    let both = stage(vec![Binding::JobImage { index: 0 }, Binding::JobImage { index: 1 }]);
    assert_eq!(validate_pipeline(&both, std::slice::from_ref(&two)).unwrap().images, vec![[2, 3], [3, 2]]);
    let same = stage(vec![Binding::JobImage { index: 0 }, Binding::JobImage { index: 0 }]);
    assert_eq!(refused(&same, std::slice::from_ref(&two)), TirErrorKind::NormalForm, "image 0 at 2 × 3 and at 3 × 2");
}

/// **The text stage** (RFC-0003 §II.2.1, NF-P9′): a `Logits` output stage over the text stream,
/// generating through a selector outside the program and replaying from the committed ids.
#[test]
fn a_text_stage_generates_through_its_selector_and_replays() {
    let (p, programs) = text_pipeline();
    let params = params_for(&programs);
    let random = GenRandom { seed: [0; 32], position: 0 };
    let job = PipelineJob { prompt: vec![3, 7, 9], ..PipelineJob::default() };
    let (run, generated) = run_text_pipeline(&p, &programs, &params, &random, &job, &mut greedy(4)).unwrap();
    assert_eq!(generated.len(), 4);
    assert_eq!(run.stages[0].trip, 3 + 4 - 1, "|prompt| + |generated| − 1: the last id is never fed back");
    assert_eq!(run.stages[0].tokens, [&job.prompt[..], &generated[..3]].concat());
    assert_eq!(run.output.shape, vec![6, 1, TOK as usize], "the logits rows, stacked");
    // Replaying the committed ids is the same run, position for position.
    let committed = PipelineJob { generated: generated.clone(), ..job.clone() };
    assert_eq!(run_pipeline(&p, &programs, &params, &random, &committed).unwrap(), run);
    let facts = stage_job_facts(&p, &programs, &committed).unwrap();
    assert_eq!((facts[0].trip, &facts[0].tokens), (6, &run.stages[0].tokens));
    // Each consumed position's greedy id is its logits' argmax: the replayed stream agrees.
    for (k, id) in generated.iter().enumerate() {
        let logits = &run.stages[0].steps[job.prompt.len() - 1 + k].output;
        let best = logits.data.iter().enumerate().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(&a.0))).unwrap().0;
        assert_eq!(*id as usize, best, "generated id {k}");
    }
    // Ending at once: one position past the prompt's first consumed logits, and no id.
    let (end, none) = run_text_pipeline(&p, &programs, &params, &random, &job, &mut |_, _| TextSelectV1::End).unwrap();
    assert!(none.is_empty());
    assert_eq!(end.stages[0].trip, 3, "the prompt's positions ran; the last one's logits ended generation");
    assert_eq!(run_pipeline(&p, &programs, &params, &random, &job).unwrap(), end, "no generated id replays the same");
    // The stream may not pass max_trip, and an empty prompt has no first position.
    assert_eq!(run_text_pipeline(&p, &programs, &params, &random, &job, &mut greedy(100)).unwrap_err().kind, TirErrorKind::Position);
    let empty = PipelineJob::default();
    assert_eq!(run_text_pipeline(&p, &programs, &params, &random, &empty, &mut greedy(1)).unwrap_err().kind, TirErrorKind::Position);
    assert_eq!(text_stream_run(&PipelineJob { generated: vec![4], ..job.clone() }), job.prompt, "one id: nothing fed back");
}

/// **The vision-language pipeline**: the image rows reach the language model only at placeholder
/// positions, by the cursor — placement is a function of the prompt ids alone.
#[test]
fn image_rows_enter_the_text_stage_at_placeholder_ids() {
    let (p, programs) = vlm_pipeline();
    let params = params_for(&programs);
    let random = GenRandom { seed: [0; 32], position: 0 };
    let base = vision_job();
    let mut dark = base.clone();
    dark.images[0].rgb.iter_mut().for_each(|b| *b /= 3);
    let logits_at = |job: &PipelineJob| -> Vec<Vec<i128>> {
        let committed = PipelineJob { generated: vec![1, 2], ..job.clone() };
        let run = run_pipeline(&p, &programs, &params, &random, &committed).unwrap();
        run.stages[1].steps.iter().map(|s| s.output.data.clone()).collect()
    };
    // A prompt with two placeholders: positions 1 and 2 read the image, the others do not.
    let with = |job: &PipelineJob| PipelineJob { prompt: vec![3, PLACEHOLDER, PLACEHOLDER, 5], ..job.clone() };
    let (a, b) = (logits_at(&with(&base)), logits_at(&with(&dark)));
    assert_eq!(a.len(), 4 + 2 - 1);
    for pos in 0..a.len() {
        let reads_image = pos == 1 || pos == 2;
        assert_eq!(a[pos] != b[pos], reads_image, "position {pos}");
    }
    // Without placeholders the image changes nothing; the rows still ran.
    let plain = |job: &PipelineJob| PipelineJob { prompt: vec![3, 4, 5], ..job.clone() };
    assert_eq!(logits_at(&plain(&base)), logits_at(&plain(&dark)));
    // A third placeholder is an ordinary token (the rows are spent): embedded as the token, as HF
    // embeds it — the image changes rows 0 and 1 only.
    let three = |job: &PipelineJob| PipelineJob { prompt: vec![PLACEHOLDER, PLACEHOLDER, PLACEHOLDER], ..job.clone() };
    let (r, dark_r) = (logits_at(&three(&base)), logits_at(&three(&dark)));
    assert_ne!(r[0], r[1], "row 0, then row 1");
    assert!(r[0] != dark_r[0] && r[1] != dark_r[1], "the first two read the image");
    assert_eq!(r[2], dark_r[2], "the third is the token's embedding, whatever the image");
    // Generating over it replays.
    let job = with(&base);
    let (run, generated) = run_text_pipeline(&p, &programs, &params, &random, &job, &mut greedy(3)).unwrap();
    assert_eq!(run_pipeline(&p, &programs, &params, &random, &PipelineJob { generated, ..job }).unwrap(), run);
}

#[test]
fn only_the_output_stage_may_be_the_text_stage() {
    let (p, programs) = vlm_pipeline();
    let refused = |p: &TirPipelineV1| validate_pipeline(p, &programs).unwrap_err().kind;
    // The text stage with a token rule, or at a fixed trip.
    let mut ruled = p.clone();
    ruled.stages[1].tokens = Some(TokenRule { prefix: vec![], source: TokenSource::Prompt, suffix: vec![], pad: None });
    assert_eq!(refused(&ruled), TirErrorKind::NormalForm, "a TextStream stage has no token rule");
    let mut fixed = p.clone();
    fixed.stages[1].trip = TripRule::Fixed { n: 12 };
    assert_eq!(refused(&fixed), TirErrorKind::NormalForm, "a Logits stage runs over the text stream");
    // TextStream on a Final program.
    let mut final_text = p.clone();
    final_text.stages[0].trip = TripRule::TextStream;
    final_text.stages[0].max_trip = 12;
    assert_eq!(refused(&final_text), TirErrorKind::NormalForm, "only a Logits program is the text stage");
    // A Logits stage that is not the output: nothing can read it, and it is not the text stage.
    let (tp, tprogs) = text_pipeline();
    let mut two = tp.clone();
    two.stages.push(StageDecl { name: "after".into(), ..tp.stages[0].clone() });
    two.output_stage = 1;
    assert!(validate_pipeline(&two, &tprogs).is_err());
    // Admission prices the text stage at max_trip positions, like any other.
    let bytes: Vec<Vec<u8>> = programs.iter().map(|x| x.encode()).collect();
    let inputs = misaka_palw_tir::admit::TirAdmitInputsV1 {
        tile_len: 16,
        h_chunk: 16,
        ceilings: misaka_palw_tir::admit::TirCeilingsV1::legacy_court_v1(),
    };
    let a = misaka_palw_tir::admit_v2::tir_admit_pipeline_v1(
        &p.encode(),
        &bytes,
        &inputs,
        &misaka_palw_tir::admit_v2::TirJobCeilingsV1::open_v1(),
    )
    .unwrap();
    assert_eq!(a.stages[1].max_trip, 12);
    assert_eq!(a.stages[1].job_cost.macs, 12 * a.stages[1].admission.view.position.cost.macs);
    assert_eq!(a.stages[1].admission.inputs[0].leaf, misaka_palw_tir::admit::ParamLeafV1::Committed, "the image rows: an edge");
}
