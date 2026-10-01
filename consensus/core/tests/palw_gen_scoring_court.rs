//! **RFC-0004 §7 (work item A7): the generative court covers scoring stages.** The four evaluation
//! pipelines of the golden vectors (`consensus-vectors/tir-v2/pipelines/eval-*.json`) as claims:
//!
//! * **the claim**: the worker's committed run of each vector's job reproduces the vector's score;
//!   in Generate mode the V4 decoder's run of the decode stage selects the vector's ids;
//! * **every leaf of every stage** — the decode stage, the judge stages, the scoring stage — is
//!   acquitted from the leaves before it, the params proven under the pipeline's artifact root and the
//!   job's facts: the generated ids and the key (ExactMatch), the decode stage's consumed logits rows
//!   read across an edge whose rows start at `|prompt| − 1` (RefLogLik), the finalized outputs (Judge,
//!   Pairwise);
//! * **a lie** at a score leaf is convicted there, and a lie in a consumed logits row convicts at the
//!   RefLogLik leaf that reads it across the edge (RefLogLik's first stage, whose leaf at position
//!   `p` reads row `p` alone);
//! * **the decode door** holds every id a decode stage that is NOT the output stage selected, and
//!   convicts an id the decode would not select.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1;
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2;
use kaspa_consensus_core::palw_gen_artifact_v1::*;
use kaspa_consensus_core::palw_gen_court_v1::*;
use kaspa_consensus_core::palw_gen_step_v1::*;
use kaspa_consensus_core::palw_gen_worker_v1::*;
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{PipelineJob, PipelineParams, TirPipelineV1, stage_job_facts, stream_stage};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::scoring::{exact_match_reference_v1, ref_loglik_join_v1, ref_loglik_reference_v1};
use misaka_palw_tir::tensor::Tensor;

const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

struct Params(Vec<MapParams>);
impl PipelineParams for Params {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.0[program as usize]
    }
}

/// An evaluation pipeline of the vectors: its programs, params, layouts (4-lane tiles, a checkpoint
/// every position), its job, and its artifact root with every param leaf opened.
struct Eval {
    pipeline: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    layouts: Vec<PalwTirLayoutV1>,
    params: Params,
    job: PipelineJob,
    score: Vec<i128>,
    root: Hash64,
    inventory: PalwGenInventoryIndexV1,
    openings: Vec<PalwArtifactOpeningV1>,
}

fn ids(v: &serde_json::Value) -> Vec<u32> {
    v.as_array().map(|a| a.iter().map(|t| t.as_u64().unwrap() as u32).collect()).unwrap_or_default()
}

fn eval(name: &str) -> Eval {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines").join(name);
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the vector")).expect("json");
    let programs: Vec<TirProgramV2> = v["programs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| TirProgramV2::decode_canonical(&unhex(p["program_borsh_hex"].as_str().unwrap())).unwrap())
        .collect();
    let pipeline = TirPipelineV1::decode_canonical(&unhex(v["pipeline_borsh_hex"].as_str().unwrap()), &programs).unwrap();
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
                    let decl = &prog.params[j as usize];
                    let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
                    let layer = e["layer"].as_u64().map(|l| l as u16);
                    m.tensors
                        .insert((j, layer), Tensor::from_le_bytes(decl.dtype, &shape, &unhex(e["le_hex"].as_str().unwrap())).unwrap());
                }
                m
            })
            .collect(),
    );
    let layouts = pipeline
        .stages
        .iter()
        .map(|st| {
            let p = &programs[st.program as usize];
            let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
            PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: st.max_trip,
                checkpoint_interval: 1,
                h_tile: 16,
                commit_tiles: vec![4; commits],
                state_tiles: vec![4; p.states.len()],
            }
        })
        .collect();
    let j = &v["job"];
    let job = PipelineJob {
        prompt: ids(&j["prompt"]),
        scalars: j["scalars"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().parse().unwrap()).collect(),
        generated: ids(&j["generated"]),
        key: ids(&j["key"]),
        finalized: j["finalized"]
            .as_array()
            .map(|a| {
                a.iter().map(|f| ((f["claim"].as_u64().unwrap() as u8, f["stage"].as_u64().unwrap() as u8), ids(&f["ids"]))).collect()
            })
            .unwrap_or_default(),
        ..PipelineJob::default()
    };
    let score = v["output"]["data"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().parse().unwrap()).collect();
    let (root, count) = palw_gen_inventory_root_v1(&programs, &params).expect("the evaluation's weights have an inventory");
    let inventory = PalwGenInventoryIndexV1::new(&programs).unwrap();
    let openings = palw_gen_open_leaves_v1(&programs, &params, 0..count).unwrap();
    Eval { pipeline, programs, layouts, params, job, score, root, inventory, openings }
}

/// The claim: the committed run of the job (a teacher-forced or a judge job's ids are given).
fn claim(c: &Eval) -> PalwGenExecutionV1 {
    palw_gen_replay_committed_v1(&c.pipeline, &c.programs, &c.layouts, &c.params, &c.job, [0; 32]).unwrap()
}

/// An execution whose one leaf is changed, its roots recomputed: the executor's lie.
fn lie(e: &PalwGenExecutionV1, stage: usize, index: usize, delta: i128) -> PalwGenExecutionV1 {
    let mut l = e.clone();
    l.leaf_values[stage][index][0] += delta;
    let leaf = l.space.stages[stage].leaves()[index];
    l.leaf_hashes[stage][index] = palw_gen_step_leaf_hash_v1(&leaf, &l.leaf_values[stage][index]).unwrap();
    l.claim.stage_roots[stage] = palw_gen_stage_root_v1(stage as u8, &l.leaf_hashes[stage]);
    l.claim.step_root = palw_gen_step_root_v1(&l.claim.stage_roots);
    l
}

/// The close of leaf `(stage, index)`: every leaf before it and every param leaf.
fn close(c: &Eval, e: &PalwGenExecutionV1, stage: usize, index: usize) -> PalwGenCloseV1 {
    let mut operands = Vec::new();
    for s in 0..=stage {
        let n = if s == stage { index } else { e.space.stages[s].leaves().len() };
        for i in 0..n {
            operands.push(e.open(s as u8, i as u64).unwrap());
        }
    }
    PalwGenCloseV1 { disputed: e.open(stage as u8, index as u64).unwrap(), operands, image_tiles: vec![], params: c.openings.clone() }
}

fn case<'a>(c: &'a Eval, e: &'a PalwGenExecutionV1, facts: &'a [misaka_palw_tir::pipeline::StageJobFacts]) -> PalwGenCourtCaseV1<'a> {
    PalwGenCourtCaseV1 {
        space: &e.space,
        pipeline: &c.pipeline,
        programs: &c.programs,
        artifact_root: c.root,
        inventory: &c.inventory,
        facts,
        images: &[],
        draw: PalwGenDrawV1 { seed: [0; 32], item_index: 0 },
        claim: &e.claim,
    }
}

fn judge(c: &Eval, e: &PalwGenExecutionV1, stage: usize, index: usize) -> Result<PalwGenVerdictV1, PalwGenCloseRefusalV1> {
    let job = PipelineJob { generated: e.claim.generated.clone(), ..c.job.clone() };
    let facts = stage_job_facts(&c.pipeline, &c.programs, &job).unwrap();
    palw_gen_adjudicate_leaf_v1(&case(c, e, &facts), &close(c, e, stage, index), &LIMITS)
}

const VECTORS: [&str; 4] = ["eval-exact-match.json", "eval-ref-loglik.json", "eval-judge.json", "eval-pairwise.json"];

#[test]
fn each_claim_commits_the_vectors_score() {
    for name in VECTORS {
        let c = eval(name);
        let e = claim(&c);
        assert_eq!(e.run.output.data, c.score, "{name}");
        assert_eq!(e.space.stages.len(), c.pipeline.stages.len());
    }
    // Generate mode: the V4 decoder over the decode stage selects the vector's ids (greedy).
    let c = eval("eval-exact-match.json");
    let decode = PalwGenDecodeV1 { config: DecodeConfigV4::NOOP, sampling: PalwDecodeSamplingV2::GREEDY, limit: 4 };
    let generating = PipelineJob { generated: vec![], ..c.job.clone() };
    let e = palw_gen_execute_v1(&c.pipeline, &c.programs, &c.layouts, &c.params, &generating, &decode, [0; 32]).unwrap();
    assert_eq!(e.claim.generated, c.job.generated, "the decode stage's ids");
    assert_eq!(e.run.output.data, c.score);
    assert_eq!(e.claim, claim(&c).claim, "the generating run and the committed replay commit one claim");
    let want = exact_match_reference_v1(&c.job.generated, &c.job.key, c.job.scalars[0], c.job.scalars[1]);
    assert_eq!(c.score, vec![want as i128]);
    // Teacher-forced: the score is the reference's log-likelihood under the decode stage's rows.
    let c = eval("eval-ref-loglik.json");
    let e = claim(&c);
    let from = c.job.prompt.len() - 1;
    let rows: Vec<Vec<i32>> =
        e.run.stages[0].steps[from..].iter().map(|s| s.output.data.iter().map(|x| *x as i32).collect()).collect();
    let want = ref_loglik_reference_v1(&rows, &c.job.generated, c.job.scalars[0]);
    assert_eq!(ref_loglik_join_v1(c.score[0] as i32, c.score[1] as i32), want);
}

#[test]
fn every_leaf_of_every_evaluation_stage_is_acquitted() {
    for name in VECTORS {
        let c = eval(name);
        let e = claim(&c);
        let mut judged = 0;
        for s in 0..c.pipeline.stages.len() {
            for i in 0..e.space.stages[s].leaves().len() {
                let verdict = judge(&c, &e, s, i);
                assert_eq!(
                    verdict,
                    Ok(PalwGenVerdictV1::Acquitted),
                    "{name}: stage {s} leaf {i} {:?}",
                    e.space.stages[s].leaves()[i].coord
                );
                judged += 1;
            }
        }
        let score_leaves = e.space.stages[c.pipeline.output_stage as usize].leaves().len();
        assert!(score_leaves > 0, "{name}: the score is committed");
        eprintln!("{name}: {judged} leaves acquitted, {score_leaves} of them the scoring stage's");
    }
}

#[test]
fn a_lie_in_a_score_or_in_a_row_it_reads_is_convicted() {
    for name in VECTORS {
        let c = eval(name);
        let e = claim(&c);
        let out = c.pipeline.output_stage as usize;
        // Every commit leaf of the scoring stage, lied about, is convicted where it stands.
        for i in 0..e.space.stages[out].leaves().len() {
            let coord = e.space.stages[out].leaves()[i].coord;
            if !matches!(coord.kind, PalwGenLeafKindV1::Commit { .. }) {
                continue;
            }
            match judge(&c, &lie(&e, out, i, 1), out, i) {
                Ok(PalwGenVerdictV1::Convicted { leaf, fault }) => {
                    assert_eq!(leaf, coord, "{name}");
                    assert!(
                        matches!(
                            fault,
                            PalwStepFaultV1::ComputationMismatch { .. } | PalwStepFaultV1::TirValueOutsideProvenInterval { .. }
                        ),
                        "{name}: {fault:?}"
                    );
                }
                other => panic!("{name}: score leaf {i}: {other:?}"),
            }
        }
    }
    // RefLogLik reads the decode stage's consumed rows across an edge: a lie in the first consumed
    // logits row (position |prompt| − 1) convicts at the scoring leaf that reads it.
    let c = eval("eval-ref-loglik.json");
    let e = claim(&c);
    let post = (c.programs[0].occurrences().len() - 1) as u16;
    let pos = c.job.prompt.len() as u32 - 1;
    let row = e.space.stages[0]
        .leaves()
        .iter()
        .position(|l| l.coord.pos == pos && matches!(l.coord.kind, PalwGenLeafKindV1::Commit { occurrence, .. } if occurrence == post))
        .expect("the first consumed row is a leaf");
    assert!(
        e.space.stages[0].leaves().iter().all(|l| l.coord.pos >= pos
            || !matches!(l.coord.kind, PalwGenLeafKindV1::Commit { occurrence, .. } if occurrence == post)),
        "no logits leaf before |prompt| − 1: only consumed rows are committed"
    );
    let lied = lie(&e, 0, row, 1 << 20);
    // RefLogLik's first stage reads row p at position p: its leaf at position 0 reads the lied row.
    let reader = e.space.stages[1]
        .leaves()
        .iter()
        .position(|l| l.coord.pos == 0 && matches!(l.coord.kind, PalwGenLeafKindV1::Commit { .. }))
        .unwrap();
    assert!(matches!(judge(&c, &lied, 1, reader), Ok(PalwGenVerdictV1::Convicted { .. })), "the row's lie reaches the score");
}

#[test]
fn the_decode_door_holds_a_decode_stage_that_is_not_the_output() {
    let c = eval("eval-exact-match.json");
    let e = claim(&c);
    let s = stream_stage(&c.pipeline).unwrap();
    assert_ne!(s, c.pipeline.output_stage as usize, "the decode stage is not the output");
    let decode = PalwGenDecodeV1 { config: DecodeConfigV4::NOOP, sampling: PalwDecodeSamplingV2::GREEDY, limit: 4 };
    let prompt_len = c.job.prompt.len() as u32;
    let door = |e: &PalwGenExecutionV1, t: u32| {
        let job = PipelineJob { generated: e.claim.generated.clone(), ..c.job.clone() };
        let facts = stage_job_facts(&c.pipeline, &c.programs, &job).unwrap();
        let post = (c.programs[s].occurrences().len() - 1) as u16;
        let pos = prompt_len - 1 + t;
        let row: Vec<PalwGenOpenedLeafV1> = e.space.stages[s]
            .leaves()
            .iter()
            .filter(|l| {
                l.coord.pos == pos && matches!(l.coord.kind, PalwGenLeafKindV1::Commit { occurrence, .. } if occurrence == post)
            })
            .map(|l| e.open_at(&l.coord).unwrap())
            .collect();
        palw_gen_decode_door_v1(&case(&c, e, &facts), &decode, prompt_len, t, &row)
    };
    for t in 0..e.claim.generated.len() as u32 {
        assert_eq!(door(&e, t), Ok(PalwGenVerdictV1::Acquitted), "id {t}");
    }
    let mut lied = e.clone();
    lied.claim.generated[1] = (lied.claim.generated[1] + 1) % 16;
    assert!(matches!(
        door(&lied, 1),
        Ok(PalwGenVerdictV1::Convicted { fault: PalwStepFaultV1::DecodeTokenMismatch { position: 1 }, .. })
    ));
}

/// **RefLogLik's first stage reads one logits row per leaf** — the decode door's own bound, so a
/// RefLogLik leaf's close carries at most one row whatever the reference's length: the units the
/// court's evaluation of stage 1's leaves at position `p` read are row `p`'s tiles of the decode stage
/// and stage 1's own leaves, and nothing else of the decode stage.
#[test]
fn a_ref_loglik_leaf_reads_one_logits_row() {
    let c = eval("eval-ref-loglik.json");
    let e = claim(&c);
    let job = PipelineJob { generated: e.claim.generated.clone(), ..c.job.clone() };
    let facts = stage_job_facts(&c.pipeline, &c.programs, &job).unwrap();
    let from = c.job.prompt.len() as u32 - 1;
    let post = (c.programs[0].occurrences().len() - 1) as u16;
    for (i, leaf) in e.space.stages[1].leaves().iter().enumerate() {
        let used = palw_gen_cone_units_v1(&case(&c, &e, &facts), &close(&c, &e, 1, i), &LIMITS).unwrap();
        for (s, index) in &used.leaves {
            if *s != 0 {
                continue;
            }
            let read = e.space.stages[0].leaves()[*index as usize].coord;
            assert!(
                read.pos == from + leaf.coord.pos
                    && matches!(read.kind, PalwGenLeafKindV1::Commit { occurrence, .. } if occurrence == post),
                "stage 1 leaf {:?} read {read:?}: only the consumed row of its position",
                leaf.coord
            );
        }
    }
}
