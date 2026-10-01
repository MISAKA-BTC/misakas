//! **RFC-0003 PALW-GEN-20: the gate prices a generative close at least what the court's builder carries** — at every leaf of a run,
//! for the golden toy classes (`consensus-vectors/tir-v2/pipelines/toy-{image,vision}.json`, an image class with a prompt encoder, a
//! denoiser over the job's steps reading the encoder's rows, a random input and a decoder; an embedding class over a job image).
//!
//! The price ([`palw_gen_worst_closes_of_class_v1`], the PALW-TIR-38 twin run over every stage with the generative court's reading of
//! inputs and of `post`-written states, priced as a generative close carries them) is held to the measurement: the cone close of EVERY
//! non-dissected commit leaf of the run is built by the evidence builder the workers use (`PalwGenEvidenceV1::cone_close` — exactly the
//! units the court's evaluation read) and serialized as the one-move accusation carries it, and it must not exceed the price of its
//! commit point. The same bound is held, on a class this size, at every position and tile there is — the structural argument is the
//! twin's; this is its check on a class small enough to enumerate (the reduced SD3 pipeline's sweep is
//! `misaka-palw-sdk/tests/gen_sd3_class.rs`).

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PROMPT_MODE_USER};
use kaspa_consensus_core::palw_gen_artifact_v1::{PalwGenInventoryIndexV1, palw_gen_inventory_root_v1};
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_close_price_v1::palw_gen_worst_closes_of_class_v1;
use kaspa_consensus_core::palw_gen_close_v1::*;
use kaspa_consensus_core::palw_gen_job_v1::*;
use kaspa_consensus_core::palw_gen_step_v1::PalwGenLeafKindV1;
use kaspa_consensus_core::palw_gen_v1::PalwGenProfileV1;
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenExecutionV1, palw_gen_execute_tensor_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{Binding, JobImageV1, PipelineParams, TirPipelineV1, TokenSource, TripRule};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::tensor::Tensor;
use std::collections::BTreeMap;

const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;
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

struct Fixture {
    class: PalwGenClassV1,
    row: PalwGenClassRecordV1,
    pipeline: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    params: Params,
    image: Option<JobImageV1>,
}

fn load(name: &str) -> (serde_json::Value, Vec<u8>, Vec<Vec<u8>>, TirPipelineV1, Vec<TirProgramV2>, Params) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines").join(name);
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the vector")).expect("json");
    let program_bytes: Vec<Vec<u8>> =
        v["programs"].as_array().unwrap().iter().map(|p| unhex(p["program_borsh_hex"].as_str().unwrap())).collect();
    let programs: Vec<TirProgramV2> = program_bytes.iter().map(|b| TirProgramV2::decode_canonical(b).unwrap()).collect();
    let pipeline_bytes = unhex(v["pipeline_borsh_hex"].as_str().unwrap());
    let pipeline = TirPipelineV1::decode_canonical(&pipeline_bytes, &programs).unwrap();
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
    (v, pipeline_bytes, program_bytes, pipeline, programs, params)
}

/// One tile of `tile` lanes per commit point and state.
fn layouts(pipeline: &TirPipelineV1, programs: &[TirProgramV2], tile: u32) -> Vec<PalwTirLayoutV1> {
    pipeline
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
                commit_tiles: vec![tile; commits],
                state_tiles: vec![tile; p.states.len()],
            }
        })
        .collect()
}

fn registered(class: PalwGenClassV1, programs: &[TirProgramV2], params: &Params) -> PalwGenClassRecordV1 {
    let (root, _) = palw_gen_inventory_root_v1(programs, params).expect("the toy weights have an inventory");
    palw_gen_class_record_v1(&class, &root).expect("the class is a row")
}

/// The toy one-stage image encoder as an Embedding class of one image (a job-image input read through input tiles).
fn vision(tile: u32) -> Fixture {
    let (v, pipeline_bytes, program_bytes, pipeline, programs, params) = load("toy-vision.json");
    let img = &v["job"]["images"][0];
    let slot = PalwGenImageOfferV1 {
        h: img["h"].as_u64().unwrap() as u32,
        w: img["w"].as_u64().unwrap() as u32,
        tile_len: img["tile_len"].as_u64().unwrap() as u32,
        token_equivalents: 0,
    };
    let image = JobImageV1 { h: slot.h, w: slot.w, rgb: unhex(img["rgb_hex"].as_str().unwrap()) };
    let class = PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Embedding as u8,
        layouts: layouts(&pipeline, &programs, tile),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: 0,
            max_negative_tokens: 0,
            images: vec![slot],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
            profile: PalwGenProfileOffersV1::Embedding(PalwGenEmbeddingOffersV1 { pooling: PALW_GEN_POOLING_CLS_V1, dims: vec![4] }),
        },
        output: OutputSpecV1::embedding_i32(1, 4, 0, false),
        pipeline: pipeline_bytes,
        programs: program_bytes,
        tokenizer_id: Hash64::from_bytes([0x71; 64]),
    };
    let row = registered(class.clone(), &programs, &params);
    Fixture { class, row, pipeline, programs, params, image: Some(image) }
}

/// The toy text-to-image pipeline as an Image class: a prompt encoder, a denoiser over the steps (reading the encoder's rows, a random
/// input and the step scalars), a decoder.
fn image(tile: u32) -> Fixture {
    let (_, pipeline_bytes, program_bytes, pipeline, programs, params) = load("toy-image.json");
    let steps_max = pipeline.stages.iter().filter(|st| matches!(st.trip, TripRule::JobSteps)).map(|st| st.max_trip).min().unwrap();
    let mut scalars = Vec::new();
    for st in &pipeline.stages {
        let ext = programs[st.program as usize].inputs.iter().filter(|d| d.is_external());
        for (b, d) in st.bind.iter().zip(ext) {
            if let Binding::JobScalar { index } = b {
                let (lo, hi) = d.interval();
                if scalars.len() <= *index as usize {
                    scalars.resize(*index as usize + 1, PalwGenScalarOfferV1 { lo: 0, hi: 0 });
                }
                scalars[*index as usize] = PalwGenScalarOfferV1 { lo: lo as i64, hi: hi as i64 };
            }
        }
    }
    let rule = pipeline.stages.iter().find_map(|st| st.tokens.as_ref()).expect("the encoder reads the prompt");
    assert_eq!(rule.source, TokenSource::Prompt);
    let encoder = pipeline.stages.iter().find(|st| st.tokens.is_some()).unwrap();
    let class = PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Image as u8,
        layouts: layouts(&pipeline, &programs, tile),
        offers: PalwGenOffersV1 {
            steps: vec![steps_max - 1, steps_max],
            profile: PalwGenProfileOffersV1::Image(PalwGenImageOffersV1 {
                sampler_id: Hash64::from_bytes([0x5A; 64]),
                guidance: Some(PalwGenGuidanceOfferV1 { scalar: 0, lo: scalars[0].lo as u16, hi: scalars[0].hi as u16 }),
                steps_scalar: Some(1),
            }),
            scalars,
            max_prompt_tokens: encoder.max_trip - (rule.prefix.len() + rule.suffix.len()) as u32,
            max_negative_tokens: 0,
            images: vec![],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
        },
        output: OutputSpecV1::image_rgb8(2, 2),
        pipeline: pipeline_bytes,
        programs: program_bytes,
        tokenizer_id: Hash64::from_bytes([0x71; 64]),
    };
    let row = registered(class.clone(), &programs, &params);
    Fixture { class, row, pipeline, programs, params, image: None }
}

fn envelope(f: &Fixture) -> PalwJobEnvelopeV1 {
    PalwJobEnvelopeV1 {
        network_domain: Hash64::from_bytes([0xD0; 64]),
        class_id: f.row.class_id,
        executor_bond: TransactionOutpoint::new(TransactionId::from_bytes([7; 64]), 3),
        // The bond's key as long as it is on chain: a close carries the whole job.
        executor_pubkey: vec![0xAB; kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN],
        operator_id: Hash64::from_bytes([0x0E; 64]),
        anchor_block: Hash64::from_bytes([0xA1; 64]),
        anchor_daa: 4_242,
        job_nonce: [0x9C; 32],
        privacy_mode: PALW_FP_PRIVACY_PANEL_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
    }
}

fn vision_job(f: &Fixture) -> PalwGenJobV1 {
    let image = f.image.as_ref().unwrap();
    let reference = palw_gen_image_input_ref_v1(image, f.class.offers.images[0].tile_len).unwrap();
    PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: envelope(f),
        seed: [0; 32],
        body: PalwGenBodyV1::Embedding(PalwGenEmbeddingBodyV1 {
            input: PalwGenEmbeddingInputV1::Image(reference),
            pooling: PALW_GEN_POOLING_CLS_V1,
            dims: 4,
            output: misaka_palw_gen::OutputKindV1::EmbeddingI32.tag(),
        }),
    }
}

fn prompt() -> Vec<u32> {
    vec![2, 5, 1]
}

/// The image class's job: the prompt, the guidance at the grid's low end, the LAST offered step count (the widest job).
fn image_job(f: &Fixture) -> PalwGenJobV1 {
    let PalwGenProfileOffersV1::Image(offers) = &f.class.offers.profile else { unreachable!() };
    let g = offers.guidance.as_ref().unwrap();
    PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: envelope(f),
        seed: [0x44; 32],
        body: PalwGenBodyV1::Image(PalwGenImageBodyV1 {
            prompt_token_ids_hash: prompt_token_ids_commitment_v1(FORM, &prompt()).unwrap(),
            prompt_tokens: prompt().len() as u32,
            negative_token_ids_hash: Hash64::default(),
            negative_tokens: 0,
            guidance_q: g.lo,
            image_index: 0,
            sampler_id: offers.sampler_id,
            steps: *f.class.offers.steps.last().unwrap() as u16,
            width: 2,
            height: 2,
            output: misaka_palw_gen::OutputKindV1::ImageRgb8.tag(),
        }),
    }
}

fn run(f: &Fixture, job: &PalwGenJobV1, ids: PalwGenIdsV1<'_>) -> (PalwGenExecutionV1, PalwGenTensorBindingV1) {
    let accepted = palw_gen_job_resolve_class_v1(job, &f.row).unwrap_or_else(|e| panic!("the job is the class's: {e}"));
    palw_gen_job_ids_admitted_v1(&f.row, &accepted, ids, FORM).unwrap_or_else(|e| panic!("the ids are the job's: {e}"));
    let images: Vec<JobImageV1> = f.image.iter().cloned().collect();
    let run_job = palw_gen_pipeline_job_v1(&accepted, ids, images);
    let e = palw_gen_execute_tensor_v1(
        &f.pipeline,
        &f.programs,
        &f.class.layouts,
        &f.params,
        &run_job,
        job.seed,
        accepted.item_index,
        &f.class.output,
    )
    .unwrap_or_else(|e| panic!("the pipeline runs: {e}"));
    let root = e.claim.output_root.expect("a tensor run commits an output root");
    let binding = PalwGenTensorBindingV1::of(job, &e.claim, e.space.leaf_count(), root);
    (e, binding)
}

/// **The check**: every non-dissected commit leaf of the run, its cone close measured, against the price of its commit point.
/// Returns `(leaves measured, the smallest margin, the largest priced close)`.
fn priced_bounds_measured(f: &Fixture, job: &PalwGenJobV1, ids: PalwGenIdsV1<'_>, what: &str) -> (usize, i64, u64) {
    let (e, binding) = run(f, job, ids);
    let programs = &f.programs;
    let leaves = PalwGenInventoryIndexV1::new(programs).expect("an inventory").leaf_count();
    let (priced, work) =
        palw_gen_worst_closes_of_class_v1(&f.class, &f.pipeline, programs, leaves, true, u64::MAX, u64::MAX, 1 << 30).expect("the sizing");
    let by_point: BTreeMap<(u8, u8, u16), (u64, bool)> = priced
        .iter()
        .enumerate()
        .flat_map(|(s, st)| st.iter().map(move |b| ((s as u8, b.block, b.node), (b.close_bytes, b.dissected))))
        .collect();
    let ev = PalwGenEvidenceV1 {
        row: &f.row,
        params: &f.params,
        execution: &e,
        binding: binding.into(),
        prompt: ids.prompt,
        negative: ids.negative,
        images: f.image.as_slice(),
        source: &[],
    };
    let (mut done, mut margin, mut state_leaves) = (0usize, i64::MAX, 0usize);
    let mut global = 0u64;
    for (s, stage) in e.space.stages.iter().enumerate() {
        for leaf in stage.leaves() {
            let index = global;
            global += 1;
            let PalwGenLeafKindV1::Commit { occurrence, node } = leaf.coord.kind else {
                state_leaves += 1;
                continue;
            };
            let block = stage.occurrence_block(occurrence).expect("an occurrence");
            let (price, dissected) = by_point[&(s as u8, block, node)];
            if dissected || f.row.dissected.contains(&(s as u8, block, node)) {
                continue;
            }
            let close = ev.cone_close(index, &LIMITS).unwrap_or_else(|e| panic!("{what}: stage {s} leaf {index}: {e}"));
            let bytes = borsh::to_vec(&PalwCourtVerdictProofV2::GenCone { close: Box::new(close) }).unwrap().len() as u64;
            assert!(
                price >= bytes,
                "{what}: stage {s} block {block} node {node} at leaf {index}: priced {price} B, measured {bytes} B"
            );
            margin = margin.min(price as i64 - bytes as i64);
            done += 1;
        }
    }
    eprintln!(
        "{what}: {} commit points sized in {work} steps, {done} cone closes measured (every non-dissected commit leaf), the smallest margin {margin} B, {state_leaves} checkpoint leaves",
        by_point.len()
    );
    assert!(done > 0, "{what}: nothing measured");
    (done, margin, by_point.values().map(|p| p.0).max().unwrap_or(0))
}

#[test]
fn the_price_bounds_every_measured_close_of_the_toy_image_class() {
    for tile in [4u32, 8] {
        let f = image(tile);
        let job = image_job(&f);
        let ids = PalwGenIdsV1 { prompt: &prompt(), ..PalwGenIdsV1::default() };
        priced_bounds_measured(&f, &job, ids, &format!("toy image, tile {tile}"));
    }
}

#[test]
fn the_price_bounds_every_measured_close_of_the_toy_embedding_class() {
    for tile in [4u32, 8] {
        let f = vision(tile);
        let job = vision_job(&f);
        priced_bounds_measured(&f, &job, PalwGenIdsV1::default(), &format!("toy vision, tile {tile}"));
    }
}

#[test]
fn a_close_priced_past_the_carrier_is_refused_by_name_and_the_sizing_stops_there() {
    let f = image(4);
    let programs = &f.programs;
    let leaves = PalwGenInventoryIndexV1::new(programs).unwrap().leaf_count();
    let all = |carriable: u64| {
        palw_gen_worst_closes_of_class_v1(&f.class, &f.pipeline, programs, leaves, true, carriable, u64::MAX, 1 << 30).expect("sizes").0
    };
    let sized: usize = all(u64::MAX).iter().map(Vec::len).sum();
    let worst = all(u64::MAX).iter().flatten().map(|b| b.close_bytes).max().unwrap();
    // A carrier smaller than the worst close: the sizing stops at the first commit point past it, and that point is the last bound.
    let stopped = all(worst - 1);
    let last = stopped.iter().rev().find(|s| !s.is_empty()).and_then(|s| s.last()).unwrap();
    assert!(last.close_bytes > worst - 1, "the last bound sized is the one past the carrier");
    assert!(stopped.iter().map(Vec::len).sum::<usize>() <= sized);
    // A work cap below what the sizing needs: refused by name, never run.
    match palw_gen_worst_closes_of_class_v1(&f.class, &f.pipeline, programs, leaves, true, u64::MAX, u64::MAX, 1_000) {
        Err(PalwClassAdmissionError::TirExceeds { limit, cap, .. }) => {
            assert_eq!(limit, "generative close sizing work");
            assert_eq!(cap, 1_000);
        }
        other => panic!("a sizing past its work cap is refused by name: {other:?}"),
    }
}
