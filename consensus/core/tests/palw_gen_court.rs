//! **RFC-0003: the generative court's own questions**, over the golden toy image pipeline
//! (`consensus-vectors/tir-v2/pipelines/toy-image.json`, run here from its programs and params):
//!
//! * `R`'s inputs are recomputed from the job, lane by lane, and equal the vector's draws;
//! * every commit point of the denoiser recomputes under the court's source from its committed
//!   leaves, the carried edge (the encoder's rows) and nothing else — no random or job input is ever
//!   asked of the carriage, and the edge's zero pad is not either;
//! * a carried edge value outside the upstream's proven interval is the executor's PALW-TIR-33 fault;
//! * the output digest: every output tile agrees with the output node's step tile; a claim whose
//!   output bytes differ from its step tiles is convicted by fault 21 at the lane; a step lane outside
//!   the proven interval by PALW-TIR-33; evidence that is not under the root convicts nobody.

use kaspa_consensus_core::palw_gen_court_v1::*;
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_gen::output::{output_leaves_v1, output_root_v1, output_tile_proof_v1};
use misaka_palw_tir::demand::{DemandContext, DemandLimits, DemandRequest, DemandTarget};
use misaka_palw_tir::demand_v2::{MapSourceV2, commit_occurrence_v2, eval_demanded_v2};
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::interp_v2::MapInputs;
use misaka_palw_tir::pipeline::{PipelineJob, PipelineParams, PipelineRun, RandomSource, TirPipelineV1, run_pipeline, stage_job_facts};
use misaka_palw_tir::program_v2::{OutputDecl, RandomDist, TirProgramV2};
use misaka_palw_tir::tensor::Tensor;
use misaka_palw_tir::types::DType;
use misaka_palw_tir::validate_v2::validate_v2;

const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };
/// The denoiser's inputs (the toy pipeline's declaration order).
const IN_NOISE: u16 = 0;
const IN_COND: u16 = 1;
const IN_JITTER: u16 = 5;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn vector() -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines/toy-image.json");
    serde_json::from_slice(&std::fs::read(path).expect("the golden vector")).expect("json")
}

fn ints(v: &serde_json::Value) -> Vec<i128> {
    v.as_array().expect("an array").iter().map(|x| x.as_str().map_or_else(|| x.as_i64().unwrap() as i128, |s| s.parse().unwrap())).collect()
}

struct Params(Vec<MapParams>);
impl PipelineParams for Params {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.0[program as usize]
    }
}

/// `R` in bulk (the executor's path); the court recomputes lane by lane.
struct Draws(PalwGenDrawV1);
impl RandomSource for Draws {
    fn random(&self, domain: u16, dist: RandomDist, step: u32, shape: &[u32]) -> Option<Tensor> {
        let n: u64 = shape.iter().map(|d| *d as u64).product();
        let (d, dtype) = match dist {
            RandomDist::Uniform { .. } => (misaka_palw_gen::RandDistV1::Uniform, DType::Idx),
            RandomDist::Normal => (misaka_palw_gen::RandDistV1::Normal, DType::I32),
        };
        let v = misaka_palw_gen::rand_values_v1(domain, d, &self.0.seed, step, self.0.item_index, n).ok()?;
        Tensor::new(dtype, shape.iter().map(|x| *x as usize).collect(), v.into_iter().map(|x| x as i128).collect()).ok()
    }
}

struct Toy {
    v: serde_json::Value,
    pipeline: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    params: Params,
    job: PipelineJob,
    draw: PalwGenDrawV1,
    run: PipelineRun,
}

fn toy() -> Toy {
    let v = vector();
    let programs: Vec<TirProgramV2> = v["programs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| TirProgramV2::decode_canonical(&unhex(p["program_borsh_hex"].as_str().unwrap())).expect("a canonical program"))
        .collect();
    let pipeline = TirPipelineV1::decode_canonical(&unhex(v["pipeline_borsh_hex"].as_str().unwrap()), &programs).expect("a pipeline");
    let params = Params(
        v["programs"]
            .as_array()
            .unwrap()
            .iter()
            .zip(&programs)
            .map(|(pj, prog)| {
                let mut m = MapParams::default();
                for e in pj["params"].as_array().unwrap() {
                    let j = e["param"].as_u64().unwrap() as u16;
                    let layer = e["layer"].as_u64().map(|l| l as u16);
                    let decl = &prog.params[j as usize];
                    let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
                    let t = Tensor::from_le_bytes(decl.dtype, &shape, &unhex(e["le_hex"].as_str().unwrap())).expect("a param");
                    m.tensors.insert((j, layer), t);
                }
                m
            })
            .collect(),
    );
    let job = PipelineJob {
        prompt: v["job"]["prompt"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u32).collect(),
        negative: v["job"]["negative"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u32).collect(),
        steps: v["job"]["steps"].as_u64().unwrap() as u32,
        scalars: ints(&v["job"]["scalars"]).into_iter().map(|x| x as i64).collect(),
    };
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&unhex(v["seed_hex"].as_str().unwrap()));
    let draw = PalwGenDrawV1 { seed, item_index: v["item_index"].as_u64().unwrap() as u32 };
    let run = run_pipeline(&pipeline, &programs, &params, &Draws(draw), &job).expect("the toy job runs");
    assert_eq!(run.output.data, ints(&v["output"]["data"]), "the vector's image, reproduced from its programs and params");
    Toy { v, pipeline, programs, params, job, draw, run }
}

#[test]
fn r_inputs_are_recomputed_from_the_job_lane_by_lane() {
    let t = toy();
    let facts = stage_job_facts(&t.pipeline, &t.programs, &t.job).unwrap();
    let answers = palw_gen_stage_answers_v1(&t.pipeline, &t.programs, 1, &facts).unwrap();
    for r in t.v["random_inputs"].as_array().unwrap() {
        let (input, step) = (r["input"].as_u64().unwrap() as u16, r["step"].as_u64().unwrap() as u32);
        let PalwGenInputAnswerV1::Random { domain, dist, per_step } = answers[input as usize] else {
            panic!("input {input} is drawn, so the court recomputes it")
        };
        assert_eq!(domain as u64, r["domain"].as_u64().unwrap());
        let want = ints(&r["value"]["data"]);
        let got: Vec<i128> =
            (0..want.len() as u64).map(|lane| palw_gen_random_element_v1(domain, dist, per_step, &t.draw, step, lane).unwrap()).collect();
        assert_eq!(got, want, "input {input} at step {step}");
        // Another seed or another item: another draw.
        let other = PalwGenDrawV1 { item_index: t.draw.item_index + 1, ..t.draw };
        assert_ne!(palw_gen_random_element_v1(domain, dist, per_step, &other, step, 0).unwrap(), want[0]);
    }
    // The noise is drawn once (step 0 at every position); the jitter per step.
    let noise = |pos| palw_gen_random_element_v1(1, RandomDist::Normal, false, &t.draw, pos, 0).unwrap();
    assert_eq!(noise(0), noise(2));
    let jitter = |pos| palw_gen_random_element_v1(2, RandomDist::Normal, true, &t.draw, pos, 0).unwrap();
    assert_ne!(jitter(0), jitter(1));
}

/// The denoiser's source: its committed leaves and params, and the carried edge (the encoder's rows
/// `drop..T`, as committed — no pad, which the court answers itself).
fn denoiser_carriage(t: &Toy) -> (MapSourceV2, Vec<PalwGenInputAnswerV1>) {
    let prog = &t.programs[1];
    let info = validate_v2(prog).unwrap();
    let stage = &t.run.stages[1];
    let tokens = vec![0u32; stage.trip as usize];
    let mut src = MapSourceV2::from_run(prog, &info, &t.params.0[1], &MapInputs::default(), &tokens, &stage.steps);
    assert!(src.inputs.is_empty(), "no input is in the carriage but the edge, below");
    let rows: Vec<i128> = t.run.stages[0].rows().iter().skip(1).flat_map(|r| r.data.clone()).collect();
    src.inputs.insert((IN_COND, None), rows);
    let facts = stage_job_facts(&t.pipeline, &t.programs, &t.job).unwrap();
    (src, palw_gen_stage_answers_v1(&t.pipeline, &t.programs, 1, &facts).unwrap())
}

#[test]
fn every_denoiser_commit_point_recomputes_under_the_court() {
    let t = toy();
    let prog = &t.programs[1];
    let info = validate_v2(prog).unwrap();
    let (carriage, answers) = denoiser_carriage(&t);
    let PalwGenInputAnswerV1::Edge { kept, .. } = answers[IN_COND as usize] else { panic!("the rows are an edge") };
    assert_eq!(kept, 4 * 4, "the encoder's 5 rows less the one dropped, 4 lanes each; the fifth pad row is the court's");
    let mut asked = 0;
    for (pos, step) in t.run.stages[1].steps.iter().enumerate() {
        for c in &step.commits {
            let ctx = DemandContext { pos: pos as u32, occurrence: commit_occurrence_v2(prog, c.block, c.layer) };
            let mut inner = carriage.clone();
            inner.base.nodes.remove(&(ctx, c.node));
            let elements: Vec<usize> = (0..c.value.data.len()).collect();
            let mut court = PalwGenStageSourceV1::new(&mut inner, answers.clone(), t.draw);
            let (vals, _) = eval_demanded_v2(
                prog,
                &info,
                &DemandRequest { target: DemandTarget::Node { ctx, node: c.node }, elements: &elements },
                &mut court,
                &LIMITS,
            )
            .unwrap_or_else(|e| panic!("position {pos} node {}: {e:?}", c.node));
            assert_eq!(vals, c.value.data, "position {pos} node {}", c.node);
            assert!(court.violation.is_none());
            // The carriage is asked for the edge's committed lanes only: never R, never the job, never the pad.
            for (_, input, index) in &inner.input_requests {
                assert_eq!(*input, IN_COND, "only the edge is carried");
                assert!((*index as u64) < kept, "the pad is the court's");
            }
            asked += inner.input_requests.len();
        }
    }
    assert!(asked > 0, "the rows reach the denoiser's cones");
}

#[test]
fn a_carried_edge_value_outside_the_upstream_interval_convicts_the_executor() {
    let t = toy();
    let prog = &t.programs[1];
    let info = validate_v2(prog).unwrap();
    let (mut carriage, answers) = denoiser_carriage(&t);
    let PalwGenInputAnswerV1::Edge { hi, .. } = answers[IN_COND as usize] else { panic!("an edge") };
    carriage.inputs.get_mut(&(IN_COND, None)).unwrap()[0] = hi + 1;
    // pre's text vector reads the rows (spec 04b §15.4): its cone at position 0.
    let pre = prog.schedule.pre;
    let node = prog.blocks[pre as usize].carry_out[2];
    let ctx = DemandContext { pos: 0, occurrence: 0 };
    carriage.base.nodes.remove(&(ctx, node));
    let n: usize = prog.blocks[pre as usize].nodes[node as usize]
        .out
        .shape
        .iter()
        .map(|d| if let misaka_palw_tir::types::Dim::Fixed(k) = d { *k as usize } else { 1 })
        .product();
    let elements: Vec<usize> = (0..n).collect();
    let mut court = PalwGenStageSourceV1::new(&mut carriage, answers, t.draw);
    let r =
        eval_demanded_v2(prog, &info, &DemandRequest { target: DemandTarget::Node { ctx, node }, elements: &elements }, &mut court, &LIMITS);
    assert!(r.is_err(), "the evaluation does not run on a value no execution produces");
    assert_eq!(court.violation, Some((IN_COND, 0, 0, hi + 1)), "recorded: the executor's PALW-TIR-33 fault, at the lane");
    // A random input is never the carriage's: withholding the noise changes nothing the court reads.
    let (mut clean, answers) = denoiser_carriage(&t);
    clean.withheld_inputs.insert(IN_NOISE, misaka_palw_tir::TirErrorKind::Operand);
    clean.withheld_inputs.insert(IN_JITTER, misaka_palw_tir::TirErrorKind::Operand);
    let post_occ = (prog.schedule.layers.len() + 1) as u16;
    let write = prog.output.node();
    let ctx = DemandContext { pos: 0, occurrence: post_occ };
    clean.base.nodes.remove(&(ctx, write));
    let mut court = PalwGenStageSourceV1::new(&mut clean, answers, t.draw);
    let (vals, _) =
        eval_demanded_v2(prog, &info, &DemandRequest { target: DemandTarget::Node { ctx, node: write }, elements: &[0] }, &mut court, &LIMITS)
            .expect("the court draws the noise itself");
    assert_eq!(vals[0], t.run.stages[1].steps[0].output.data[0]);
}

#[test]
fn the_output_digest_is_the_output_nodes_step_tiles() {
    let t = toy();
    let img = &t.v["output_image"];
    let spec: OutputSpecV1 = borsh::from_slice(&unhex(img["spec_borsh_hex"].as_str().unwrap())).unwrap();
    let tile_len = img["tile_len"].as_u64().unwrap() as u32;
    let values: Vec<i64> = t.run.output.data.iter().map(|v| *v as i64).collect();
    let root = output_root_v1(&spec, &values, tile_len).unwrap();
    assert_eq!(root.to_vec(), unhex(img["output_root_hex"].as_str().unwrap()), "the vector's root is the run's");
    let leaves = output_leaves_v1(&spec, &values, tile_len).unwrap();
    let tiles = misaka_palw_gen::output::output_tiles_v1(&spec, &values, tile_len).unwrap();
    let decoder = &t.programs[2];
    let interval = misaka_palw_tir::interval_v2::output_interval_v2(decoder).unwrap();
    let interval = (interval.lo, interval.hi);
    let lanes = |tile: usize| -> Vec<i128> { t.run.output.data[tile * tile_len as usize..][..tile_len as usize].to_vec() };
    // The decoder is a `Final` stage at one position: output tile t is its step tile t.
    let row = t.run.output.data.len() as u64;
    for tile in 0..tiles.len() {
        assert_eq!(palw_gen_output_tile_of_v1(&decoder.output, row, tile_len, 1, 0, tile as u64), Some(tile as u64));
        let proof = output_tile_proof_v1(&leaves, tile).unwrap();
        let verdict = palw_gen_output_tile_check_v1(&spec, &root, tile_len, tile as u64, interval, &lanes(tile), &tiles[tile], &proof);
        assert_eq!(verdict, Ok(None), "tile {tile}: the executor's two statements agree");
    }
    assert_eq!(palw_gen_output_tile_of_v1(&decoder.output, row, tile_len, 1, 0, tiles.len() as u64), None);

    // An executor whose output bytes are not its step tiles: another pixel in tile 1, lane 2.
    let mut lied = values.clone();
    lied[tile_len as usize + 2] = (lied[tile_len as usize + 2] + 1) % 256;
    let lied_root = output_root_v1(&spec, &lied, tile_len).unwrap();
    let lied_leaves = output_leaves_v1(&spec, &lied, tile_len).unwrap();
    let lied_tiles = misaka_palw_gen::output::output_tiles_v1(&spec, &lied, tile_len).unwrap();
    for tile in 0..tiles.len() {
        let proof = output_tile_proof_v1(&lied_leaves, tile).unwrap();
        let verdict =
            palw_gen_output_tile_check_v1(&spec, &lied_root, tile_len, tile as u64, interval, &lanes(tile), &lied_tiles[tile], &proof);
        let want = if tile == 1 { Some(PalwStepFaultV1::TirOutputDigestMismatch { value_index: 2 }) } else { None };
        assert_eq!(verdict, Ok(want), "tile {tile}");
    }
    // A step lane outside the proven interval is PALW-TIR-33's fault, at the lane, before any byte.
    let proof = output_tile_proof_v1(&leaves, 0).unwrap();
    let mut outside = lanes(0);
    outside[1] = interval.1 + 1;
    assert_eq!(
        palw_gen_output_tile_check_v1(&spec, &root, tile_len, 0, interval, &outside, &tiles[0], &proof),
        Ok(Some(PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 1 }))
    );
    // Evidence that fails convicts nobody.
    let wrong_proof = output_tile_proof_v1(&leaves, 1).unwrap();
    assert_eq!(
        palw_gen_output_tile_check_v1(&spec, &root, tile_len, 0, interval, &lanes(0), &tiles[0], &wrong_proof),
        Err(PalwGenOutputRefusalV1::TileNotProven)
    );
    let mut forged = tiles[0].clone();
    forged[0] ^= 1;
    assert_eq!(
        palw_gen_output_tile_check_v1(&spec, &root, tile_len, 0, interval, &lanes(0), &forged, &proof),
        Err(PalwGenOutputRefusalV1::TileNotProven)
    );
    assert_eq!(
        palw_gen_output_tile_check_v1(&spec, &root, tile_len, 0, interval, &lanes(0)[..3], &tiles[0], &proof),
        Err(PalwGenOutputRefusalV1::LaneCount { want: tile_len as u64, got: 3 })
    );
}

#[test]
fn rows_outputs_align_row_by_row() {
    let rows = OutputDecl::Rows { node: 0 };
    assert_eq!(palw_gen_output_tile_of_v1(&rows, 8, 4, 3, 2, 1), Some(5), "row 2, tile 1 of 2");
    assert_eq!(palw_gen_output_tile_of_v1(&rows, 8, 4, 3, 3, 0), None, "past the trip");
    assert_eq!(palw_gen_output_tile_of_v1(&rows, 8, 4, 3, 0, 2), None, "past the row");
    assert_eq!(palw_gen_output_tile_of_v1(&rows, 6, 4, 3, 0, 0), None, "a row that is not whole tiles has no alignment");
    let fin = OutputDecl::Final { node: 0 };
    assert_eq!(palw_gen_output_tile_of_v1(&fin, 8, 4, 3, 1, 0), None, "a Final stage's earlier positions hold no output");
    assert_eq!(palw_gen_output_tile_of_v1(&fin, 8, 4, 3, 2, 1), Some(1));
}

#[test]
fn fault_21_is_appended() {
    let tag = |f: PalwStepFaultV1| borsh::to_vec(&f).unwrap()[0];
    assert_eq!(tag(PalwStepFaultV1::TirOutputDigestMismatch { value_index: 7 }), 21);
    assert_eq!(tag(PalwStepFaultV1::TirLogitsTraceMismatch { value_index: 7 }), 20, "the previous last fault keeps its tag");
    assert_eq!(tag(PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 7 }), 19);
}
