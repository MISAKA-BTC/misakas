//! **The golden vectors of program version 2** (`consensus-vectors/tir-v2/`, spec 04b §15.8).
//!
//! * `programs/<name>.json` — the three toy stage programs: canonical bytes, identity, params, the
//!   inputs of every position, every position's output and commit points, the `Fixed` states after
//!   the run, cone cases, and input refusals;
//! * `pipelines/toy-image.json` — the whole toy text-to-image pipeline: its bytes, the programs,
//!   the job, the seed, every random input `R` drew, every stage's positions, the output tensor,
//!   and the output's canonical `ImageRgb8` bytes and `output_root` (RFC-0003 §I.3);
//! * `encoding.json` — byte strings `TirProgramV2::decode_canonical` / `TirProgram::decode_canonical`
//!   must accept or refuse, with the class.
//!
//! The test regenerates every file and requires the bytes on disk to be identical.
//! `TIR_V2_BLESS=1 cargo test -p misaka-palw-tir --test golden_v2` rewrites them (a separate switch
//! from version 1's `TIR_BLESS`, so blessing one can never rewrite the other).

mod v2common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use misaka_palw_gen::output::{OutputSpecV1, output_root_v1};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::interp::CommitRecord;
use misaka_palw_tir::interp_v2::{InterpreterV2, MapInputs, StepOutputV2};
use misaka_palw_tir::pipeline::*;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::program_v2::*;
use misaka_palw_tir::{ConeEnv, DType, MapParams, Ref, RunState, Tensor, TirErrorKind};
use serde::Serialize;
use v2common::*;

const SPEC: &str = "docs/spec/palw/04b-tensor-ir.md §15";
const GRAPH_IR_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/tir/graph-ir-root/v1";

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn root(bytes: &[u8]) -> String {
    hex(blake2b_simd::Params::new().hash_length(64).key(GRAPH_IR_ROOT_DOMAIN_V1).hash(bytes).as_bytes())
}

fn check_or_bless(rel: &str, json: String) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("consensus-vectors").join("tir-v2").join(rel);
    let json = json + "\n";
    if std::env::var("TIR_V2_BLESS").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &json).unwrap();
        return;
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{} is missing; run with TIR_V2_BLESS=1", path.display()));
    assert!(on_disk == json, "{} differs from the regenerated vectors", path.display());
}

#[derive(Serialize, Clone)]
struct TensorJson {
    dtype: String,
    shape: Vec<u64>,
    data: Vec<String>,
}

fn tj(t: &Tensor) -> TensorJson {
    TensorJson {
        dtype: t.dtype.name().into(),
        shape: t.shape.iter().map(|d| *d as u64).collect(),
        data: t.data.iter().map(|v| v.to_string()).collect(),
    }
}

#[derive(Serialize)]
struct ParamJson {
    param: u16,
    layer: Option<u16>,
    le_hex: String,
}

fn params_json(m: &MapParams) -> Vec<ParamJson> {
    m.tensors.iter().map(|((j, l), t)| ParamJson { param: *j, layer: *l, le_hex: hex(&t.to_le_bytes()) }).collect()
}

#[derive(Serialize)]
struct InputJson {
    input: u16,
    /// `null`: this value at every position.
    pos: Option<u32>,
    value: TensorJson,
}

#[derive(Serialize)]
struct CommitJson {
    slot: u32,
    block: u8,
    layer: Option<u16>,
    node: u16,
    value: TensorJson,
}

fn commits_json(c: &[CommitRecord]) -> Vec<CommitJson> {
    c.iter().map(|r| CommitJson { slot: r.slot, block: r.block, layer: r.layer, node: r.node, value: tj(&r.value) }).collect()
}

#[derive(Serialize)]
struct StepJson {
    pos: u32,
    token: u32,
    output: TensorJson,
    commits: Vec<CommitJson>,
}

fn steps_json(steps: &[StepOutputV2], tokens: &[u32]) -> Vec<StepJson> {
    steps
        .iter()
        .zip(tokens)
        .map(|(s, t)| StepJson { pos: s.pos, token: *t, output: tj(&s.output), commits: commits_json(&s.commits) })
        .collect()
}

#[derive(Serialize)]
struct StateJson {
    state: u16,
    layer: Option<u16>,
    value: TensorJson,
}

#[derive(Serialize)]
struct ConeJson {
    name: String,
    block: u8,
    layer: Option<u16>,
    target: u16,
    pos: u32,
    token: Option<u32>,
    carry_in: BTreeMap<u8, TensorJson>,
    fixed: BTreeMap<u16, TensorJson>,
    hist_prior: BTreeMap<u16, Vec<TensorJson>>,
    inputs: BTreeMap<u16, TensorJson>,
    expect: TensorJson,
}

#[derive(Serialize)]
struct RefusalJson {
    what: String,
    input: u16,
    value: Option<TensorJson>,
    expect_error: String,
}

#[derive(Serialize)]
struct ProgramFileJson {
    format: String,
    spec: String,
    name: String,
    output_kind: String,
    program_borsh_hex: String,
    graph_ir_root_hex: String,
    params: Vec<ParamJson>,
    inputs: Vec<InputJson>,
    steps: Vec<StepJson>,
    state_after: Vec<StateJson>,
    cones: Vec<ConeJson>,
    refusals: Vec<RefusalJson>,
}

fn inputs_json(m: &MapInputs) -> Vec<InputJson> {
    let mut out: Vec<InputJson> = m.constant.iter().map(|(k, t)| InputJson { input: *k, pos: None, value: tj(t) }).collect();
    out.extend(m.at.iter().map(|((k, p), t)| InputJson { input: *k, pos: Some(*p), value: tj(t) }));
    out
}

/// The carry-out values of the occurrence before `occ` at one position, from its commit records.
fn carry_before(p: &TirProgramV2, step: &StepOutputV2, occ: usize) -> BTreeMap<u8, Tensor> {
    let (block, layer) = p.occurrences()[occ - 1];
    let b = &p.blocks[block as usize];
    b.carry_out
        .iter()
        .enumerate()
        .map(|(k, n)| {
            let v = step.commits.iter().find(|c| c.block == block && c.layer == layer && c.node == *n).unwrap().value.clone();
            (k as u8, v)
        })
        .collect()
}

struct ProgramCase {
    name: &'static str,
    program: TirProgramV2,
    params: MapParams,
    inputs: MapInputs,
    tokens: Vec<u32>,
}

fn program_file(
    c: &ProgramCase,
    cones: Vec<(&'static str, usize, u16, u32)>,
    refusals: Vec<(&'static str, u16, Option<Tensor>, TirErrorKind)>,
) -> String {
    let p = &c.program;
    let interp = InterpreterV2::new(p).unwrap();
    let mut state = RunState::default();
    let steps: Vec<StepOutputV2> = c.tokens.iter().map(|t| interp.step(&c.params, &c.inputs, &mut state, *t).unwrap()).collect();
    let state_after = state.fixed.iter().map(|((j, l), t)| StateJson { state: *j, layer: *l, value: tj(t) }).collect();
    // Cones: (name, occurrence, target node, position), evaluated from the committed carry-ins.
    let cones = cones
        .into_iter()
        .map(|(name, occ, target, pos)| {
            let (block, layer) = p.occurrences()[occ];
            let step = &steps[pos as usize];
            let carry_in = if occ == 0 { BTreeMap::new() } else { carry_before(p, step, occ) };
            let mut env = ConeEnv { token: Some(c.tokens[pos as usize]), pos, carry_in: carry_in.clone(), ..Default::default() };
            // Every Fixed state's value at the start of `pos`, replayed from the run.
            let mut replay = RunState::default();
            for q in 0..pos {
                interp.step(&c.params, &c.inputs, &mut replay, c.tokens[q as usize]).unwrap();
            }
            for (j, s) in p.states.iter().enumerate() {
                if let misaka_palw_tir::program::StateKind::Fixed { .. } = s.kind {
                    let v = replay
                        .fixed
                        .get(&(j as u16, layer.filter(|_| s.per_layer)))
                        .cloned()
                        .unwrap_or_else(|| Tensor::zeros(s.dtype, &s.shape.iter().map(|d| *d as usize).collect::<Vec<_>>()));
                    env.fixed.insert(j as u16, v);
                } else {
                    let rows: Vec<Tensor> = replay
                        .hist
                        .get(&(j as u16, layer.filter(|_| s.per_layer)))
                        .map(|r| r.iter().cloned().collect())
                        .unwrap_or_default();
                    env.hist_prior.insert(j as u16, rows);
                }
            }
            let expect = interp.eval_cone(block, layer, target, &c.params, &c.inputs, &env).unwrap();
            let used: BTreeMap<u16, TensorJson> = (0..p.inputs.len() as u16)
                .filter_map(|k| misaka_palw_tir::interp_v2::InputProvider::input(&c.inputs, k, pos).map(|t| (k, tj(&t))))
                .collect();
            ConeJson {
                name: name.into(),
                block,
                layer,
                target,
                pos,
                token: env.token,
                carry_in: carry_in.iter().map(|(k, t)| (*k, tj(t))).collect(),
                fixed: env.fixed.iter().map(|(j, t)| (*j, tj(t))).collect(),
                hist_prior: env.hist_prior.iter().map(|(j, r)| (*j, r.iter().map(tj).collect())).collect(),
                inputs: used,
                expect: tj(&expect),
            }
        })
        .collect();
    let refusals = refusals
        .into_iter()
        .map(|(what, k, value, want)| {
            let mut inputs = c.inputs.clone();
            match &value {
                Some(t) => {
                    inputs.constant.insert(k, t.clone());
                    inputs.at.retain(|(kk, _), _| *kk != k);
                }
                None => {
                    inputs.constant.remove(&k);
                    inputs.at.retain(|(kk, _), _| *kk != k);
                }
            }
            let got = interp.step(&c.params, &inputs, &mut RunState::default(), c.tokens[0]).unwrap_err().kind;
            assert_eq!(got, want, "{}: {what}", c.name);
            RefusalJson { what: what.into(), input: k, value: value.as_ref().map(tj), expect_error: format!("{want:?}") }
        })
        .collect();
    let bytes = p.encode();
    let file = ProgramFileJson {
        format: "palw-tir-v2/program-vectors/1".into(),
        spec: SPEC.into(),
        name: c.name.into(),
        output_kind: p.output.name().into(),
        program_borsh_hex: hex(&bytes),
        graph_ir_root_hex: root(&bytes),
        params: params_json(&c.params),
        inputs: inputs_json(&c.inputs),
        steps: steps_json(&steps, &c.tokens),
        state_after,
        cones,
        refusals,
    };
    serde_json::to_string_pretty(&file).unwrap()
}

#[test]
fn programs() {
    let random = GenRandom { seed: [0x2a; 32], position: 0 };
    // The encoder: a Rows program over a token run with a Hist window.
    let enc = encoder_program();
    let enc_case = ProgramCase {
        name: "encoder-rows",
        params: materialize_v2(&enc, 100),
        program: enc,
        inputs: MapInputs::default(),
        tokens: vec![1, 5, 6, 7, 2],
    };
    let row = enc_case.program.output.node();
    check_or_bless("programs/encoder-rows.json", program_file(&enc_case, vec![("post row at position 3", 1, row, 3)], vec![]));
    // The denoiser: random, external and per-step inputs, and the latent written in post.
    let den = denoiser_program();
    let mut rng = Lcg(99);
    let den_inputs = denoiser_inputs(&random, cond_rows(&mut rng), 3, 24, 1, 3);
    let den_case = ProgramCase {
        name: "denoiser-final",
        params: materialize_v2(&den, 101),
        program: den.clone(),
        inputs: den_inputs,
        tokens: vec![0; 3],
    };
    let post_occ = den.occurrences().len() - 1;
    let out_node = den.output.node();
    check_or_bless(
        "programs/denoiser-final.json",
        program_file(
            &den_case,
            vec![
                ("pre x at position 0 (the noise)", 0, pre_carry(&den, 0), 0),
                ("pre x at position 2 (the latent)", 0, pre_carry(&den, 0), 2),
                ("post write at position 1", post_occ, out_node, 1),
            ],
            vec![
                ("the guidance is missing", IN_GUIDANCE, None, TirErrorKind::Missing),
                ("guidance above 160/16", IN_GUIDANCE, Some(Tensor::scalar(DType::I32, 161).unwrap()), TirErrorKind::Operand),
                (
                    "a step-set index of the wrong dtype",
                    IN_STEPS_IDX,
                    Some(Tensor::scalar(DType::I32, 1).unwrap()),
                    TirErrorKind::Operand,
                ),
                ("a row count above 5", IN_COND_LEN, Some(Tensor::scalar(DType::Idx, 6).unwrap()), TirErrorKind::Operand),
                (
                    "a noise value outside the Gaussian table",
                    IN_NOISE,
                    Some(Tensor::new(DType::I32, vec![3, 2, 2], vec![72_560_102; 12]).unwrap()),
                    TirErrorKind::Operand,
                ),
                ("the per-step jitter is missing", IN_JITTER, None, TirErrorKind::Missing),
            ],
        ),
    );
    // The decoder: one position, pixels proved in [0, 255].
    let dec = decoder_program();
    let mut dec_inputs = MapInputs::default();
    dec_inputs
        .constant
        .insert(0, Tensor::new(DType::I32, vec![LAT as usize], (0..LAT as i128).map(|i| i * 777 - 5000).collect()).unwrap());
    let pixels = pre_carry(&dec, 0);
    let dec_case =
        ProgramCase { name: "decoder-final", params: MapParams::default(), program: dec, inputs: dec_inputs, tokens: vec![0] };
    check_or_bless(
        "programs/decoder-final.json",
        program_file(
            &dec_case,
            vec![("pre pixels", 0, pixels, 0)],
            vec![(
                "a latent outside the input's interval",
                0,
                Some(Tensor::new(DType::I32, vec![LAT as usize], vec![LATENT_HI as i128 + 1; LAT as usize]).unwrap()),
                TirErrorKind::Operand,
            )],
        ),
    );
}

#[derive(Serialize)]
struct PipelineProgramJson {
    program_borsh_hex: String,
    graph_ir_root_hex: String,
    params: Vec<ParamJson>,
}

#[derive(Serialize)]
struct JobJson {
    prompt: Vec<u32>,
    negative: Vec<u32>,
    steps: u32,
    scalars: Vec<String>,
}

#[derive(Serialize)]
struct RandomInputJson {
    stage: u8,
    input: u16,
    domain: u16,
    dist: String,
    step: u32,
    value: TensorJson,
}

#[derive(Serialize)]
struct StageJson {
    name: String,
    trip: u32,
    tokens: Vec<u32>,
    steps: Vec<StepJson>,
}

#[derive(Serialize)]
struct OutputImageJson {
    spec_borsh_hex: String,
    tile_len: u32,
    canonical_hex: String,
    output_root_hex: String,
}

#[derive(Serialize)]
struct PipelineFileJson {
    format: String,
    spec: String,
    name: String,
    pipeline_borsh_hex: String,
    programs: Vec<PipelineProgramJson>,
    job: JobJson,
    seed_hex: String,
    item_index: u32,
    random_inputs: Vec<RandomInputJson>,
    stages: Vec<StageJson>,
    output: TensorJson,
    output_image: OutputImageJson,
}

#[test]
fn pipelines() {
    let (p, programs) = toy_pipeline();
    let params = ProgramParams(programs.iter().enumerate().map(|(i, prog)| materialize_v2(prog, 100 + i as u64)).collect());
    let job = toy_job();
    let random = GenRandom { seed: [0x2a; 32], position: 0 };
    let run = run_pipeline(&p, &programs, &params, &random, &job).unwrap();
    // Every value R drew, so a second implementation can check the run without R, and R without the run.
    let mut random_inputs = Vec::new();
    for (s, st) in p.stages.iter().enumerate() {
        let prog = &programs[st.program as usize];
        for (k, d) in prog.inputs.iter().enumerate() {
            if let InputSource::Random { domain, dist, per_step } = d.source {
                let steps: Vec<u32> = if per_step { (0..run.stages[s].trip).collect() } else { vec![0] };
                for step in steps {
                    random_inputs.push(RandomInputJson {
                        stage: s as u8,
                        input: k as u16,
                        domain,
                        dist: format!("{dist:?}"),
                        step,
                        value: tj(&random.random(domain, dist, step, &d.shape).unwrap()),
                    });
                }
            }
        }
    }
    let stages = run
        .stages
        .iter()
        .zip(&p.stages)
        .map(|(r, st)| {
            let tokens = if r.tokens.is_empty() { vec![0; r.trip as usize] } else { r.tokens.clone() };
            StageJson { name: st.name.clone(), trip: r.trip, tokens: r.tokens.clone(), steps: steps_json(&r.steps, &tokens) }
        })
        .collect();
    // The class output as RFC-0003's canonical ImageRgb8 and its output root (tiles of 4 lanes).
    let spec = OutputSpecV1::image_rgb8(2, 2);
    let values: Vec<i64> = run.output.data.iter().map(|v| *v as i64).collect();
    let canonical = spec.canonical_bytes(&values).unwrap();
    let out_root = output_root_v1(&spec, &values, 4).unwrap();
    let file = PipelineFileJson {
        format: "palw-tir-v2/pipeline-vectors/1".into(),
        spec: SPEC.into(),
        name: "toy-image".into(),
        pipeline_borsh_hex: hex(&p.encode()),
        programs: programs
            .iter()
            .zip(&params.0)
            .map(|(prog, m)| PipelineProgramJson {
                program_borsh_hex: hex(&prog.encode()),
                graph_ir_root_hex: root(&prog.encode()),
                params: params_json(m),
            })
            .collect(),
        job: JobJson {
            prompt: job.prompt.clone(),
            negative: job.negative.clone(),
            steps: job.steps,
            scalars: job.scalars.iter().map(|v| v.to_string()).collect(),
        },
        seed_hex: hex(&random.seed),
        item_index: random.position,
        random_inputs,
        stages,
        output: tj(&run.output),
        output_image: OutputImageJson {
            spec_borsh_hex: hex(&spec.encode()),
            tile_len: 4,
            canonical_hex: hex(&canonical),
            output_root_hex: hex(&out_root),
        },
    };
    check_or_bless("pipelines/toy-image.json", serde_json::to_string_pretty(&file).unwrap());
}

#[derive(Serialize)]
struct EncodingCaseJson {
    name: String,
    bytes_hex: String,
    expect: String,
}

#[derive(Serialize)]
struct EncodingFileJson {
    format: String,
    spec: String,
    cases: Vec<EncodingCaseJson>,
}

fn classify(bytes: &[u8]) -> String {
    match TirProgram::decode_canonical(bytes) {
        Ok(TirProgram::V1(_)) => "ok-v1".into(),
        Ok(TirProgram::V2(_)) => "ok".into(),
        Err(e) => format!("{:?}", e.kind),
    }
}

#[test]
fn encoding() {
    let den = denoiser_program();
    let dec = decoder_program();
    let good = den.encode();
    let mut cases: Vec<(String, Vec<u8>, &str)> =
        vec![("valid denoiser".into(), good.clone(), "ok"), ("valid decoder".into(), dec.encode(), "ok")];
    let mut trailing = good.clone();
    trailing.push(0);
    cases.push(("a trailing byte".into(), trailing, "Encoding"));
    cases.push(("truncated".into(), good[..good.len() - 1].to_vec(), "Encoding"));
    cases.push(("empty".into(), vec![], "Encoding"));
    let mut v3 = good.clone();
    v3[0] = 3;
    cases.push(("version 3".into(), v3, "NormalForm"));
    // The first input's source: version, prim set, bounds, the inputs' count, then the first input's
    // name, dtype and shape.
    let head = borsh::to_vec(&(den.version, den.prim_set_id, den.token_bound, den.history_bound)).unwrap().len() + 4;
    let first = &den.inputs[0];
    let source_at = head + borsh::to_vec(&(first.name.clone(), first.dtype, first.shape.clone())).unwrap().len();
    let mut tag = good.clone();
    assert_eq!(tag[source_at], 1, "the noise input is Random");
    tag[source_at] = 2;
    cases.push(("an unknown InputSource tag".into(), tag, "Encoding"));
    let mut per_step = good.clone();
    let per_step_at = source_at + 1 + 2 + 1; // tag, domain u16, dist tag (Normal carries no field)
    assert_eq!(per_step[per_step_at], 0, "the noise input is not per step");
    per_step[per_step_at] = 2;
    cases.push(("a per_step bool of 2".into(), per_step, "Encoding"));
    let mut out_tag = good.clone();
    let n = out_tag.len();
    assert_eq!(out_tag[n - 3], 2, "the output is Final");
    out_tag[n - 3] = 3;
    cases.push(("an unknown OutputDecl tag".into(), out_tag, "Encoding"));
    let with = |f: &dyn Fn(&mut TirProgramV2)| {
        let mut p = den.clone();
        f(&mut p);
        p.encode()
    };
    cases.push((
        "domain 0 as an input".into(),
        with(&|p| p.inputs[0].source = InputSource::Random { domain: 0, dist: RandomDist::Normal, per_step: false }),
        "NormalForm",
    ));
    cases.push((
        "an unused input".into(),
        with(&|p| {
            p.inputs.push(InputDecl {
                name: "unused".into(),
                dtype: DType::I32,
                shape: vec![1],
                source: InputSource::External { lo: 0, hi: 1 },
            })
        }),
        "NormalForm",
    ));
    cases.push((
        "a Logits program writes in post".into(),
        with(&|p| p.output = OutputDecl::Logits { node: p.output.node(), scheme_id: [0; 64] }),
        "NormalForm",
    ));
    cases.push(("an uncommitted output".into(), denoiser_variant(false, true).encode(), "NormalForm"));
    // Version 1, through the dispatcher: decoded exactly as before.
    let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let t = b.cast(Ref::Input(0), DType::I32);
        b.finish(&[t])
    };
    let post = {
        let mut b = pb.block("post", vec![misaka_palw_tir::TensorType::scalar(DType::I32)]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.commit(l);
        b.finish(&[])
    };
    cases.push(("a version-1 program".into(), pb.finish(pre, vec![], post, 0).encode(), "ok-v1"));
    let cases = cases
        .into_iter()
        .map(|(name, bytes, expect)| {
            assert_eq!(classify(&bytes), expect, "{name}");
            EncodingCaseJson { name, bytes_hex: hex(&bytes), expect: expect.into() }
        })
        .collect();
    let file = EncodingFileJson { format: "palw-tir-v2/encoding-vectors/1".into(), spec: SPEC.into(), cases };
    check_or_bless("encoding.json", serde_json::to_string_pretty(&file).unwrap());
}
