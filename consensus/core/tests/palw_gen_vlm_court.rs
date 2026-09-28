//! **A vision-language claim end to end at the library level** (RFC-0003 §II.2.1; FP Job V5):
//!
//! * **the worker** runs the toy VLM class (`consensus-vectors/tir-v2/pipelines/toy-vlm.json`: a
//!   vision stage over a job image, then the text stage over a prompt with two placeholder ids)
//!   through FP Job V4's decoder into the one step tree and its roots;
//! * **the panel**: a seat's replay finds the claim `Valid`, and names a changed answer or stage;
//! * **the court composition**: every leaf of both stages is acquitted from the leaves before it,
//!   the class's params, the job's facts and the image's proven tiles; a lie in a vision leaf, in a
//!   text leaf that reads the image rows, or in a logits leaf is convicted at the leaf
//!   (`ComputationMismatch`); a generated id the decode would not select is convicted by the door
//!   (`DecodeTokenMismatch`); across the toy image pipeline's stages a carried edge value outside
//!   its upstream's interval is convicted by PALW-TIR-33; and evidence that fails convicts nobody.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2;
use kaspa_consensus_core::palw_gen_court_v1::*;
use kaspa_consensus_core::palw_gen_step_v1::*;
use kaspa_consensus_core::palw_gen_worker_v1::*;
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{JobImageV1, PipelineJob, PipelineParams, TirPipelineV1, stage_job_facts};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::tensor::Tensor;

const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };
const PLACEHOLDER: u32 = 15;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

struct Params(Vec<MapParams>);
impl PipelineParams for Params {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.0[program as usize]
    }
}

struct Class {
    pipeline: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    layouts: Vec<PalwTirLayoutV1>,
    params: Params,
    v: serde_json::Value,
}

fn class(vector: &str) -> Class {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines").join(vector);
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
    Class { pipeline, programs, layouts, params, v }
}

fn vlm_job(c: &Class) -> (PipelineJob, PalwGenImageRefV1, Vec<(Vec<u8>, Vec<[u8; 64]>)>) {
    let img = &c.v["job"]["images"][0];
    let (h, w) = (img["h"].as_u64().unwrap() as u32, img["w"].as_u64().unwrap() as u32);
    let rgb = unhex(img["rgb_hex"].as_str().unwrap());
    let root = misaka_palw_gen::output::input_image_root_v1(h, w, 4, &rgb).unwrap();
    let tiles = misaka_palw_gen::output::input_image_tiles_v1(h, w, 4, &rgb).unwrap();
    let job =
        PipelineJob { prompt: vec![3, PLACEHOLDER, PLACEHOLDER, 5], images: vec![JobImageV1 { h, w, rgb }], ..PipelineJob::default() };
    (job, PalwGenImageRefV1 { input_root: root, h, w, tile_len: 4 }, tiles)
}

fn decode() -> PalwGenDecodeV1 {
    PalwGenDecodeV1 { config: DecodeConfigV4::NOOP, sampling: PalwDecodeSamplingV2::GREEDY, limit: 4 }
}

/// An execution whose one leaf is changed and whose roots are recomputed over it: the executor's
/// lie, committed consistently.
fn lie(e: &PalwGenExecutionV1, stage: usize, index: usize, lane: usize, delta: i128) -> PalwGenExecutionV1 {
    let mut l = e.clone();
    l.leaf_values[stage][index][lane] += delta;
    let leaf = l.space.stages[stage].leaves()[index];
    l.leaf_hashes[stage][index] = palw_gen_step_leaf_hash_v1(&leaf, &l.leaf_values[stage][index]).unwrap();
    l.claim.stage_roots[stage] = palw_gen_stage_root_v1(stage as u8, &l.leaf_hashes[stage]);
    l.claim.step_root = palw_gen_step_root_v1(&l.claim.stage_roots);
    l
}

/// The close of leaf `(stage, index)` carrying every leaf before it and every image tile.
fn close(e: &PalwGenExecutionV1, stage: usize, index: usize, tiles: &[(Vec<u8>, Vec<[u8; 64]>)]) -> PalwGenCloseV1 {
    let mut operands = Vec::new();
    for s in 0..=stage {
        let n = if s == stage { index } else { e.space.stages[s].leaves().len() };
        for i in 0..n {
            operands.push(e.open(s as u8, i as u64).unwrap());
        }
    }
    let image_tiles = tiles
        .iter()
        .enumerate()
        .map(|(t, (bytes, proof))| PalwGenImageTileV1 { image: 0, tile: t as u64, bytes: bytes.clone(), proof: proof.clone() })
        .collect();
    PalwGenCloseV1 { disputed: e.open(stage as u8, index as u64).unwrap(), operands, image_tiles }
}

fn judge(
    c: &Class,
    e: &PalwGenExecutionV1,
    job: &PipelineJob,
    image: PalwGenImageRefV1,
    close: &PalwGenCloseV1,
) -> Result<PalwGenVerdictV1, PalwGenCloseRefusalV1> {
    let court_job = PipelineJob { images: vec![], generated: e.claim.generated.clone(), ..job.clone() };
    let facts = stage_job_facts(&c.pipeline, &c.programs, &court_job).unwrap();
    let images = [image];
    let case = PalwGenCourtCaseV1 {
        space: &e.space,
        pipeline: &c.pipeline,
        programs: &c.programs,
        params: &c.params,
        facts: &facts,
        images: &images,
        draw: PalwGenDrawV1 { seed: [0; 32], item_index: 0 },
        claim: &e.claim,
    };
    palw_gen_adjudicate_leaf_v1(&case, close, &LIMITS)
}

#[test]
fn the_worker_commits_and_the_panel_replays() {
    let c = class("toy-vlm.json");
    let (job, _, _) = vlm_job(&c);
    let e = palw_gen_execute_v1(&c.pipeline, &c.programs, &c.layouts, &c.params, &job, &decode(), [0; 32]).unwrap();
    assert_eq!(e.claim.generated.len(), 4, "the budget: 4 ids");
    assert_eq!(e.space.stages[1].trip, 4 + 4 - 1);
    assert_eq!(e.claim.stage_roots.len(), 2);
    // Only the consumed positions' logits are leaves of the text stage.
    let post = (c.programs[1].occurrences().len() - 1) as u16;
    let logits_positions: Vec<u32> = e.space.stages[1]
        .leaves()
        .iter()
        .filter(|l| matches!(l.coord.kind, PalwGenLeafKindV1::Commit { occurrence, .. } if occurrence == post))
        .map(|l| l.coord.pos)
        .collect();
    assert!(logits_positions.iter().all(|p| *p >= 3) && logits_positions.contains(&3), "{logits_positions:?}");
    // The panel: a replay is Valid; the committed replay commits the same roots.
    let seat = |claim: &PalwGenClaimRootsV1| {
        palw_gen_replay_v1(&c.pipeline, &c.programs, &c.layouts, &c.params, &job, &decode(), [0; 32], claim).unwrap()
    };
    assert_eq!(seat(&e.claim), PalwGenSeatVerdictV1::Valid);
    let committed = PipelineJob { generated: e.claim.generated.clone(), ..job.clone() };
    let replay = palw_gen_replay_committed_v1(&c.pipeline, &c.programs, &c.layouts, &c.params, &committed, [0; 32]).unwrap();
    assert_eq!(replay.claim, e.claim);
    let mut other = e.claim.clone();
    other.generated[2] ^= 1;
    assert_eq!(seat(&other), PalwGenSeatVerdictV1::AnswerDiffers { first: 2 });
    let lied = lie(&e, 0, 1, 0, 1);
    assert_eq!(seat(&lied.claim), PalwGenSeatVerdictV1::StageDiffers { stage: 0 });
    let mut unbound = e.claim.clone();
    unbound.stage_roots[1] = Hash64::from_bytes([9; 64]);
    assert_eq!(seat(&unbound), PalwGenSeatVerdictV1::RootsNotBound);
    // Another image: another answer's leaves.
    let mut dark = job.clone();
    dark.images[0].rgb.iter_mut().for_each(|b| *b /= 3);
    let e2 = palw_gen_execute_v1(&c.pipeline, &c.programs, &c.layouts, &c.params, &dark, &decode(), [0; 32]).unwrap();
    assert_ne!(e2.claim.stage_roots[0], e.claim.stage_roots[0]);
}

#[test]
fn every_leaf_of_both_stages_is_acquitted_from_the_leaves_before_it() {
    let c = class("toy-vlm.json");
    let (job, image, tiles) = vlm_job(&c);
    let e = palw_gen_execute_v1(&c.pipeline, &c.programs, &c.layouts, &c.params, &job, &decode(), [0; 32]).unwrap();
    let mut judged = 0;
    for s in 0..2 {
        for i in 0..e.space.stages[s].leaves().len() {
            let verdict = judge(&c, &e, &job, image, &close(&e, s, i, &tiles));
            assert_eq!(verdict, Ok(PalwGenVerdictV1::Acquitted), "stage {s} leaf {i} {:?}", e.space.stages[s].leaves()[i].coord);
            judged += 1;
        }
    }
    assert!(judged > 20, "{judged}");
}

#[test]
fn a_lie_in_any_stage_is_convicted_at_its_leaf() {
    let c = class("toy-vlm.json");
    let (job, image, tiles) = vlm_job(&c);
    let e = palw_gen_execute_v1(&c.pipeline, &c.programs, &c.layouts, &c.params, &job, &decode(), [0; 32]).unwrap();
    let find = |stage: usize, pred: &dyn Fn(&PalwGenLeafCoordV1) -> bool| {
        e.space.stages[stage].leaves().iter().position(|l| pred(&l.coord)).expect("such a leaf")
    };
    let post1 = (c.programs[1].occurrences().len() - 1) as u16;
    let cases: Vec<(&str, usize, usize)> = vec![
        (
            "the vision stage's output (the image rows)",
            0,
            find(0, &|k| k.tile == 1 && matches!(k.kind, PalwGenLeafKindV1::Commit { occurrence, .. } if occurrence > 0)),
        ),
        (
            "the text stage at a placeholder position: its input is an image row",
            1,
            find(1, &|k| k.pos == 1 && matches!(k.kind, PalwGenLeafKindV1::Commit { occurrence: 0, .. })),
        ),
        (
            "the placement cursor after the first placeholder",
            1,
            find(1, &|k| k.pos == 1 && matches!(k.kind, PalwGenLeafKindV1::State { .. })),
        ),
        (
            "a logits tile the decode consumes",
            1,
            find(1, &|k| k.pos == 4 && matches!(k.kind, PalwGenLeafKindV1::Commit { occurrence, .. } if occurrence == post1)),
        ),
    ];
    for (what, stage, index) in cases {
        let lied = lie(&e, stage, index, 0, 1);
        let coord = e.space.stages[stage].leaves()[index].coord;
        match judge(&c, &lied, &job, image, &close(&lied, stage, index, &tiles)) {
            Ok(PalwGenVerdictV1::Convicted { leaf, fault }) => {
                assert_eq!(leaf, coord, "{what}");
                assert!(
                    matches!(
                        fault,
                        PalwStepFaultV1::ComputationMismatch { value_index: 0 }
                            | PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 0 }
                    ),
                    "{what}: {fault:?}"
                );
            }
            other => panic!("{what}: {other:?}"),
        }
    }
    // A lie upstream convicts the executor at the leaf that reads it, too: the text stage's leaf
    // recomputed from the lying image row is not the leaf it committed.
    let row_leaf = find(0, &|k| k.tile == 0 && matches!(k.kind, PalwGenLeafKindV1::Commit { occurrence, .. } if occurrence > 0));
    let lied = lie(&e, 0, row_leaf, 0, 1 << 20);
    let reader = find(1, &|k| k.pos == 1 && matches!(k.kind, PalwGenLeafKindV1::Commit { occurrence: 0, .. }));
    assert!(matches!(judge(&c, &lied, &job, image, &close(&lied, 1, reader, &tiles)), Ok(PalwGenVerdictV1::Convicted { .. })));
}

#[test]
fn the_decode_door_convicts_an_id_the_decode_would_not_select() {
    let c = class("toy-vlm.json");
    let (job, image, _) = vlm_job(&c);
    let e = palw_gen_execute_v1(&c.pipeline, &c.programs, &c.layouts, &c.params, &job, &decode(), [0; 32]).unwrap();
    let door = |e: &PalwGenExecutionV1, t: u32| {
        let court_job = PipelineJob { images: vec![], generated: e.claim.generated.clone(), ..job.clone() };
        let facts = stage_job_facts(&c.pipeline, &c.programs, &court_job).unwrap();
        let images = [image];
        let case = PalwGenCourtCaseV1 {
            space: &e.space,
            pipeline: &c.pipeline,
            programs: &c.programs,
            params: &c.params,
            facts: &facts,
            images: &images,
            draw: PalwGenDrawV1 { seed: [0; 32], item_index: 0 },
            claim: &e.claim,
        };
        let post = (c.programs[1].occurrences().len() - 1) as u16;
        let pos = 3 + t;
        let row: Vec<PalwGenOpenedLeafV1> = e.space.stages[1]
            .leaves()
            .iter()
            .filter(|l| {
                l.coord.pos == pos && matches!(l.coord.kind, PalwGenLeafKindV1::Commit { occurrence, .. } if occurrence == post)
            })
            .map(|l| e.open_at(&l.coord).unwrap())
            .collect();
        palw_gen_decode_door_v1(&case, &decode(), 4, t, &row)
    };
    for t in 0..4 {
        assert_eq!(door(&e, t), Ok(PalwGenVerdictV1::Acquitted), "id {t}");
    }
    let mut lied = e.clone();
    lied.claim.generated[1] = (lied.claim.generated[1] + 1) % 16;
    assert!(matches!(
        door(&lied, 1),
        Ok(PalwGenVerdictV1::Convicted { fault: PalwStepFaultV1::DecodeTokenMismatch { position: 1 }, .. })
    ));
}

#[test]
fn an_edge_value_outside_its_upstream_interval_is_convicted_across_stages() {
    // The toy image pipeline: the decoder reads the denoiser's latent (clamped to ±2^20) by an edge.
    let c = class("toy-image.json");
    let job = PipelineJob { prompt: vec![5, 6, 7], steps: 3, scalars: vec![24, 1], ..PipelineJob::default() };
    let e = palw_gen_replay_committed_v1(&c.pipeline, &c.programs, &c.layouts, &c.params, &job, [0x2a; 32]).unwrap();
    let image = PalwGenImageRefV1 { input_root: [0; 64], h: 1, w: 1, tile_len: 4 };
    // Honest: the decoder's output leaf is acquitted across the stages (the court draws the noise itself).
    let dec_leaf = e.space.stages[2].leaves().len() - 1;
    let judge_toy = |e: &PalwGenExecutionV1, close: &PalwGenCloseV1| {
        let facts = stage_job_facts(&c.pipeline, &c.programs, &job).unwrap();
        let images = [image];
        let case = PalwGenCourtCaseV1 {
            space: &e.space,
            pipeline: &c.pipeline,
            programs: &c.programs,
            params: &c.params,
            facts: &facts,
            images: &images,
            draw: PalwGenDrawV1 { seed: [0x2a; 32], item_index: 0 },
            claim: &e.claim,
        };
        palw_gen_adjudicate_leaf_v1(&case, close, &LIMITS)
    };
    assert_eq!(judge_toy(&e, &close(&e, 2, dec_leaf, &[])), Ok(PalwGenVerdictV1::Acquitted));
    // The denoiser's last latent pushed past its proven interval: PALW-TIR-33 names that leaf.
    let post = (c.programs[1].occurrences().len() - 1) as u16;
    let last = e.space.stages[1].trip - 1;
    let latent = e.space.stages[1]
        .leaves()
        .iter()
        .position(|l| l.coord.pos == last && matches!(l.coord.kind, PalwGenLeafKindV1::Commit { occurrence, node } if occurrence == post && node == c.programs[1].output.node()))
        .unwrap();
    let lied = lie(&e, 1, latent, 0, 1 << 22);
    match judge_toy(&lied, &close(&lied, 2, dec_leaf, &[])) {
        Ok(PalwGenVerdictV1::Convicted { leaf, fault: PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 0 } }) => {
            assert_eq!(leaf, e.space.stages[1].leaves()[latent].coord);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn evidence_that_fails_convicts_nobody() {
    let c = class("toy-vlm.json");
    let (job, image, tiles) = vlm_job(&c);
    let e = palw_gen_execute_v1(&c.pipeline, &c.programs, &c.layouts, &c.params, &job, &decode(), [0; 32]).unwrap();
    let reader = e.space.stages[1]
        .leaves()
        .iter()
        .position(|l| l.coord.pos == 1 && matches!(l.coord.kind, PalwGenLeafKindV1::Commit { occurrence: 0, .. }))
        .unwrap();
    let good = close(&e, 1, reader, &tiles);
    // A disputed leaf not under the root.
    let mut forged = good.clone();
    forged.disputed.values[0] += 1;
    assert_eq!(judge(&c, &e, &job, image, &forged), Err(PalwGenCloseRefusalV1::LeafNotProven(good.disputed.coord)));
    // An operand that does not precede.
    let mut later = good.clone();
    later.operands.push(e.open(1, reader as u64 + 1).unwrap());
    assert!(matches!(judge(&c, &e, &job, image, &later), Err(PalwGenCloseRefusalV1::NotPreceding(_))));
    // The image rows not carried: the evaluation cannot run.
    let mut thin = good.clone();
    thin.operands.retain(|o| o.coord.stage == 1);
    assert!(matches!(judge(&c, &e, &job, image, &thin), Err(PalwGenCloseRefusalV1::Incomplete(_))));
    // A forged image tile.
    let vision = close(&e, 0, 0, &tiles);
    let mut bad_tile = vision.clone();
    bad_tile.image_tiles[1].bytes[0] ^= 1;
    assert!(matches!(
        judge(&c, &e, &job, image, &bad_tile),
        Err(PalwGenCloseRefusalV1::Image(PalwGenImageRefusalV1::TileNotProven { .. }))
    ));
    // Roots the step root does not bind.
    let mut unbound = e.clone();
    unbound.claim.stage_roots[0] = Hash64::from_bytes([1; 64]);
    assert_eq!(judge(&c, &unbound, &job, image, &good), Err(PalwGenCloseRefusalV1::RootsNotBound));
}
