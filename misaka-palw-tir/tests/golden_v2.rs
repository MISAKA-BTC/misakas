//! **The golden vectors of program version 2** (`consensus-vectors/tir-v2/`, spec 04b §15.8).
//!
//! * `programs/<name>.json` — the three toy stage programs: canonical bytes, identity, params, the
//!   inputs of every position, every position's output and commit points, the `Fixed` states after
//!   the run, cone cases, and input refusals;
//! * `pipelines/toy-image.json` — the whole toy text-to-image pipeline: its bytes, the programs,
//!   the job, the seed, every random input `R` drew, every stage's positions, the output tensor,
//!   and the output's canonical `ImageRgb8` bytes and `output_root` (RFC-0003 §I.3);
//! * `pipelines/toy-vision.json` — a one-stage image encoder over a job image (`JobImage`, RFC-0003
//!   §II.4): the image's bytes, its `input_root`, every input tile with its path, the run, and the
//!   `EmbeddingI32` output;
//! * `pipelines/toy-vlm.json` — the vision-language pipeline (RFC-0003 §II.2.1): the image, a prompt
//!   with two placeholder ids, the greedy selector's generated ids, and every stage's positions —
//!   the text stage's logits rows included;
//! * `pipelines/toy-encdec.json` — the encoder–decoder pipeline (RFC-0003 §II.2.2): the job's source
//!   (`TokenSource::Source`) through the encoder, a prompt that starts with the class's forced start
//!   id, the greedy selector's generated ids, and every stage's positions;
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
    /// Absent for a job with no image (the text-to-image and bidirectional files predate images).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<JobImageJson>,
    /// The text stage's generated ids (absent for a pipeline with no text stage).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    generated: Vec<u32>,
    /// The job's source ids (RFC-0003 §II.2.2; absent for a job with none — every earlier file).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    source: Vec<u32>,
}

/// A job image: what the executor holds (the bytes) and what the chain holds (`input_root`), with
/// every input tile and its path — what a court opens (RFC-0003 §II.4).
#[derive(Serialize)]
struct JobImageJson {
    h: u32,
    w: u32,
    rgb_hex: String,
    /// The class's input tile length for this image's slot.
    tile_len: u32,
    input_root_hex: String,
    tiles: Vec<InputTileJson>,
}

#[derive(Serialize)]
struct InputTileJson {
    tile: u64,
    bytes_hex: String,
    proof_hex: Vec<String>,
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
            images: vec![],
            generated: vec![],
            source: vec![],
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

// ---- admission (spec 04b §15.9) ------------------------------------------------------------------

#[derive(Serialize)]
struct CostJson {
    macs: String,
    elementwise: String,
    transcendentals: String,
    bytes_read: String,
    bytes_written: String,
}

fn cost_json(c: &misaka_palw_tir::admit::CostV1) -> CostJson {
    CostJson {
        macs: c.macs.to_string(),
        elementwise: c.elementwise.to_string(),
        transcendentals: c.transcendentals.to_string(),
        bytes_read: c.bytes_read.to_string(),
        bytes_written: c.bytes_written.to_string(),
    }
}

#[derive(Serialize)]
struct PositionJson {
    cost: CostJson,
    state_bytes: String,
    peak_live_bytes: String,
    commit_lanes: String,
    step_leaves: String,
}

fn position_json(p: &misaka_palw_tir::admit::PositionV1) -> PositionJson {
    PositionJson {
        cost: cost_json(&p.cost),
        state_bytes: p.state_bytes.to_string(),
        peak_live_bytes: p.peak_live_bytes.to_string(),
        commit_lanes: p.commit_lanes.to_string(),
        step_leaves: p.step_leaves.to_string(),
    }
}

#[derive(Serialize)]
struct ConeJson2 {
    block: String,
    node: String,
    leaves: Vec<String>,
    tiles: String,
    tile: CostJson,
    tile_opened_bytes: String,
    operands: String,
    h_reductions: Vec<String>,
}

#[derive(Serialize)]
struct InputAdmissionJson {
    interval: [String; 2],
    leaf: String,
}

#[derive(Serialize)]
struct StateCkptJson {
    state: String,
    closure: Vec<String>,
    groups: String,
    interval: String,
}

#[derive(Serialize)]
struct ProgramAdmissionJson {
    first_input_param: String,
    inputs: Vec<InputAdmissionJson>,
    post_written: Vec<String>,
    checkpoint_interval: String,
    cone_work: String,
    position: PositionJson,
    states: Vec<StateCkptJson>,
    cones: Vec<ConeJson2>,
    intervals: Vec<Vec<[String; 2]>>,
}

#[derive(Serialize)]
struct AdmitRefusalJson {
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cap: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    class: Option<String>,
}

#[derive(Serialize)]
struct ProgramAdmissionCaseJson {
    name: String,
    program_borsh_hex: String,
    expect: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    refusal: Option<AdmitRefusalJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    admission: Option<ProgramAdmissionJson>,
}

#[derive(Serialize)]
struct StageAdmissionJson {
    stage: String,
    max_trip: String,
    inputs: Vec<InputAdmissionJson>,
    position: PositionJson,
    job_cost: CostJson,
    job_step_leaves: String,
    cones: Vec<ConeJson2>,
}

#[derive(Serialize)]
struct PipelineAdmissionJson {
    job_cost: CostJson,
    job_step_leaves: String,
    cone_work: String,
    output_interval: [String; 2],
    stages: Vec<StageAdmissionJson>,
}

#[derive(Serialize)]
struct PipelineAdmissionCaseJson {
    name: String,
    pipeline_borsh_hex: String,
    programs_borsh_hex: Vec<String>,
    job_ceilings: JobCeilingsJson,
    expect: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    refusal: Option<AdmitRefusalJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    admission: Option<PipelineAdmissionJson>,
}

#[derive(Serialize)]
struct CeilingsJson {
    max_tile_macs: String,
    max_tile_transcendentals: String,
    max_tile_opened_bytes: String,
    max_tile_operands: String,
    max_position_macs: String,
    max_position_transcendentals: String,
    max_state_bytes: String,
    max_step_leaves: String,
    max_checkpoint_interval: String,
    max_cone_work: String,
}

#[derive(Serialize)]
struct JobCeilingsJson {
    max_job_macs: String,
    max_job_transcendentals: String,
    max_job_step_leaves: String,
    max_job_cone_work: String,
}

#[derive(Serialize)]
struct AdmissionFileJson {
    format: String,
    spec: String,
    tile_len: String,
    h_chunk: String,
    ceilings: CeilingsJson,
    programs: Vec<ProgramAdmissionCaseJson>,
    pipelines: Vec<PipelineAdmissionCaseJson>,
}

fn leaf_text(l: &misaka_palw_tir::admit::LeafV1, first_input: u16) -> String {
    use misaka_palw_tir::admit::LeafV1 as L;
    match l {
        L::Commit(n) => format!("commit:{n}"),
        L::CarryIn(k) => format!("carry_in:{k}"),
        L::State(s) => format!("state:{s}"),
        L::History(s) => format!("history:{s}"),
        L::Param(j) if *j >= first_input => format!("input:{}", j - first_input),
        L::Param(j) => format!("param:{j}"),
        L::Const(j) => format!("const:{j}"),
        L::Input(j) => format!("ref_input:{j}"),
    }
}

fn cones_json(a: &misaka_palw_tir::admit_v2::TirAdmissionV2) -> Vec<ConeJson2> {
    a.view
        .cones
        .iter()
        .map(|k| ConeJson2 {
            block: k.block.to_string(),
            node: k.node.to_string(),
            leaves: k.leaves.iter().map(|l| leaf_text(l, a.first_input_param)).collect(),
            tiles: k.tiles.to_string(),
            tile: cost_json(&k.tile),
            tile_opened_bytes: k.tile_opened_bytes.to_string(),
            operands: k.operands.to_string(),
            h_reductions: k.h_reductions.iter().map(|n| n.to_string()).collect(),
        })
        .collect()
}

fn inputs_admission_json(a: &misaka_palw_tir::admit_v2::TirAdmissionV2) -> Vec<InputAdmissionJson> {
    a.inputs
        .iter()
        .map(|i| InputAdmissionJson {
            interval: [i.interval.lo.to_string(), i.interval.hi.to_string()],
            leaf: format!("{:?}", i.leaf),
        })
        .collect()
}

fn refusal_json(e: &misaka_palw_tir::admit::TirAdmitError) -> AdmitRefusalJson {
    use misaka_palw_tir::admit::TirAdmitError as E;
    match e {
        E::Exceeds { limit, at, value, cap } => AdmitRefusalJson {
            kind: "exceeds".into(),
            limit: Some(limit.to_string()),
            at: Some(at.clone()),
            value: Some(value.to_string()),
            cap: Some(cap.to_string()),
            class: None,
        },
        E::Program(t) => AdmitRefusalJson {
            kind: "program".into(),
            limit: None,
            at: None,
            value: None,
            cap: None,
            class: Some(format!("{:?}", t.kind)),
        },
        E::Inputs(_) => AdmitRefusalJson { kind: "inputs".into(), limit: None, at: None, value: None, cap: None, class: None },
    }
}

#[test]
fn admission() {
    use misaka_palw_tir::admit::{TirAdmitInputsV1, TirCeilingsV1};
    use misaka_palw_tir::admit_v2::*;
    let inputs = TirAdmitInputsV1 { tile_len: 64, h_chunk: 16, ceilings: TirCeilingsV1::legacy_court_v1() };
    let c = &inputs.ceilings;
    let mut wide = denoiser_program();
    wide.inputs[IN_GUIDANCE as usize].source = InputSource::External { lo: 0, hi: i32::MAX as i64 };
    let programs: Vec<(&str, TirProgramV2)> = vec![
        ("encoder-rows", encoder_program()),
        ("denoiser-final", denoiser_program()),
        ("decoder-final", decoder_program()),
        ("refused: guidance up to i32::MAX overflows the update", wide),
    ];
    let programs = programs
        .into_iter()
        .map(|(name, p)| {
            let bytes = p.encode();
            let (expect, refusal, admission) = match tir_admit_v2(&bytes, &inputs) {
                Ok(a) => {
                    let adm = ProgramAdmissionJson {
                        first_input_param: a.first_input_param.to_string(),
                        inputs: inputs_admission_json(&a),
                        post_written: a.post_written.iter().map(|s| s.to_string()).collect(),
                        checkpoint_interval: a.view.checkpoint_interval.to_string(),
                        cone_work: a.view.cone_work.to_string(),
                        position: position_json(&a.view.position),
                        states: a
                            .view
                            .states
                            .iter()
                            .map(|s| StateCkptJson {
                                state: s.state.to_string(),
                                closure: s.closure.iter().map(|t| t.to_string()).collect(),
                                groups: s.groups.to_string(),
                                interval: s.interval.to_string(),
                            })
                            .collect(),
                        cones: cones_json(&a),
                        intervals: a
                            .view
                            .intervals
                            .iter()
                            .map(|b| b.iter().map(|i| [i.lo.to_string(), i.hi.to_string()]).collect())
                            .collect(),
                    };
                    ("admitted".to_string(), None, Some(adm))
                }
                Err(e) => ("refused".to_string(), Some(refusal_json(&e)), None),
            };
            ProgramAdmissionCaseJson { name: name.into(), program_borsh_hex: hex(&bytes), expect, refusal, admission }
        })
        .collect();
    let open = TirJobCeilingsV1::open_v1();
    let toy = toy_pipeline();
    let toy_leaves = tir_admit_pipeline_v1(&toy.0.encode(), &toy.1.iter().map(|p| p.encode()).collect::<Vec<_>>(), &inputs, &open)
        .unwrap()
        .job_step_leaves;
    let pipelines: Vec<(&str, (TirPipelineV1, Vec<TirProgramV2>), TirJobCeilingsV1)> = vec![
        ("toy-image", toy_pipeline(), open),
        ("toy-bidirectional (JobTokens + JobTokenCount)", bidirectional_pipeline(0), open),
        ("matmul (the job's MACs)", matmul_pipeline(), open),
        ("toy-vision (JobImage: 1 byte a lane, an operand)", vision_pipeline(), open),
        ("toy-text (the text stage: Logits over TextStream)", text_pipeline(), open),
        ("toy-vlm (image rows into the text stage)", vlm_pipeline(), open),
        ("refused: max_job_macs one short", matmul_pipeline(), TirJobCeilingsV1 { max_job_macs: 3 * 512 - 1, ..open }),
        ("refused: max_job_step_leaves one short", toy_pipeline(), TirJobCeilingsV1 { max_job_step_leaves: toy_leaves - 1, ..open }),
    ];
    let pipelines = pipelines
        .into_iter()
        .map(|(name, (p, progs), job)| {
            let pb = p.encode();
            let bytes: Vec<Vec<u8>> = progs.iter().map(|x| x.encode()).collect();
            let (expect, refusal, admission) = match tir_admit_pipeline_v1(&pb, &bytes, &inputs, &job) {
                Ok(a) => {
                    let adm = PipelineAdmissionJson {
                        job_cost: cost_json(&a.job_cost),
                        job_step_leaves: a.job_step_leaves.to_string(),
                        cone_work: a.cone_work.to_string(),
                        output_interval: [a.output_interval.lo.to_string(), a.output_interval.hi.to_string()],
                        stages: a
                            .stages
                            .iter()
                            .map(|s| StageAdmissionJson {
                                stage: s.stage.to_string(),
                                max_trip: s.max_trip.to_string(),
                                inputs: inputs_admission_json(&s.admission),
                                position: position_json(&s.admission.view.position),
                                job_cost: cost_json(&s.job_cost),
                                job_step_leaves: s.job_step_leaves.to_string(),
                                cones: cones_json(&s.admission),
                            })
                            .collect(),
                    };
                    ("admitted".to_string(), None, Some(adm))
                }
                Err(e) => ("refused".to_string(), Some(refusal_json(&e)), None),
            };
            PipelineAdmissionCaseJson {
                name: name.into(),
                pipeline_borsh_hex: hex(&pb),
                programs_borsh_hex: bytes.iter().map(|b| hex(b)).collect(),
                job_ceilings: JobCeilingsJson {
                    max_job_macs: job.max_job_macs.to_string(),
                    max_job_transcendentals: job.max_job_transcendentals.to_string(),
                    max_job_step_leaves: job.max_job_step_leaves.to_string(),
                    max_job_cone_work: job.max_job_cone_work.to_string(),
                },
                expect,
                refusal,
                admission,
            }
        })
        .collect();
    let file = AdmissionFileJson {
        format: "palw-tir-v2/admission-vectors/1".into(),
        spec: SPEC.into(),
        tile_len: inputs.tile_len.to_string(),
        h_chunk: inputs.h_chunk.to_string(),
        ceilings: CeilingsJson {
            max_tile_macs: c.max_tile_macs.to_string(),
            max_tile_transcendentals: c.max_tile_transcendentals.to_string(),
            max_tile_opened_bytes: c.max_tile_opened_bytes.to_string(),
            max_tile_operands: c.max_tile_operands.to_string(),
            max_position_macs: c.max_position_macs.to_string(),
            max_position_transcendentals: c.max_position_transcendentals.to_string(),
            max_state_bytes: c.max_state_bytes.to_string(),
            max_step_leaves: c.max_step_leaves.to_string(),
            max_checkpoint_interval: c.max_checkpoint_interval.to_string(),
            max_cone_work: c.max_cone_work.to_string(),
        },
        programs,
        pipelines,
    };
    check_or_bless("admission.json", serde_json::to_string_pretty(&file).unwrap());
}

// ---- demand evaluation (spec 04b §15.4) ------------------------------------------------------------

#[derive(Serialize)]
struct DemandWorkJson {
    elements: String,
    terms: String,
}

#[derive(Serialize)]
struct DemandExpectJson {
    values: Vec<String>,
    work: DemandWorkJson,
    /// `[pos, input, indices]`, the input questions asked, grouped (indices as inclusive runs).
    input_requests: Vec<(u32, u16, String)>,
}

#[derive(Serialize)]
struct DemandCaseJson {
    name: String,
    target: serde_json::Value,
    elements: Vec<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    withhold_input: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expect: Option<DemandExpectJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expect_error: Option<String>,
}

#[derive(Serialize)]
struct DemandFileJson {
    format: String,
    spec: String,
    /// The program vector whose params, inputs, tokens and steps are the source's committed data;
    /// the source answers `Fixed` values at position 0 (the initial zeros) and `Replay` elsewhere.
    program: String,
    limits: DemandWorkJson,
    cases: Vec<DemandCaseJson>,
}

fn runs(indices: &[usize]) -> String {
    let mut v: Vec<usize> = indices.to_vec();
    v.sort_unstable();
    v.dedup();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let mut j = i;
        while j + 1 < v.len() && v[j + 1] == v[j] + 1 {
            j += 1;
        }
        out.push(if i == j { v[i].to_string() } else { format!("{}-{}", v[i], v[j]) });
        i = j + 1;
    }
    out.join(",")
}

fn demand_file(name: &str, c: &ProgramCase) -> String {
    use misaka_palw_tir::demand::{DemandContext, DemandError, DemandLimits, DemandRequest, DemandTarget};
    use misaka_palw_tir::demand_v2::{MapSourceV2, commit_occurrence_v2, eval_demanded_v2};
    use misaka_palw_tir::validate_v2::validate_v2;
    let p = &c.program;
    let info = validate_v2(p).unwrap();
    let interp = InterpreterV2::new(p).unwrap();
    let mut state = RunState::default();
    let steps: Vec<StepOutputV2> = c.tokens.iter().map(|t| interp.step(&c.params, &c.inputs, &mut state, *t).unwrap()).collect();
    let limits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };
    let mut cases = Vec::new();
    let mut case = |name: String, target: DemandTarget, json: serde_json::Value, elements: Vec<usize>, withhold: Option<u16>| {
        let mut src = MapSourceV2::from_run(p, &info, &c.params, &c.inputs, &c.tokens, &steps);
        if let DemandTarget::Node { ctx, node } = target {
            src.base.nodes.remove(&(ctx, node));
        }
        if let Some(k) = withhold {
            src.withheld_inputs.insert(k, TirErrorKind::Operand);
        }
        let r = eval_demanded_v2(p, &info, &DemandRequest { target, elements: &elements }, &mut src, &limits);
        let (expect, expect_error) = match r {
            Ok((vals, work)) => {
                let mut groups: BTreeMap<(u32, u16), Vec<usize>> = BTreeMap::new();
                for (pos, k, i) in &src.input_requests {
                    groups.entry((*pos, *k)).or_default().push(*i);
                }
                (
                    Some(DemandExpectJson {
                        values: vals.iter().map(|v| v.to_string()).collect(),
                        work: DemandWorkJson { elements: work.elements.to_string(), terms: work.terms.to_string() },
                        input_requests: groups.into_iter().map(|((pos, k), is)| (pos, k, runs(&is))).collect(),
                    }),
                    None,
                )
            }
            Err(DemandError::Tir(t)) => (None, Some(format!("{:?}", t.kind))),
            Err(DemandError::WorkLimit(_)) => (None, Some("WorkLimit".into())),
        };
        cases.push(DemandCaseJson { name, target: json, elements, withhold_input: withhold, expect, expect_error });
    };
    for (pos, step) in steps.iter().enumerate() {
        for rec in &step.commits {
            let occ = commit_occurrence_v2(p, rec.block, rec.layer);
            let ctx = DemandContext { pos: pos as u32, occurrence: occ };
            let n = rec.value.data.len();
            let mut elements = vec![0, n / 2, n - 1];
            elements.dedup();
            case(
                format!("node pos {pos} occurrence {occ} node {}", rec.node),
                DemandTarget::Node { ctx, node: rec.node },
                serde_json::json!({"node": {"pos": pos, "occurrence": occ, "node": rec.node}}),
                elements,
                None,
            );
        }
    }
    for (j, s) in p.states.iter().enumerate() {
        if matches!(s.kind, misaka_palw_tir::program::StateKind::Fixed { .. }) && !s.per_layer {
            let n: usize = s.shape.iter().map(|d| *d as usize).product();
            for pos in 0..steps.len() as u32 {
                case(
                    format!("state {} after {pos}", s.name),
                    DemandTarget::StateAfter { pos, state: j as u16, layer: None },
                    serde_json::json!({"state_after": {"pos": pos, "state": j, "layer": null}}),
                    (0..n).collect(),
                    None,
                );
            }
        }
    }
    for k in 0..p.inputs.len() as u16 {
        let x = pre_carry(p, 0);
        case(
            format!("pre carry 0 at position 0 with input {k} withheld"),
            DemandTarget::Node { ctx: DemandContext { pos: 0, occurrence: 0 }, node: x },
            serde_json::json!({"node": {"pos": 0, "occurrence": 0, "node": x}}),
            vec![0],
            Some(k),
        );
    }
    let file = DemandFileJson {
        format: "palw-tir-v2/demand-vectors/1".into(),
        spec: SPEC.into(),
        program: format!("programs/{name}.json"),
        limits: DemandWorkJson { elements: limits.max_elements.to_string(), terms: limits.max_terms.to_string() },
        cases,
    };
    serde_json::to_string_pretty(&file).unwrap()
}

#[test]
fn demand() {
    let random = GenRandom { seed: [0x2a; 32], position: 0 };
    let enc = encoder_program();
    let enc_case = ProgramCase {
        name: "encoder-rows",
        params: materialize_v2(&enc, 100),
        program: enc,
        inputs: MapInputs::default(),
        tokens: vec![1, 5, 6, 7, 2],
    };
    check_or_bless("demand/encoder-rows.json", demand_file("encoder-rows", &enc_case));
    let den = denoiser_program();
    let mut rng = Lcg(99);
    let den_inputs = denoiser_inputs(&random, cond_rows(&mut rng), 3, 24, 1, 3);
    let den_case = ProgramCase {
        name: "denoiser-final",
        params: materialize_v2(&den, 101),
        program: den,
        inputs: den_inputs,
        tokens: vec![0; 3],
    };
    check_or_bless("demand/denoiser-final.json", demand_file("denoiser-final", &den_case));
}

#[test]
fn bidirectional_pipeline_vector() {
    let (p, programs) = bidirectional_pipeline(0);
    let params = ProgramParams(programs.iter().enumerate().map(|(i, prog)| materialize_v2(prog, 200 + i as u64)).collect());
    let job = PipelineJob { prompt: vec![5, 6], ..toy_job() };
    let random = GenRandom { seed: [0; 32], position: 0 };
    let run = run_pipeline(&p, &programs, &params, &random, &job).unwrap();
    // The same job under another pad id: the same output (the mask is the count, not the pad).
    let (p7, programs7) = bidirectional_pipeline(7);
    assert_eq!(run_pipeline(&p7, &programs7, &params, &random, &job).unwrap().output, run.output);
    let stages = run
        .stages
        .iter()
        .zip(&p.stages)
        .map(|(r, st)| StageJson {
            name: st.name.clone(),
            trip: r.trip,
            tokens: r.tokens.clone(),
            steps: steps_json(&r.steps, &vec![0; r.trip as usize]),
        })
        .collect();
    let spec = misaka_palw_gen::output::OutputSpecV1::embedding_i32(1, D, 0, false);
    let values: Vec<i64> = run.output.data.iter().map(|v| *v as i64).collect();
    let file = PipelineFileJson {
        format: "palw-tir-v2/pipeline-vectors/1".into(),
        spec: SPEC.into(),
        name: "toy-bidirectional".into(),
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
            images: vec![],
            generated: vec![],
            source: vec![],
        },
        seed_hex: hex(&random.seed),
        item_index: random.position,
        random_inputs: vec![],
        stages,
        output: tj(&run.output),
        output_image: OutputImageJson {
            spec_borsh_hex: hex(&spec.encode()),
            tile_len: 4,
            canonical_hex: hex(&spec.canonical_bytes(&values).unwrap()),
            output_root_hex: hex(&misaka_palw_gen::output::output_root_v1(&spec, &values, 4).unwrap()),
        },
    };
    check_or_bless("pipelines/toy-bidirectional.json", serde_json::to_string_pretty(&file).unwrap());
}

#[test]
fn vision_pipeline_vector() {
    let (p, programs) = vision_pipeline();
    let params = ProgramParams(programs.iter().enumerate().map(|(i, prog)| materialize_v2(prog, 300 + i as u64)).collect());
    let job = vision_job();
    let random = GenRandom { seed: [0; 32], position: 0 };
    let run = run_pipeline(&p, &programs, &params, &random, &job).unwrap();
    let tile_len = 4;
    let images = job
        .images
        .iter()
        .map(|img| {
            let root = misaka_palw_gen::output::input_image_root_v1(img.h, img.w, tile_len, &img.rgb).unwrap();
            let tiles = misaka_palw_gen::output::input_image_tiles_v1(img.h, img.w, tile_len, &img.rgb).unwrap();
            let spec = misaka_palw_gen::output::input_image_spec_v1(img.h, img.w);
            for (t, (bytes, proof)) in tiles.iter().enumerate() {
                assert!(misaka_palw_gen::output::verify_output_tile_v1(&root, &spec, tile_len, t as u64, bytes, proof));
            }
            JobImageJson {
                h: img.h,
                w: img.w,
                rgb_hex: hex(&img.rgb),
                tile_len,
                input_root_hex: hex(&root),
                tiles: tiles
                    .iter()
                    .enumerate()
                    .map(|(t, (bytes, proof))| InputTileJson {
                        tile: t as u64,
                        bytes_hex: hex(bytes),
                        proof_hex: proof.iter().map(|h| hex(h)).collect(),
                    })
                    .collect(),
            }
        })
        .collect();
    let stages = run
        .stages
        .iter()
        .zip(&p.stages)
        .map(|(r, st)| StageJson {
            name: st.name.clone(),
            trip: r.trip,
            tokens: r.tokens.clone(),
            steps: steps_json(&r.steps, &vec![0; r.trip as usize]),
        })
        .collect();
    let spec = misaka_palw_gen::output::OutputSpecV1::embedding_i32(1, D, 0, false);
    let values: Vec<i64> = run.output.data.iter().map(|v| *v as i64).collect();
    let file = PipelineFileJson {
        format: "palw-tir-v2/pipeline-vectors/1".into(),
        spec: SPEC.into(),
        name: "toy-vision".into(),
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
            images,
            generated: vec![],
            source: vec![],
        },
        seed_hex: hex(&random.seed),
        item_index: random.position,
        random_inputs: vec![],
        stages,
        output: tj(&run.output),
        output_image: OutputImageJson {
            spec_borsh_hex: hex(&spec.encode()),
            tile_len: 4,
            canonical_hex: hex(&spec.canonical_bytes(&values).unwrap()),
            output_root_hex: hex(&misaka_palw_gen::output::output_root_v1(&spec, &values, 4).unwrap()),
        },
    };
    check_or_bless("pipelines/toy-vision.json", serde_json::to_string_pretty(&file).unwrap());
}

#[test]
fn vlm_pipeline_vector() {
    let (p, programs) = vlm_pipeline();
    let params = ProgramParams(programs.iter().enumerate().map(|(i, prog)| materialize_v2(prog, 400 + i as u64)).collect());
    let job = PipelineJob { prompt: vec![3, PLACEHOLDER, PLACEHOLDER, 5], ..vision_job() };
    let random = GenRandom { seed: [0; 32], position: 0 };
    let (run, generated) = run_text_pipeline(&p, &programs, &params, &random, &job, &mut greedy(4)).unwrap();
    // The committed ids replay the run exactly.
    let committed = PipelineJob { generated: generated.clone(), ..job.clone() };
    assert_eq!(run_pipeline(&p, &programs, &params, &random, &committed).unwrap(), run);
    let tile_len = 4;
    let images = job
        .images
        .iter()
        .map(|img| {
            let root = misaka_palw_gen::output::input_image_root_v1(img.h, img.w, tile_len, &img.rgb).unwrap();
            let tiles = misaka_palw_gen::output::input_image_tiles_v1(img.h, img.w, tile_len, &img.rgb).unwrap();
            JobImageJson {
                h: img.h,
                w: img.w,
                rgb_hex: hex(&img.rgb),
                tile_len,
                input_root_hex: hex(&root),
                tiles: tiles
                    .iter()
                    .enumerate()
                    .map(|(t, (bytes, proof))| InputTileJson {
                        tile: t as u64,
                        bytes_hex: hex(bytes),
                        proof_hex: proof.iter().map(|h| hex(h)).collect(),
                    })
                    .collect(),
            }
        })
        .collect();
    let stages = run
        .stages
        .iter()
        .zip(&p.stages)
        .map(|(r, st)| {
            let tokens = if r.tokens.is_empty() { vec![0; r.trip as usize] } else { r.tokens.clone() };
            StageJson { name: st.name.clone(), trip: r.trip, tokens: r.tokens.clone(), steps: steps_json(&r.steps, &tokens) }
        })
        .collect();
    let file = PipelineFileJson {
        format: "palw-tir-v2/pipeline-vectors/1".into(),
        spec: SPEC.into(),
        name: "toy-vlm".into(),
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
            scalars: vec![],
            images,
            generated,
            source: vec![],
        },
        seed_hex: hex(&random.seed),
        item_index: random.position,
        random_inputs: vec![],
        stages,
        output: tj(&run.output),
        // A text class's output is its generated ids: no output root (RFC-0003 §I.3.3).
        output_image: OutputImageJson {
            spec_borsh_hex: String::new(),
            tile_len: 0,
            canonical_hex: String::new(),
            output_root_hex: String::new(),
        },
    };
    check_or_bless("pipelines/toy-vlm.json", serde_json::to_string_pretty(&file).unwrap());
}

/// `pipelines/toy-encdec.json` (RFC-0003 §II.2.2): the toy encoder–decoder — the source through the
/// encoder, the prompt from the forced start id, greedy ids, every stage's positions.
#[test]
fn encdec_pipeline_vector() {
    let (p, programs) = encdec_pipeline();
    let params = ProgramParams(programs.iter().enumerate().map(|(i, prog)| materialize_v2(prog, 500 + i as u64)).collect());
    let job = encdec_job();
    let random = GenRandom { seed: [0; 32], position: 0 };
    let (run, generated) = run_text_pipeline(&p, &programs, &params, &random, &job, &mut greedy(4)).unwrap();
    let committed = PipelineJob { generated: generated.clone(), ..job.clone() };
    assert_eq!(run_pipeline(&p, &programs, &params, &random, &committed).unwrap(), run);
    let stages = run
        .stages
        .iter()
        .zip(&p.stages)
        .map(|(r, st)| {
            let tokens = if r.tokens.is_empty() { vec![0; r.trip as usize] } else { r.tokens.clone() };
            StageJson { name: st.name.clone(), trip: r.trip, tokens: r.tokens.clone(), steps: steps_json(&r.steps, &tokens) }
        })
        .collect();
    let file = PipelineFileJson {
        format: "palw-tir-v2/pipeline-vectors/1".into(),
        spec: SPEC.into(),
        name: "toy-encdec".into(),
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
            negative: vec![],
            steps: job.steps,
            scalars: vec![],
            images: vec![],
            generated,
            source: job.source.clone(),
        },
        seed_hex: hex(&random.seed),
        item_index: random.position,
        random_inputs: vec![],
        stages,
        output: tj(&run.output),
        output_image: OutputImageJson {
            spec_borsh_hex: String::new(),
            tile_len: 0,
            canonical_hex: String::new(),
            output_root_hex: String::new(),
        },
    };
    check_or_bless("pipelines/toy-encdec.json", serde_json::to_string_pretty(&file).unwrap());
}
