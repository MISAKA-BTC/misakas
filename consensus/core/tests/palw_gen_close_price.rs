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
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2;
use kaspa_consensus_core::palw_fp_job_v5::PalwFreePromptJobV5;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V4_VERSION, PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_gen_artifact_v1::{PalwGenInventoryIndexV1, palw_gen_inventory_root_v1};
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
use kaspa_consensus_core::palw_gen_admission_v1::{PalwGenAdmissionRulesV1, PalwGenAdmittedV1, verify_gen_class_admission_v1};
use kaspa_consensus_core::palw_gen_close_price_v1::{PalwGenWholeCloseKindV1, palw_gen_whole_closes_v1, palw_gen_worst_closes_of_class_v1};
use kaspa_consensus_core::palw_gen_one_move_v1::palw_gen_one_move_max_proof_bytes_v1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_consensus_core::palw_gen_close_v1::*;
use kaspa_consensus_core::palw_gen_job_v1::*;
use kaspa_consensus_core::palw_gen_step_v1::PalwGenLeafKindV1;
use kaspa_consensus_core::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenDecodeV1, PalwGenExecutionV1, palw_gen_execute_tensor_v1, palw_gen_execute_v1};
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

/// The twin every sizing here is asked with (the element twin; the range twin is compared with it at the end of the file).
const TWIN: kaspa_consensus_core::palw_tir_close_range_v1::PalwTirCloseTwinV1 =
    kaspa_consensus_core::palw_tir_close_range_v1::PalwTirCloseTwinV1::Element;

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
    layouts_at(pipeline, programs, tile, 16, 1)
}

/// [`layouts`] with the history tile and the checkpoint interval given.
fn layouts_at(pipeline: &TirPipelineV1, programs: &[TirProgramV2], tile: u32, h_tile: u32, checkpoint: u32) -> Vec<PalwTirLayoutV1> {
    pipeline
        .stages
        .iter()
        .map(|st| {
            let p = &programs[st.program as usize];
            let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
            PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: st.max_trip,
                checkpoint_interval: checkpoint,
                h_tile,
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
        palw_gen_worst_closes_of_class_v1(&f.class, &f.pipeline, programs, leaves, true, u64::MAX, u64::MAX, 1 << 30, TWIN).expect("the sizing");
    let by_point: BTreeMap<(u8, u8, u16), (u64, bool)> = priced
        .iter()
        .enumerate()
        .flat_map(|(s, st)| st.iter().filter(|b| b.checkpoint.is_none()).map(move |b| ((s as u8, b.block, b.node), (b.close_bytes, b.dissected))))
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
        palw_gen_worst_closes_of_class_v1(&f.class, &f.pipeline, programs, leaves, true, carriable, u64::MAX, 1 << 30, TWIN).expect("sizes").0
    };
    let sized: usize = all(u64::MAX).iter().map(Vec::len).sum();
    let worst = all(u64::MAX).iter().flatten().map(|b| b.close_bytes).max().unwrap();
    // A carrier smaller than the worst close: the sizing stops at the first commit point past it, and that point is the last bound.
    let stopped = all(worst - 1);
    let last = stopped.iter().rev().find(|s| !s.is_empty()).and_then(|s| s.last()).unwrap();
    assert!(last.close_bytes > worst - 1, "the last bound sized is the one past the carrier");
    assert!(stopped.iter().map(Vec::len).sum::<usize>() <= sized);
    // A work cap below what the sizing needs: refused by name, never run.
    match palw_gen_worst_closes_of_class_v1(&f.class, &f.pipeline, programs, leaves, true, u64::MAX, u64::MAX, 1_000, TWIN) {
        Err(PalwClassAdmissionError::TirExceeds { limit, cap, .. }) => {
            assert_eq!(limit, "generative close sizing work");
            assert_eq!(cap, 1_000);
        }
        other => panic!("a sizing past its work cap is refused by name: {other:?}"),
    }
}

// ---- a Text class (the golden toy VLM: a vision stage, the language model as the text stage over `TextStream`, one image) --------

fn vlm() -> (Fixture, PalwFreePromptJobV5) {
    let (f, job, _) = text_vector("toy-vlm.json", 4, 16, 1, 4, Some(vlm_prompt()), 4);
    (f, job)
}

/// **A golden pipeline vector as a registered Text class with one image slot**, its weights, a V5 job of it (the key as long as it is
/// on chain, a greedy budget of `budget` ids) and the prompt — the toy VLM and tir-lower's lowered tiny LLaVA / Qwen2-VL / Qwen2.5-VL.
fn text_vector(
    name: &str,
    tile: u32,
    h_tile: u32,
    checkpoint: u32,
    image_tile: u32,
    prompt: Option<Vec<u32>>,
    budget: u32,
) -> (Fixture, PalwFreePromptJobV5, Vec<u32>) {
    let (v, pipeline_bytes, program_bytes, pipeline, programs, params) = load(name);
    let img = &v["job"]["images"][0];
    let image = JobImageV1 {
        h: img["h"].as_u64().unwrap() as u32,
        w: img["w"].as_u64().unwrap() as u32,
        rgb: unhex(img["rgb_hex"].as_str().unwrap()),
    };
    let prompt = prompt.unwrap_or_else(|| v["job"]["prompt"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as u32).collect());
    text_fixture(
        pipeline_bytes,
        program_bytes,
        pipeline,
        programs,
        params,
        image,
        [tile, h_tile, checkpoint, image_tile],
        prompt,
        budget,
        DecodeConfigV4::NOOP,
    )
}

/// **A pipeline of programs as a registered Text class with one image slot**, its weights, a V5 job of it and the prompt: the part of
/// [`text_vector`] after the golden file is read, so a class built here (a real vocabulary) is held to the same measurement.
/// `shape` is `[tile, h_tile, checkpoint, image_tile]`.
#[allow(clippy::too_many_arguments)]
fn text_fixture(
    pipeline_bytes: Vec<u8>,
    program_bytes: Vec<Vec<u8>>,
    pipeline: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    params: Params,
    image: JobImageV1,
    shape: [u32; 4],
    prompt: Vec<u32>,
    budget: u32,
    decode: DecodeConfigV4,
) -> (Fixture, PalwFreePromptJobV5, Vec<u32>) {
    let [tile, h_tile, checkpoint, image_tile] = shape;
    let class = PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        pipeline: pipeline_bytes,
        programs: program_bytes,
        layouts: layouts_at(&pipeline, &programs, tile, h_tile, checkpoint),
        output: OutputSpecV1::tokens(pipeline.stages[pipeline.output_stage as usize].max_trip),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: 8.max(prompt.len() as u32),
            max_negative_tokens: 0,
            images: vec![PalwGenImageOfferV1 { h: image.h, w: image.w, tile_len: image_tile, token_equivalents: 1_000_000 }],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
            profile: PalwGenProfileOffersV1::None,
        },
        tokenizer_id: Hash64::from_bytes([0x72; 64]),
    };
    let row = registered(class.clone(), &programs, &params);
    let f = Fixture { class, row, pipeline, programs, params, image: Some(image) };
    // An FP Job V4 of the golden vectors, retargeted at the class, with the key as long as it is on chain.
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/fp-v4/job_v4_encoding.json");
    let j: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut v4: PalwFreePromptJobV3 = j["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| borsh::from_slice::<PalwFreePromptJobV3>(&unhex(c["borsh_hex"].as_str().unwrap())).unwrap())
        .find(|j| j.version == PALW_FP_V4_VERSION)
        .unwrap();
    v4.class_id = f.row.class_id;
    v4.tokenizer_id = f.class.tokenizer_id;
    v4.prompt_tokens = prompt.len() as u32;
    v4.prompt_token_ids_hash = prompt_token_ids_commitment_v1(FORM, &prompt).unwrap();
    v4.decode_token_limit = budget;
    v4.decode = Some(decode);
    v4.temperature_q = PalwDecodeSamplingV2::GREEDY.temperature_q;
    v4.executor_pubkey = vec![0xAB; kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
    let image = f.image.as_ref().unwrap();
    let input_root = misaka_palw_gen::output::input_image_root_v1(image.h, image.w, image_tile, &image.rgb).unwrap();
    let images = vec![PalwGenImageInputRefV1 { input_root: Hash64::from_bytes(input_root), h: image.h, w: image.w }];
    (f, PalwFreePromptJobV5 { v4, images, source: None }, prompt)
}

fn vlm_prompt() -> Vec<u32> {
    vec![3, 15, 15, 5]
}

#[test]
fn the_price_bounds_every_measured_close_of_the_toy_vlm_class() {
    let (f, job) = vlm();
    let run_job = misaka_palw_tir::pipeline::PipelineJob {
        prompt: vlm_prompt(),
        images: vec![f.image.clone().unwrap()],
        ..misaka_palw_tir::pipeline::PipelineJob::default()
    };
    let decode = PalwGenDecodeV1::of(&job).unwrap();
    let e = palw_gen_execute_v1(&f.pipeline, &f.programs, &f.class.layouts, &f.params, &run_job, &decode, job.v4.sampling_seed).unwrap();
    let binding = PalwGenStepBindingV1::of(&job, &e.claim, e.space.leaf_count());
    let leaves = PalwGenInventoryIndexV1::new(&f.programs).expect("an inventory").leaf_count();
    let (priced, work) =
        palw_gen_worst_closes_of_class_v1(&f.class, &f.pipeline, &f.programs, leaves, true, u64::MAX, u64::MAX, 1 << 30, TWIN).expect("sizing");
    let by_point: BTreeMap<(u8, u8, u16), (u64, bool)> = priced
        .iter()
        .enumerate()
        .flat_map(|(s, st)| st.iter().filter(|b| b.checkpoint.is_none()).map(move |b| ((s as u8, b.block, b.node), (b.close_bytes, b.dissected))))
        .collect();
    // The checkpoint leaves' prices, by `(stage, state)`.
    let by_state: BTreeMap<(u8, u16), u64> = priced
        .iter()
        .enumerate()
        .flat_map(|(s, st)| st.iter().filter_map(move |b| b.checkpoint.map(|state| ((s as u8, state), b.close_bytes))))
        .collect();
    let prompt = vlm_prompt();
    let images: Vec<JobImageV1> = f.image.iter().cloned().collect();
    let ev = PalwGenEvidenceV1 {
        row: &f.row,
        params: &f.params,
        execution: &e,
        binding: binding.into(),
        prompt: &prompt,
        negative: &[],
        images: &images,
        source: &[],
    };
    let (mut done, mut margin, mut state_leaves, mut worst_state) = (0usize, i64::MAX, 0usize, 0u64);
    let mut global = 0u64;
    for (s, stage) in e.space.stages.iter().enumerate() {
        for leaf in stage.leaves() {
            let index = global;
            global += 1;
            let close = || {
                let c = ev.cone_close(index, &LIMITS).unwrap_or_else(|e| panic!("stage {s} leaf {index}: {e}"));
                borsh::to_vec(&PalwCourtVerdictProofV2::GenCone { close: Box::new(c) }).unwrap().len() as u64
            };
            let PalwGenLeafKindV1::Commit { occurrence, node } = leaf.coord.kind else {
                let PalwGenLeafKindV1::State { state, .. } = leaf.coord.kind else { unreachable!() };
                let bytes = close();
                let price = by_state
                    .get(&(s as u8, state))
                    .copied()
                    .unwrap_or_else(|| panic!("toy vlm: no price for the checkpoint of state {state} of stage {s}"));
                assert!(price >= bytes, "toy vlm: stage {s} checkpoint of state {state} at leaf {index}: priced {price} B, measured {bytes} B");
                state_leaves += 1;
                worst_state = worst_state.max(bytes);
                continue;
            };
            let block = stage.occurrence_block(occurrence).expect("an occurrence");
            let (price, dissected) = by_point[&(s as u8, block, node)];
            if dissected || f.row.dissected.contains(&(s as u8, block, node)) {
                continue;
            }
            let bytes = close();
            assert!(price >= bytes, "toy vlm: stage {s} block {block} node {node} at leaf {index}: priced {price} B, measured {bytes} B");
            margin = margin.min(price as i64 - bytes as i64);
            done += 1;
        }
    }
    let worst_priced = by_point.values().map(|p| p.0).max().unwrap_or(0);
    eprintln!(
        "toy vlm: {} commit points sized in {work} steps, {done} cone closes measured, the smallest margin {margin} B; {state_leaves} checkpoint leaves (priced per state: {by_state:?}), the largest of their closes {worst_state} B (the largest priced commit-point close {worst_priced} B)",
        by_point.len()
    );
    assert!(done > 0);
}

// ---- the closes that are not cone closes: a text class's DECODE close and a tensor class's OUTPUT close --------------------------------

/// A text job's execution and the claim's binding.
fn run_text(f: &Fixture, job: &PalwFreePromptJobV5, prompt: &[u32]) -> (PalwGenExecutionV1, PalwGenStepBindingV1) {
    let run_job = misaka_palw_tir::pipeline::PipelineJob {
        prompt: prompt.to_vec(),
        images: f.image.iter().cloned().collect(),
        ..misaka_palw_tir::pipeline::PipelineJob::default()
    };
    let decode = PalwGenDecodeV1::of(job).unwrap();
    let e = palw_gen_execute_v1(&f.pipeline, &f.programs, &f.class.layouts, &f.params, &run_job, &decode, job.v4.sampling_seed).unwrap();
    let binding = PalwGenStepBindingV1::of(job, &e.claim, e.space.leaf_count());
    (e, binding)
}

/// **Every decode close of a run, measured, against the price**: the cone-free `GenDecodeToken` of each generated id — every tile of
/// the committed logits row, as the one-move accusation carries it. Returns `(ids measured, the largest measured, the price)`.
fn decode_bounded(f: &Fixture, job: &PalwFreePromptJobV5, prompt: &[u32], what: &str) -> (usize, u64, u64) {
    let (e, binding) = run_text(f, job, prompt);
    let prices = palw_gen_whole_closes_v1(&f.class, &f.pipeline, &f.programs).expect("the whole closes");
    let (_, priced) = *prices.iter().find(|(k, _)| *k == PalwGenWholeCloseKindV1::Decode).expect("a text class has a decode close");
    assert_eq!(prices.len(), 1, "a text class has one whole close: its decode close");
    let images: Vec<JobImageV1> = f.image.iter().cloned().collect();
    let ev = PalwGenEvidenceV1 {
        row: &f.row,
        params: &f.params,
        execution: &e,
        binding: binding.into(),
        prompt,
        negative: &[],
        images: &images,
        source: &[],
    };
    assert!(!e.claim.generated.is_empty(), "{what}: the run generates ids");
    let (mut worst, mut n) = (0u64, 0usize);
    for t in 0..e.claim.generated.len() as u32 {
        let close = ev.decode_close(t).unwrap_or_else(|er| panic!("{what}: the decode close of id {t}: {er}"));
        let bytes = borsh::to_vec(&PalwCourtVerdictProofV2::GenDecodeToken { close: Box::new(close) }).unwrap().len() as u64;
        assert!(priced >= bytes, "{what}: id {t}: priced {priced} B, measured {bytes} B");
        worst = worst.max(bytes);
        n += 1;
    }
    eprintln!("{what}: {n} decode closes measured, the largest {worst} B, priced {priced} B (margin {} B)", priced - worst);
    (n, worst, priced)
}

#[test]
fn the_price_bounds_every_measured_decode_close_of_the_toy_text_class() {
    for tile in [4u32, 8, 16] {
        let (f, job, prompt) = text_vector("toy-vlm.json", tile, 16, 1, 4, Some(vlm_prompt()), 4);
        let (n, _, _) = decode_bounded(&f, &job, &prompt, &format!("toy vlm, tile {tile}"));
        assert!(n >= 2);
    }
}

/// tir-lower's lowered tiny LLaVA — the real architecture at tiny width, HF-lowered with its own calibrated weights — run on its own job.
/// (The Qwen2-VL / Qwen2.5-VL vectors carry programs for admission only: their weights are incomplete, so they are priced, never run.)
#[test]
fn the_price_bounds_every_measured_decode_close_of_the_lowered_tiny_llava() {
    let (f, job, prompt) = text_vector("vlm-llava-tiny.json", 16, 8, 4, 64, None, 6);
    let (n, _, _) = decode_bounded(&f, &job, &prompt, "vlm-llava-tiny.json");
    assert!(n >= 2);
}

/// **The output close of a tensor class**, every output tile of a claim, measured against the price.
fn output_bounded(f: &Fixture, job: &PalwGenJobV1, ids: PalwGenIdsV1<'_>, what: &str) {
    let (e, binding) = run(f, job, ids);
    let prices = palw_gen_whole_closes_v1(&f.class, &f.pipeline, &f.programs).expect("the whole closes");
    assert_eq!(prices.len(), 1, "a tensor class has one whole close: its output close");
    let (kind, priced) = prices[0];
    assert_eq!(kind, PalwGenWholeCloseKindV1::Output);
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
    let tile_len = palw_gen_output_tile_len_v1(&f.pipeline, &f.programs, &f.class.layouts).expect("the output node's tile");
    let tiles = misaka_palw_gen::output::output_tile_count_v1(&f.class.output, tile_len).expect("the output's tiles");
    let mut worst = 0u64;
    for tile in 0..tiles {
        let close = ev.output_close(tile).unwrap_or_else(|er| panic!("{what}: the output close of tile {tile}: {er}"));
        let bytes = borsh::to_vec(&PalwCourtVerdictProofV2::GenOutputTile { close: Box::new(close) }).unwrap().len() as u64;
        assert!(priced >= bytes, "{what}: output tile {tile}: priced {priced} B, measured {bytes} B");
        worst = worst.max(bytes);
    }
    eprintln!("{what}: {tiles} output closes measured (every tile), the largest {worst} B, priced {priced} B (margin {} B)", priced - worst);
}

#[test]
fn the_price_bounds_every_measured_output_close_of_the_toy_tensor_classes() {
    for tile in [4u32, 8, 16] {
        let f = image(tile);
        let job = image_job(&f);
        let ids = PalwGenIdsV1 { prompt: &prompt(), ..PalwGenIdsV1::default() };
        output_bounded(&f, &job, ids, &format!("toy image, tile {tile}"));
        let f = vision(tile);
        let job = vision_job(&f);
        output_bounded(&f, &job, PalwGenIdsV1::default(), &format!("toy vision, tile {tile}"));
    }
}

// ---- a REAL VOCABULARY: the decode close at the sizes real language models have ---------------------------------------------------
//
// A decode close carries every tile of the committed logits row — `4 · V` bytes of lanes and `⌈V / T⌉` paths — so it is a property of
// the vocabulary `V` and the logits node's commit tile `T`, not of the model's depth. The golden toy VLM has a vocabulary of 16; this
// is its language model at a real one: the golden program's own structure, the head laid out as HF lays out `lm_head` (`[V, D]`: a
// tile of the logits reads that many ROWS, each a leaf of the artifact), the vision tower the golden vector's (the job takes an image
// slot, so it is an FP Job V5 and has a generative binding at all).

/// The golden toy vision tower's width: the image rows the language model reads are `[2, D]`.
const TOY_D: u32 = 4;
/// The golden toy language model's image placeholder id.
const TOY_PLACEHOLDER: u32 = 15;

/// **The toy VLM's language model at vocabulary `vocab`.**
fn real_lm_program(vocab: u32) -> TirProgramV2 {
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::program_v2::{InputSource, OutputDecl};
    use misaka_palw_tir::{Cmp, DType, Ref, Rounding};
    let mut pb = ProgramBuilder::new(vocab, HISTORY_BOUND_V1_SMALL);
    let rows = pb.param("lm.image_rows", DType::I32, &[2, TOY_D], false);
    let emb = pb.param("lm.emb", DType::I8, &[vocab, TOY_D], false);
    let head = pb.param("lm.head", DType::I8, &[vocab, TOY_D], false);
    let cursor = pb.fixed_state("lm.cursor", DType::I32, &[1], 0, 2, false);
    let pre = {
        let mut b = pb.block("lm.pre", vec![]);
        let e = b.gather(emb, Ref::Input(0), 0, 0);
        let e = b.cast(e, DType::I32);
        let e = b.reshape_fixed(e, &[1, TOY_D]);
        let ph = b.c(DType::Idx, TOY_PLACEHOLDER as i128);
        let is_ph = b.compare(Ref::Input(0), ph, Cmp::Eq);
        let rows_n = b.c(DType::I32, 2);
        let room = b.compare(Ref::State(cursor), rows_n, Cmp::Lt);
        let no = b.c(DType::I8, 0);
        let is_img = b.select(is_ph, room, no, DType::I8);
        let at = b.clamp(Ref::State(cursor), 0, 1, DType::Idx);
        let row = b.gather(rows, at, 0, 0);
        let row = b.shr(row, 16, Rounding::HalfAwayFromZero, DType::I32);
        let row = b.clamp(row, -128, 127, DType::I32);
        let x = b.select(is_img, row, e, DType::I32);
        let step = b.cast(is_img, DType::I32);
        let next = b.add(Ref::State(cursor), step, DType::I32);
        let next = b.clamp(next, 0, 2, DType::I32);
        b.state_write(cursor, next);
        // The head as HF has it: `[V, D] · [D, 1]`.
        let xt = b.reshape_fixed(x, &[TOY_D, 1]);
        let l = b.matmul(head, xt, DType::I32);
        let l = b.reshape_fixed(l, &[1, vocab]);
        b.finish(&[l])
    };
    let carry = {
        let b = &pb.blocks[pre as usize];
        vec![b.nodes[b.carry_out[0] as usize].out.clone()]
    };
    let (post, out) = {
        let mut b = pb.block("lm.post", carry);
        let l = b.clamp(Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(l);
        let Ref::Node(n) = l else { unreachable!("a clamp is a node") };
        (b.finish(&[]), n)
    };
    let v1 = pb.finish(pre, vec![], post, out);
    TirProgramV2::from_v1_lifting_params(
        &v1,
        &[(0, InputSource::External { lo: i32::MIN as i64, hi: i32::MAX as i64 })],
        OutputDecl::Logits { node: out, scheme_id: v1.logits_scheme_id },
    )
    .expect("the toy language model at a real vocabulary lifts")
}

/// **The toy VLM with a language model of `vocab` ids** whose logits commit in tiles of `tile` lanes — registered, its weights
/// (deterministic `i8` noise), a V5 job of it with its image and the prompt. `budget` is the greedy decode's ids.
fn real_vocab_vlm(vocab: u32, tile: u32, budget: u32) -> (Fixture, PalwFreePromptJobV5, Vec<u32>) {
    real_vocab_vlm_decoding(vocab, tile, budget, DecodeConfigV4::NOOP)
}

/// [`real_vocab_vlm`] with the job's decode rules given.
fn real_vocab_vlm_decoding(vocab: u32, tile: u32, budget: u32, decode: DecodeConfigV4) -> (Fixture, PalwFreePromptJobV5, Vec<u32>) {
    let (v, _, _, golden_pipeline, golden_programs, golden_params) = load("toy-vlm.json");
    let img = &v["job"]["images"][0];
    let image = JobImageV1 {
        h: img["h"].as_u64().unwrap() as u32,
        w: img["w"].as_u64().unwrap() as u32,
        rgb: unhex(img["rgb_hex"].as_str().unwrap()),
    };
    let lm = real_lm_program(vocab);
    let mut state = 0x9E37_79B9_7F4A_7C15u64 ^ vocab as u64;
    let mut noise = |n: usize| -> Vec<i128> {
        (0..n)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((state >> 33) % 255) as i128 - 127
            })
            .collect()
    };
    let shape = vec![vocab as usize, TOY_D as usize];
    let mut weights = MapParams::default();
    weights.tensors.insert((0, None), Tensor::new(misaka_palw_tir::DType::I8, shape.clone(), noise(shape.iter().product())).unwrap());
    weights.tensors.insert((1, None), Tensor::new(misaka_palw_tir::DType::I8, shape.clone(), noise(shape.iter().product())).unwrap());
    let programs = vec![golden_programs[0].clone(), lm];
    let params = Params(vec![golden_params.0[0].clone(), weights]);
    let program_bytes: Vec<Vec<u8>> = programs.iter().map(|p| p.encode()).collect();
    text_fixture(golden_pipeline.encode(), program_bytes, golden_pipeline, programs, params, image, [tile, 16, 4, 4], vlm_prompt(), budget, decode)
}

/// The grid of the sweep: vocabularies a small, a Llama-2-class and a Qwen2.5-class model have, at the tiles a registrant would try.
const REAL_VOCABS: [u32; 3] = [4_096, 32_000, 151_936];

/// **The decode close, measured at every id of a run, against the price** — a small, a Llama-2-class and a Qwen2.5-class vocabulary at
/// the tiles a registrant would try, down to 16 lanes a tile (9,496 paths and an 11 MB close at 151,936 ids).
#[test]
fn the_price_bounds_every_measured_decode_close_at_real_vocabularies() {
    let grid = [
        (4_096u32, 16u32),
        (4_096, 64),
        (4_096, 256),
        (32_000, 16),
        (32_000, 256),
        (32_000, 1_024),
        (151_936, 16),
        (151_936, 64),
        (151_936, 256),
        (151_936, 1_024),
    ];
    for (vocab, tile) in grid {
        let (f, job, prompt) = real_vocab_vlm(vocab, tile, 4);
        let (n, worst, priced) = decode_bounded(&f, &job, &prompt, &format!("real-vocab VLM V={vocab} T={tile}"));
        assert!(n >= 2);
        // The closed form's lanes alone are `4 · V`: the price is never below them, and the measured close is within the price.
        assert!(priced >= 4 * vocab as u64 && worst >= 4 * vocab as u64, "V={vocab} T={tile}: the lanes of a logits row are 4 · V bytes");
    }
}

/// **The widest decode rules a job can carry** (`DecodeConfigV4`'s own bounds: 300 logit-bias entries, 4 stop sequences of 16 ids, every
/// penalty active over the longest window). The binding carries the whole job, so the decode rules ride every close; the price's frame is
/// the object at its widest binding, and the measured close of a job at the widest rules must not exceed it.
fn widest_decode_rules() -> DecodeConfigV4 {
    DecodeConfigV4 {
        repeat_penalty_q: 2 * 65_536,
        penalty_window: 256,
        frequency_penalty_q: 1 << 20,
        presence_penalty_q: -(1 << 20),
        logit_bias: (0..300u32).map(|i| (i, 1 << 24)).collect(),
        stop_sequences: (0..4u32).map(|i| vec![1_000 + i; 16]).collect(),
    }
}

#[test]
fn the_price_bounds_a_decode_close_of_a_job_at_the_widest_decode_rules() {
    for (vocab, tile) in [(4_096u32, 64u32), (32_000, 256)] {
        let (f, job, prompt) = real_vocab_vlm_decoding(vocab, tile, 4, widest_decode_rules());
        let (_, worst, priced) = decode_bounded(&f, &job, &prompt, &format!("real-vocab VLM V={vocab} T={tile}, the widest decode rules"));
        let (g, job, prompt) = real_vocab_vlm(vocab, tile, 4);
        let (_, plain, _) = decode_bounded(&g, &job, &prompt, &format!("real-vocab VLM V={vocab} T={tile}, no decode rules"));
        assert!(worst > plain, "the decode rules ride the binding: {worst} B against {plain} B");
        assert!(priced >= worst);
    }
}

/// **`open_many` — one build of a stage's tree for the thousands of tiles of a logits row — opens every leaf exactly as `open` does**
/// (the same coordinate, values and path), at every stage of a pipeline, and refuses a leaf past the last.
#[test]
fn opening_many_leaves_of_a_stage_is_opening_each_of_them() {
    let (f, job, prompt) = real_vocab_vlm(4_096, 64, 3);
    let (e, _) = run_text(&f, &job, &prompt);
    for out in 0..f.pipeline.stages.len() as u8 {
        let n = e.space.stages[out as usize].leaves().len() as u64;
        let picks: Vec<u64> = (0..n).step_by(3).collect();
        let many = e.open_many(out, &picks).expect("the leaves open");
        for (k, i) in picks.iter().enumerate() {
            assert_eq!(many[k], e.open(out, *i).expect("the leaf opens"), "stage {out} leaf {i}");
        }
        assert!(e.open_many(out, &[n]).is_none(), "a leaf past the last does not open");
    }
}

// ---- the gate: a class whose decode close cannot ride a carrier is refused by name ---------------------------------------------------

const GATE_AT: u64 = 1_100;

fn armed() -> kaspa_consensus_core::config::params::Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(GATE_AT)));
    p.sync_palw_tir_v1();
    p.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(GATE_AT)));
    p.sync_palw_gen_v1();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("both fences at {GATE_AT}: {e}"));
    p
}

/// The gate's verdict on a registration of `class`, where closes may ride chunks (`chunks`: `palw_held_close_chunks_v1`) or only one
/// carrier (the held regime's one-move accusation, PALW-GEN-21).
fn gate(class: &PalwGenClassV1, chunks: bool) -> Result<PalwGenAdmittedV1, PalwClassAdmissionError> {
    let p = armed();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let rules = PalwGenAdmissionRulesV1::at(&p, GATE_AT).expect("the fence is in force");
    assert!(rules.held_armed, "the held regime");
    let rules = PalwGenAdmissionRulesV1 { held_close_chunks: chunks, ..rules };
    let bond = PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(3));
    let object = kaspa_consensus_core::palw_gen_admission_v1::palw_gen_post_genesis_registration_v1(
        class.clone(),
        Hash64::from_bytes([0xA7; 64]),
        0,
        1 << 100,
        1,
        GATE_AT,
        bond,
        vec![],
    )
    .expect("the registration counts");
    verify_gen_class_admission_v1(bundle, &rules, &object)
}

/// The largest priced CONE close of a class (what the gate priced before it priced the decode close).
fn cone_close_max(f: &Fixture) -> u64 {
    let leaves = PalwGenInventoryIndexV1::new(&f.programs).expect("an inventory").leaf_count();
    let (priced, _) =
        palw_gen_worst_closes_of_class_v1(&f.class, &f.pipeline, &f.programs, leaves, true, u64::MAX, u64::MAX, 1 << 30, TWIN).expect("sizing");
    priced.iter().flatten().map(|b| b.close_bytes).max().expect("a commit point")
}

/// What one carrier holds of a one-move accusation, and what the chunks carry.
fn carriers() -> (u64, u64) {
    let p = armed();
    let PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let carried = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1(&b.court).min(b.court.max_close_bytes());
    (palw_gen_one_move_max_proof_bytes_v1().min(carried), carried)
}

#[test]
fn a_decode_close_past_what_can_be_carried_is_refused_by_name_and_the_catalog_records_the_one_it_admits() {
    let (one_move, carried) = carriers();
    eprintln!("a one-move accusation carries {one_move} B; the carried cap (chunks) is {carried} B");
    for vocab in REAL_VOCABS {
        for tile in [16u32, 64, 256, 1_024] {
            let (f, _, _) = real_vocab_vlm(vocab, tile, 4);
            let prices = palw_gen_whole_closes_v1(&f.class, &f.pipeline, &f.programs).expect("the whole closes");
            let [(PalwGenWholeCloseKindV1::Decode, decode)] = prices[..] else { panic!("one decode close: {prices:?}") };
            let describe = |r: &Result<PalwGenAdmittedV1, PalwClassAdmissionError>| match r {
                Ok(a) => format!("admitted (the catalog's largest close {} B)", a.entry.court_cost.max_close_bytes),
                Err(e) => e.to_string(),
            };
            let (one, chunked) = (gate(&f.class, false), gate(&f.class, true));
            let cones = cone_close_max(&f);
            eprintln!(
                "V={vocab:>7} T={tile:>5}: decode close priced {decode:>9} B, the cone closes' largest {cones:>9} B | one-move: {} | chunks: {}",
                describe(&one),
                describe(&chunked)
            );
            for (chunks, verdict, cap) in [(false, one, one_move), (true, chunked, carried)] {
                match verdict {
                    Ok(a) => {
                        assert!(decode <= cap, "V={vocab} T={tile} chunks={chunks}: admitted with a decode close of {decode} B past {cap} B");
                        assert!(
                            a.entry.court_cost.max_close_bytes >= decode,
                            "V={vocab} T={tile}: the catalog records at least the decode close"
                        );
                    }
                    Err(PalwClassAdmissionError::CourtCostExceedsCeiling { what, got, ceiling }) => {
                        assert_eq!(ceiling, cap, "V={vocab} T={tile} chunks={chunks}: the bound is what the regime carries");
                        assert!(got > ceiling);
                        if decode > cap {
                            // The decode close is asked FIRST: past the cap it is the refusal, by name, whatever the cones hold.
                            assert_eq!(what, "generative decode close bytes as carried", "V={vocab} T={tile} chunks={chunks}");
                            assert_eq!(got, decode, "the refusal carries the price");
                        } else {
                            assert_eq!(what, "generative close bytes as carried", "V={vocab} T={tile} chunks={chunks}: a cone close");
                        }
                    }
                    Err(e) => panic!("V={vocab} T={tile} chunks={chunks}: {e}"),
                }
            }
        }
    }
}

#[test]
fn the_registrable_real_vocabulary_classes_are_where_the_regime_carries_their_decode_close() {
    let (one_move, carried) = carriers();
    // A 32,000-id vocabulary's logits alone are 128,000 B (`4 · V`): past one carrier at every tile, inside the carried cap. So it is
    // convictable only where closes ride chunks, and refused by name where they may not.
    let (f, _, _) = real_vocab_vlm(32_000, 256, 4);
    assert!(matches!(
        gate(&f.class, false),
        Err(PalwClassAdmissionError::CourtCostExceedsCeiling { what: "generative decode close bytes as carried", ceiling, .. }) if ceiling == one_move
    ));
    let admitted = gate(&f.class, true).unwrap_or_else(|e| panic!("V=32,000 T=256 with chunks: {e}"));
    assert!(admitted.entry.court_cost.max_close_bytes > one_move && admitted.entry.court_cost.max_close_bytes <= carried);
    // A Qwen2.5-sized vocabulary at a coarse tile is the same, at about 1.2 MB.
    let (f, _, _) = real_vocab_vlm(151_936, 256, 4);
    assert!(matches!(
        gate(&f.class, false),
        Err(PalwClassAdmissionError::CourtCostExceedsCeiling { what: "generative decode close bytes as carried", .. })
    ));
    gate(&f.class, true).unwrap_or_else(|e| panic!("V=151,936 T=256 with chunks: {e}"));
    // At 16 lanes a tile its 9,496 paths are 11 MB: past even the carried cap, refused by name everywhere. THE GAP: its cone closes are
    // 16 rows each, far inside one carrier, so before the decode close was priced the gate admitted it — a class whose every lie at a
    // generated id was unconvictable.
    let (f, _, _) = real_vocab_vlm(151_936, 16, 4);
    assert!(cone_close_max(&f) <= one_move, "the cone sizing alone admits this class: {} B", cone_close_max(&f));
    for chunks in [false, true] {
        assert!(
            matches!(
                gate(&f.class, chunks),
                Err(PalwClassAdmissionError::CourtCostExceedsCeiling { what: "generative decode close bytes as carried", got, .. }) if got > carried
            ),
            "chunks={chunks}"
        );
    }
}

/// **The generative gate's sizing work cap is no looser than the IR's** (coordinator, 2026-10-02): the same `2^26`. Defined as the IR's
/// constant, so they cannot drift apart by edit; held here so a redefinition fails by name, and the number is the decided one.
#[test]
fn the_generative_sizing_work_cap_is_no_looser_than_the_ir_one() {
    use kaspa_consensus_core::palw_gen_close_price_v1::PALW_GEN_CLOSE_SIZING_WORK_CAP_V1;
    use kaspa_consensus_core::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1;
    assert!(
        PALW_GEN_CLOSE_SIZING_WORK_CAP_V1 <= PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
        "the generative close sizing may do {PALW_GEN_CLOSE_SIZING_WORK_CAP_V1} steps, the IR's {PALW_TIR_CLOSE_SIZING_WORK_CAP_V1}: the gate is looser"
    );
    assert_eq!(PALW_GEN_CLOSE_SIZING_WORK_CAP_V1, 1 << 26, "2^26, as decided");
}

// =================================================================================================
// `palw_gen_range_twin_v1`: the range twin sizes every close of a pipeline exactly as the element twin
// =================================================================================================

/// Both twins over one class: the same bound of every commit point and checkpoint leaf of every stage, byte for byte; the range
/// twin's work beside the element twin's.
fn both_twins(f: &Fixture, what: &str) -> (u64, u64) {
    use kaspa_consensus_core::palw_tir_close_range_v1::PalwTirCloseTwinV1;
    let leaves = PalwGenInventoryIndexV1::new(&f.programs).expect("an inventory").leaf_count();
    let size = |twin| {
        palw_gen_worst_closes_of_class_v1(&f.class, &f.pipeline, &f.programs, leaves, true, u64::MAX, u64::MAX, 1 << 30, twin)
            .unwrap_or_else(|e| panic!("{what}: {twin:?}: {e}"))
    };
    let (element, we) = size(PalwTirCloseTwinV1::Element);
    let (range, wr) = size(PalwTirCloseTwinV1::Range);
    assert_eq!(element, range, "{what}: the range twin's bounds are the element twin's");
    eprintln!("{what}: {} bounds equal; work element {we}, range {wr}", element.iter().map(Vec::len).sum::<usize>());
    (we, wr)
}

#[test]
fn the_generative_range_twin_sizes_every_close_exactly_as_the_element_twin() {
    for tile in [16u32, 64] {
        both_twins(&vision(tile), &format!("toy vision class, tile {tile}"));
        both_twins(&image(tile), &format!("toy image class, tile {tile}"));
    }
    let (f, _) = vlm();
    both_twins(&f, "toy vision-language class (a tower stage and a text stage over its rows)");
    let (f, _, _) = real_vocab_vlm(32_000, 64, 8);
    both_twins(&f, "toy vision-language class at a 32,000-id vocabulary");
}
