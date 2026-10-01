//! **RFC-0003 step 7: the tensor claim — a job of an image or an embedding class, its canonical output,
//! and the court door that holds the output to its own step tree.**
//!
//! * the job (`PalwGenJobV1`) is one canonical encoding with one id, accepted against its class by name
//!   (`palw_gen_job_resolve_class_v1`), and maps to the facts a pipeline runs over;
//! * the worker (`palw_gen_execute_tensor_v1`) commits the output node's elements as the kind's
//!   canonical bytes under an `output_root`, and the claim's execution root commits both the step tree
//!   and that root (`PalwGenTensorBindingV1`);
//! * the output close (`GenOutputTile`, tag 13) holds one output tile, proven under the claim's root,
//!   to the output node's committed step tile of the same lanes: an honest claim is acquitted at every
//!   tile; a lane whose canonical bytes are not the committed value (`TirOutputDigestMismatch`), a
//!   committed lane outside the node's proven interval (`TirValueOutsideProvenInterval`) is convicted;
//!   evidence that does not hold (a tile not under the root, a step tile not under its stage's root, a
//!   step tile that is not the output's) convicts nobody.
//!
//! The classes are the golden toys (`consensus-vectors/tir-v2/pipelines/toy-{image,vision}.json`).

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER};
use kaspa_consensus_core::palw_gen_artifact_v1::palw_gen_inventory_root_v1;
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_close_v1::*;
use kaspa_consensus_core::palw_gen_job_v1::*;
use kaspa_consensus_core::palw_gen_step_v1::{PalwGenLeafCoordV1, PalwGenLeafKindV1, PalwGenLeafV1, palw_gen_step_leaf_hash_v1};
use kaspa_consensus_core::palw_tir_step_v1::{palw_tir_lane_values_v1, palw_tir_lanes_le_v1, palw_tir_lanes_wire_v1};
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::types::DType;
use kaspa_consensus_core::palw_gen_v1::PalwGenProfileV1;
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenExecutionV1, palw_gen_execute_tensor_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{Binding, JobImageV1, PipelineParams, TirPipelineV1, TokenSource, TripRule};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::tensor::Tensor;

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

/// A registered tensor class: its class, its row, its decoded pipeline, its weights.
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

fn layouts(pipeline: &TirPipelineV1, programs: &[TirProgramV2]) -> Vec<PalwTirLayoutV1> {
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
                commit_tiles: vec![4; commits],
                state_tiles: vec![4; p.states.len()],
            }
        })
        .collect()
}

fn registered(class: PalwGenClassV1, programs: &[TirProgramV2], params: &Params) -> PalwGenClassRecordV1 {
    let (root, _) = palw_gen_inventory_root_v1(programs, params).expect("the toy weights have an inventory");
    palw_gen_class_record_v1(&class, &root).expect("the class is a row")
}

/// The toy one-stage image encoder as an Embedding class of one image: `[1, 4]` lanes of `i32`.
fn vision() -> Fixture {
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
        layouts: layouts(&pipeline, &programs),
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

/// The toy text-to-image pipeline as an Image class: a `2×2` RGB image, two offered step counts, a
/// guidance scalar, a prompt.
fn image() -> Fixture {
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
        layouts: layouts(&pipeline, &programs),
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
        executor_pubkey: vec![0xAB; 8],
        operator_id: Hash64::from_bytes([0x0E; 64]),
        anchor_block: Hash64::from_bytes([0xA1; 64]),
        anchor_daa: 4_242,
        job_nonce: [0x9C; 32],
        privacy_mode: PALW_FP_PRIVACY_PANEL_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
    }
}

/// The vision class's job: embed the vector's one image.
fn vision_job(f: &Fixture) -> PalwGenJobV1 {
    let image = f.image.as_ref().unwrap();
    let reference = palw_gen_image_input_ref_v1(image, 4).unwrap();
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

/// The image class's job: the prompt, the guidance at the grid's low end, the first offered step count.
fn image_job(f: &Fixture, steps_position: usize, image_index: u16, seed: [u8; 32]) -> PalwGenJobV1 {
    let PalwGenProfileOffersV1::Image(offers) = &f.class.offers.profile else { unreachable!() };
    let g = offers.guidance.as_ref().unwrap();
    PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: envelope(f),
        seed,
        body: PalwGenBodyV1::Image(PalwGenImageBodyV1 {
            prompt_token_ids_hash: prompt_token_ids_commitment_v1(FORM, &prompt()).unwrap(),
            prompt_tokens: prompt().len() as u32,
            negative_token_ids_hash: Hash64::default(),
            negative_tokens: 0,
            guidance_q: g.lo,
            image_index,
            sampler_id: offers.sampler_id,
            steps: f.class.offers.steps[steps_position] as u16,
            width: 2,
            height: 2,
            output: misaka_palw_gen::OutputKindV1::ImageRgb8.tag(),
        }),
    }
}

/// The worker's run of a tensor job: the execution and its binding.
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

fn evidence<'a>(
    f: &'a Fixture,
    e: &'a PalwGenExecutionV1,
    binding: &PalwGenTensorBindingV1,
    ids: PalwGenIdsV1<'a>,
) -> PalwGenEvidenceV1<'a> {
    PalwGenEvidenceV1 {
        row: &f.row,
        params: &f.params,
        execution: e,
        binding: binding.clone().into(),
        prompt: ids.prompt,
        negative: ids.negative,
        images: f.image.as_slice(),
        source: &[],
    }
}

fn no_ids() -> PalwGenIdsV1<'static> {
    PalwGenIdsV1::default()
}

/// An execution whose claimed canonical output differs from its own step tree at one lane (the step
/// tree untouched, the output's root recomputed over the lie): the executor's digest fault.
fn lie_about_the_output(e: &PalwGenExecutionV1, lane: usize) -> PalwGenExecutionV1 {
    let mut l = e.clone();
    let out = l.output.as_mut().unwrap();
    out.values[lane] ^= 1;
    out.root = Hash64::from_bytes(misaka_palw_gen::output_root_v1(&out.spec, &out.values, out.tile_len).unwrap());
    l.claim.output_root = Some(out.root);
    l
}

/// An execution whose output node's committed step tile holds `value` at `lane` of the step tile at
/// `coord`, every root recomputed over it — the executor's lie about the computation, committed
/// consistently (its canonical output left as it was).
fn lie_in_the_step_tile(
    e: &PalwGenExecutionV1,
    coord: &kaspa_consensus_core::palw_gen_step_v1::PalwGenLeafCoordV1,
    lane: usize,
    value: i128,
) -> PalwGenExecutionV1 {
    use kaspa_consensus_core::palw_gen_step_v1::{palw_gen_stage_root_v1, palw_gen_step_leaf_hash_v1, palw_gen_step_root_v1};
    let mut l = e.clone();
    let stage = coord.stage as usize;
    let index = l.space.stages[stage].leaf_index(coord).unwrap() as usize;
    l.leaf_values[stage][index][lane] = value;
    let leaf = l.space.stages[stage].leaves()[index];
    l.leaf_hashes[stage][index] = palw_gen_step_leaf_hash_v1(&leaf, &l.leaf_values[stage][index]).unwrap();
    l.claim.stage_roots[stage] = palw_gen_stage_root_v1(stage as u8, &l.leaf_hashes[stage]);
    l.claim.step_root = palw_gen_step_root_v1(&l.claim.stage_roots);
    l
}

fn check(f: &Fixture, close: &PalwGenOutputCloseV1, binding: &PalwGenTensorBindingV1, narrowed: Option<u64>) -> PalwGenCloseOutcomeV1 {
    check_gen_output_close_v1(close, &f.row, &f.row.class_id, &binding.committed_execution_root, narrowed)
}

// ---------------------------------------------------------------------------------------------
// The job
// ---------------------------------------------------------------------------------------------

#[test]
fn a_job_is_one_canonical_encoding_with_one_id() {
    let f = image();
    let job = image_job(&f, 0, 0, [0x33; 32]);
    let bytes = job.encode();
    assert_eq!(PalwGenJobV1::decode_canonical(&bytes), Ok(job.clone()));
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(
        matches!(PalwGenJobV1::decode_canonical(&trailing), Err(PalwGenJobErrorV1::NotCanonical(_))),
        "a trailing byte is refused"
    );
    assert!(matches!(PalwGenJobV1::decode_canonical(&bytes[..bytes.len() - 1]), Err(PalwGenJobErrorV1::NotCanonical(_))));
    // The id covers every field: the envelope, the seed, the body.
    let id = job.id();
    assert_eq!(id, palw_gen_job_id_v1(&job));
    let mut other = job.clone();
    other.seed[0] ^= 1;
    assert_ne!(other.id(), id, "the seed");
    let mut other = job.clone();
    other.envelope.job_nonce[31] ^= 1;
    assert_ne!(other.id(), id, "the nonce");
    let mut other = job.clone();
    if let PalwGenBodyV1::Image(b) = &mut other.body {
        b.image_index = 1;
    }
    assert_ne!(other.id(), id, "the image index");
    // The canonical seed is the first half of the keyed digest of the anchor.
    let anchor = Hash64::from_bytes([0xA1; 64]);
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_GEN_CANONICAL_SEED_DOMAIN_V1).to_state();
    state.update(anchor.as_byte_slice());
    assert_eq!(palw_gen_canonical_seed_v1(&anchor)[..], state.finalize().as_bytes()[..32]);
}

#[test]
fn the_class_accepts_a_job_by_name_and_maps_it_to_the_facts_a_pipeline_runs_over() {
    let f = image();
    let job = image_job(&f, 1, 3, [0x33; 32]);
    let accepted = palw_gen_job_resolve_class_v1(&job, &f.row).expect("the job is the class's");
    assert_eq!(accepted.profile, PalwGenProfileV1::Image);
    assert_eq!(accepted.item_index, 3, "R's position is the image's index");
    assert_eq!(accepted.steps, f.class.offers.steps[1]);
    let PalwGenProfileOffersV1::Image(offers) = &f.class.offers.profile else { unreachable!() };
    let g = offers.guidance.as_ref().unwrap();
    // The scalars in the class's declared order: the guidance value, then the step count's POSITION.
    assert_eq!(accepted.scalars[g.scalar as usize], g.lo as i64);
    assert_eq!(accepted.scalars[offers.steps_scalar.unwrap() as usize], 1);
    assert!(!accepted.public_da, "an image job defaults to PanelDa here");
    let mut public = job.clone();
    public.envelope.privacy_mode = PALW_FP_PRIVACY_PUBLIC_DA;
    assert!(palw_gen_job_resolve_class_v1(&public, &f.row).unwrap().public_da);
}

#[test]
fn every_way_a_job_is_not_the_classes_is_refused_by_name() {
    use PalwGenJobErrorV1 as E;
    let f = image();
    let good = image_job(&f, 0, 0, [0x33; 32]);
    let resolve = |j: &PalwGenJobV1| palw_gen_job_resolve_class_v1(j, &f.row);
    let with = |edit: &dyn Fn(&mut PalwGenJobV1)| {
        let mut j = good.clone();
        edit(&mut j);
        j
    };
    let image_edit = |edit: fn(&mut PalwGenImageBodyV1)| {
        let mut j = good.clone();
        let PalwGenBodyV1::Image(b) = &mut j.body else { unreachable!("an image job") };
        edit(b);
        j
    };
    assert_eq!(resolve(&with(&|j| j.version = 2)), Err(E::Version(2)));
    assert_eq!(resolve(&with(&|j| j.envelope.class_id = Hash64::from_bytes([1; 64]))), Err(E::ClassMismatch));
    assert_eq!(resolve(&with(&|j| j.envelope.privacy_mode = 0)), Err(E::PrivacyModeNotOffered(0)));
    assert_eq!(resolve(&with(&|j| j.envelope.privacy_mode = 3)), Err(E::PrivacyModeNotOffered(3)));
    assert_eq!(resolve(&with(&|j| j.envelope.prompt_mode = 1)), Err(E::PromptModeNotOffered(1)));
    assert_eq!(resolve(&with(&|j| j.seed = [0; 32])), Err(E::SeedRequired), "a class that draws randomness needs a seed");
    assert_eq!(resolve(&vision_job(&vision())), Err(E::ClassMismatch), "another class's job");
    assert!(matches!(resolve(&image_edit(|b| b.steps += 1000)), Err(E::StepsNotOffered { .. })));
    assert_eq!(resolve(&image_edit(|b| b.sampler_id = Hash64::from_bytes([9; 64]))), Err(E::SamplerNotOffered));
    assert!(matches!(resolve(&image_edit(|b| b.guidance_q = u16::MAX)), Err(E::GuidanceOutOfRange { .. })));
    assert!(matches!(resolve(&image_edit(|b| b.width = 9)), Err(E::ResolutionNotOffered { .. })));
    assert!(matches!(resolve(&image_edit(|b| b.output = 2)), Err(E::OutputSpecNotOffered { .. })));
    assert!(matches!(resolve(&image_edit(|b| b.prompt_tokens = 0)), Err(E::EmptyIdsEncoding { .. })), "a count of 0 with a hash");
    assert!(matches!(resolve(&image_edit(|b| b.prompt_token_ids_hash = Hash64::default())), Err(E::EmptyIdsEncoding { .. })));
    assert!(matches!(resolve(&image_edit(|b| b.prompt_tokens = 10_000)), Err(E::PromptTooLong { .. })));
    assert_eq!(
        resolve(&image_edit(|b| {
            b.negative_tokens = 1;
            b.negative_token_ids_hash = Hash64::from_bytes([5; 64]);
        })),
        Err(E::NegativePromptNotOffered),
        "a class that offers no true CFG takes no negative prompt"
    );
    // A body of another profile.
    let embed = vision_job(&vision());
    let mut wrong_body = good.clone();
    wrong_body.body = embed.body.clone();
    assert!(matches!(resolve(&wrong_body), Err(E::BodyKindMismatch { .. })));
    // The ids, held against the job.
    let accepted = resolve(&good).unwrap();
    let ids = prompt();
    assert_eq!(palw_gen_job_ids_admitted_v1(&f.row, &accepted, PalwGenIdsV1 { prompt: &ids, negative: &[] }, FORM), Ok(()));
    assert!(matches!(
        palw_gen_job_ids_admitted_v1(&f.row, &accepted, PalwGenIdsV1 { prompt: &ids[..2], negative: &[] }, FORM),
        Err(E::IdsCountMismatch { .. })
    ));
    let mut forged = ids.clone();
    forged[0] ^= 1;
    assert!(matches!(
        palw_gen_job_ids_admitted_v1(&f.row, &accepted, PalwGenIdsV1 { prompt: &forged, negative: &[] }, FORM),
        Err(E::IdsHashMismatch { .. })
    ));
    // Every id below the bound the class's stages read it under.
    let bound = palw_gen_token_bound_v1(&f.row, TokenSource::Prompt).unwrap();
    let mut high = ids.clone();
    high[1] = bound;
    let mut high_job = good.clone();
    if let PalwGenBodyV1::Image(b) = &mut high_job.body {
        b.prompt_token_ids_hash = prompt_token_ids_commitment_v1(FORM, &high).unwrap();
    }
    let accepted_high = resolve(&high_job).unwrap();
    assert!(matches!(
        palw_gen_job_ids_admitted_v1(&f.row, &accepted_high, PalwGenIdsV1 { prompt: &high, negative: &[] }, FORM),
        Err(E::PromptTokenOutOfRange { .. })
    ));
}

#[test]
fn a_class_that_draws_no_randomness_takes_a_zero_seed_and_only_that() {
    use PalwGenJobErrorV1 as E;
    let f = vision();
    let job = vision_job(&f);
    assert!(palw_gen_job_resolve_class_v1(&job, &f.row).is_ok());
    let mut seeded = job.clone();
    seeded.seed[7] = 1;
    assert_eq!(palw_gen_job_resolve_class_v1(&seeded, &f.row), Err(E::SeedNotUsed), "two job ids for one computation");
    let mut pooled = job.clone();
    if let PalwGenBodyV1::Embedding(b) = &mut pooled.body {
        b.pooling = PALW_GEN_POOLING_MEAN_V1;
    }
    assert!(matches!(palw_gen_job_resolve_class_v1(&pooled, &f.row), Err(E::PoolingNotOffered { .. })));
    let mut wide = job.clone();
    if let PalwGenBodyV1::Embedding(b) = &mut wide.body {
        b.dims = 8;
    }
    assert!(matches!(palw_gen_job_resolve_class_v1(&wide, &f.row), Err(E::DimsNotOffered { .. })));
    let mut text = job.clone();
    if let PalwGenBodyV1::Embedding(b) = &mut text.body {
        b.input = PalwGenEmbeddingInputV1::Text { token_ids_hash: Hash64::from_bytes([1; 64]), tokens: 3 };
    }
    assert!(matches!(palw_gen_job_resolve_class_v1(&text, &f.row), Err(E::InputNotOffered(_))), "this class embeds an image");
    let mut other_image = job.clone();
    if let PalwGenBodyV1::Embedding(b) = &mut other_image.body {
        let PalwGenEmbeddingInputV1::Image(r) = &mut b.input else { unreachable!() };
        r.h += 1;
    }
    assert!(matches!(palw_gen_job_resolve_class_v1(&other_image, &f.row), Err(E::Image(_))), "an image of another size");
}

// ---------------------------------------------------------------------------------------------
// The worker and the claim
// ---------------------------------------------------------------------------------------------

#[test]
fn an_embedding_runs_to_a_canonical_output_the_execution_commits() {
    let f = vision();
    let job = vision_job(&f);
    let (e, binding) = run(&f, &job, no_ids());
    let out = e.output.as_ref().expect("a tensor run commits a canonical output");
    // The output is the output node's elements, as the kind's canonical bytes.
    assert_eq!(out.values.len(), 4);
    assert_eq!(out.values, e.run.output.data.iter().map(|v| *v as i64).collect::<Vec<_>>());
    let spec = &f.class.output;
    assert_eq!(out.root.as_byte_slice(), &misaka_palw_gen::output_root_v1(spec, &out.values, out.tile_len).unwrap()[..]);
    assert_eq!(e.claim.output_root, Some(out.root));
    // The execution root commits the step tree AND the output.
    assert_eq!(binding.execution_root(), binding.committed_execution_root);
    assert_eq!(binding.step_root(), e.claim.step_root);
    let mut other = binding.clone();
    other.output_root = Hash64::from_bytes([1; 64]);
    assert_ne!(other.execution_root(), binding.committed_execution_root, "the output root is part of the execution root");
    // The binding verifies against the class, the job and the registry.
    let outcome = verify_gen_binding_any_v1(
        &binding.clone().into(),
        &f.row,
        &f.row.class_id,
        &binding.committed_execution_root,
        PalwGenJobIdsV1::default(),
    );
    assert!(matches!(outcome, Ok(PalwGenBindingOutcomeV1::Verified(_))), "an honest tensor claim's binding verifies");
    // Replaying the job is the same claim; another job is another one.
    let (_, again) = run(&f, &job, no_ids());
    assert_eq!(again, binding, "the same job, the same claim");
    let mut other_job = job.clone();
    other_job.envelope.job_nonce[0] ^= 1;
    let (_, other_claim) = run(&f, &other_job, no_ids());
    assert_ne!(other_claim.committed_execution_root, binding.committed_execution_root, "the job id is part of the claim");
}

#[test]
fn an_image_job_draws_its_noise_from_its_seed_and_its_index() {
    let f = image();
    let p = prompt();
    let ids = PalwGenIdsV1 { prompt: &p, negative: &[] };
    let (a, _) = run(&f, &image_job(&f, 0, 0, [0x33; 32]), ids);
    let (a2, _) = run(&f, &image_job(&f, 0, 0, [0x33; 32]), ids);
    assert_eq!(a.output.as_ref().unwrap().values, a2.output.as_ref().unwrap().values, "the same job, the same image");
    let (b, _) = run(&f, &image_job(&f, 0, 0, [0x34; 32]), ids);
    let (c, _) = run(&f, &image_job(&f, 0, 1, [0x33; 32]), ids);
    let va = &a.output.as_ref().unwrap().values;
    assert_ne!(va, &b.output.as_ref().unwrap().values, "another seed is another image");
    assert_ne!(va, &c.output.as_ref().unwrap().values, "another image index is another image");
    // Every lane is an RGB8 byte.
    assert!(va.iter().all(|v| (0..=255).contains(v)));
    assert_eq!(va.len(), 2 * 2 * 3);
}

// ---------------------------------------------------------------------------------------------
// The output close
// ---------------------------------------------------------------------------------------------

fn output_tiles(e: &PalwGenExecutionV1) -> u64 {
    let out = e.output.as_ref().unwrap();
    misaka_palw_gen::output::output_tile_count_v1(&out.spec, out.tile_len).unwrap()
}

#[test]
fn an_honest_claim_is_acquitted_at_every_output_tile() {
    for f in [vision(), image()] {
        let p = prompt();
        let (job, ids) = if f.image.is_some() {
            (vision_job(&f), no_ids())
        } else {
            (image_job(&f, 0, 0, [0x33; 32]), PalwGenIdsV1 { prompt: &p, negative: &[] })
        };
        let (e, binding) = run(&f, &job, ids);
        let ev = evidence(&f, &e, &binding, ids);
        let tiles = output_tiles(&e);
        assert!(tiles >= 1);
        for tile in 0..tiles {
            let close = ev.output_close(tile).unwrap_or_else(|er| panic!("tile {tile}: {er}"));
            assert_eq!(check(&f, &close, &binding, None), Ok(None), "tile {tile} of an honest claim");
            // In a session the close must be the leaf the ladder narrowed to.
            let global = e.space.global_index(&close.step_tile.coord).expect("the step tile is a leaf of the tree");
            assert_eq!(check(&f, &close, &binding, Some(global)), Ok(None));
            assert_eq!(
                check(&f, &close, &binding, Some(global + 1)),
                Err(PalwGenCloseErrorV1::NotTheNarrowedLeaf { opened: global, narrowed: global + 1 }),
                "an output close names the leaf it is"
            );
        }
        // One tile past the output is no tile.
        assert!(ev.output_close(tiles).is_err());
        // It rides the court's close enum at tag 13, below every other generative close in the byte order.
        let close = ev.output_close(0).unwrap();
        let proof = PalwCourtVerdictProofV2::GenOutputTile { close: Box::new(close) };
        assert!(proof.is_gen_v1() && !proof.is_tir_v1());
        assert_eq!(borsh::to_vec(&proof).unwrap()[0], 13);
        let back: PalwCourtVerdictProofV2 = borsh::from_slice(&borsh::to_vec(&proof).unwrap()).unwrap();
        assert_eq!(back, proof);
        let object = kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::CourtClosed {
            session_id: Hash64::from_bytes([1; 64]),
            verdict: kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty,
            proof,
        };
        assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_gen_v1(&object), "dropped by name below palw_gen_v1");
    }
}

#[test]
fn a_planted_digest_fault_is_convicted_at_its_lane_and_only_there() {
    for f in [vision(), image()] {
        let p = prompt();
        let (job, ids) = if f.image.is_some() {
            (vision_job(&f), no_ids())
        } else {
            (image_job(&f, 0, 0, [0x33; 32]), PalwGenIdsV1 { prompt: &p, negative: &[] })
        };
        let (honest, _) = run(&f, &job, ids);
        let tile_len = honest.output.as_ref().unwrap().tile_len as usize;
        let lanes = honest.output.as_ref().unwrap().values.len();
        for lane in [0usize, lanes / 2, lanes - 1] {
            let lied = lie_about_the_output(&honest, lane);
            let accused = PalwGenTensorBindingV1::of(&job, &lied.claim, lied.space.leaf_count(), lied.claim.output_root.unwrap());
            assert_ne!(accused.committed_execution_root, run(&f, &job, ids).1.committed_execution_root, "a lie is another claim");
            let ev = evidence(&f, &lied, &accused, ids);
            let bad_tile = (lane / tile_len) as u64;
            for tile in 0..output_tiles(&lied) {
                let close = ev.output_close(tile).unwrap();
                let verdict = check(&f, &close, &accused, None);
                if tile == bad_tile {
                    assert_eq!(
                        verdict,
                        Ok(Some(PalwStepFaultV1::TirOutputDigestMismatch { value_index: (lane % tile_len) as u32 })),
                        "the lane {lane}'s tile convicts at the lane"
                    );
                } else {
                    assert_eq!(verdict, Ok(None), "tile {tile} is honest");
                }
            }
        }
    }
}

#[test]
fn a_committed_lane_outside_the_nodes_proven_interval_is_convicted() {
    // A committed lane one past the output node's proven interval is a lane no honest run produces
    // (PALW-TIR-33): the output close convicts it at its lane, whatever the canonical bytes say.
    let mut planted = 0;
    for f in [vision(), image()] {
        let p = prompt();
        let (job, ids) = if f.image.is_some() {
            (vision_job(&f), no_ids())
        } else {
            (image_job(&f, 0, 0, [0x33; 32]), PalwGenIdsV1 { prompt: &p, negative: &[] })
        };
        let (honest, _) = run(&f, &job, ids);
        let out_stage = f.pipeline.output_stage as usize;
        let program = &f.programs[f.pipeline.stages[out_stage].program as usize];
        let interval = misaka_palw_tir::interval_v2::output_interval_v2(program).expect("the output node's proven interval");
        let tile_len = honest.output.as_ref().unwrap().tile_len;
        let coord = palw_gen_output_step_coord_v1(&honest.space.stages[out_stage], 0, tile_len).expect("tile 0 has a step tile");
        let leaf_index = honest.space.stages[out_stage].leaf_index(&coord).unwrap() as usize;
        let dtype = honest.space.stages[out_stage].leaves()[leaf_index].dtype;
        for (lane, value) in [(1usize, interval.hi + 1), (2usize, interval.lo - 1)] {
            if !dtype.contains(value) {
                continue;
            }
            let lied = lie_in_the_step_tile(&honest, &coord, lane, value);
            let accused = PalwGenTensorBindingV1::of(&job, &lied.claim, lied.space.leaf_count(), lied.claim.output_root.unwrap());
            let ev = evidence(&f, &lied, &accused, ids);
            let close = ev.output_close(0).unwrap();
            assert_eq!(
                check(&f, &close, &accused, None),
                Ok(Some(PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: lane as u32 })),
                "lane {lane} at {value} is outside [{}, {}]",
                interval.lo,
                interval.hi
            );
            planted += 1;
        }
    }
    assert!(planted >= 1, "at least one toy node's proven interval is narrower than its dtype");
}

#[test]
fn evidence_that_does_not_hold_convicts_nobody() {
    let f = vision();
    let job = vision_job(&f);
    let (e, binding) = run(&f, &job, no_ids());
    let ev = evidence(&f, &e, &binding, no_ids());
    let close = ev.output_close(0).unwrap();
    assert_eq!(check(&f, &close, &binding, None), Ok(None));

    // An output tile that is not under the claim's root.
    let mut forged_tile = close.clone();
    forged_tile.output_tile[0] ^= 0xFF;
    assert_eq!(
        check(&f, &forged_tile, &binding, None),
        Err(PalwGenCloseErrorV1::OutputRefused(kaspa_consensus_core::palw_gen_court_v1::PalwGenOutputRefusalV1::TileNotProven))
    );
    // A tile number that is not the one the bytes are proven at.
    let mut other_index = close.clone();
    other_index.tile += 1;
    assert!(check(&f, &other_index, &binding, None).is_err());

    // A step tile whose values are not under its stage's root.
    let mut forged_step = close.clone();
    forged_step.step_tile.lanes_le[0] ^= 1;
    assert_eq!(
        check(&f, &forged_step, &binding, None),
        Err(PalwGenCloseErrorV1::Refused(kaspa_consensus_core::palw_gen_court_v1::PalwGenCloseRefusalV1::LeafNotProven(
            close.step_tile.coord
        )))
    );

    // A step tile that is not the output node's for this output tile.
    let mut elsewhere = close.clone();
    elsewhere.step_tile.coord.pos += 1;
    assert_eq!(check(&f, &elsewhere, &binding, None), Err(PalwGenCloseErrorV1::NotTheOutputStepTile { tile: 0 }));

    // A binding that is not the claim's.
    let mut other_binding = close.clone();
    other_binding.binding.output_root = Hash64::from_bytes([3; 64]);
    assert!(matches!(check(&f, &other_binding, &binding, None), Err(PalwGenCloseErrorV1::NotTheClaims(_))));

    // An unsupported version.
    let mut old = close.clone();
    old.version = 9;
    assert_eq!(check(&f, &old, &binding, None), Err(PalwGenCloseErrorV1::Version(9)));
}

// ---------------------------------------------------------------------------------------------
// Finding G21: a lane outside its dtype is hashed as committed, so it opens, proves and is convicted
// ---------------------------------------------------------------------------------------------

fn coord_of(dtype: DType) -> PalwGenLeafV1 {
    PalwGenLeafV1 {
        coord: PalwGenLeafCoordV1 { stage: 0, pos: 3, kind: PalwGenLeafKindV1::Commit { occurrence: 1, node: 2 }, tile: 0 },
        dtype,
        first_element: 0,
        value_count: 3,
    }
}

#[test]
fn the_leaf_hash_is_total_over_four_byte_lanes_and_unchanged_inside_the_dtype() {
    // Inside the dtype the wire encoding is the strict one, byte for byte: honest leaves, every root and
    // every golden are what they were.
    let inside: [(DType, [i128; 3]); 4] = [
        (DType::I8, [-128, 0, 127]),
        (DType::I16, [-32_768, 1, 32_767]),
        (DType::I32, [i32::MIN as i128, -1, i32::MAX as i128]),
        (DType::Idx, [0, 1, u32::MAX as i128]),
    ];
    for (dtype, values) in inside {
        assert_eq!(palw_tir_lanes_wire_v1(dtype, &values).unwrap(), palw_tir_lanes_le_v1(dtype, &values).unwrap(), "{dtype:?}");
    }
    // Outside it the strict encoding refuses and the wire encoding is the lane as committed: the low 32 bits.
    let outside: [(DType, [i128; 3]); 4] = [
        (DType::I8, [128, -129, 100_000]),
        (DType::I16, [32_768, -32_769, 100_000]),
        (DType::I32, [1 << 31, -(1 << 31) - 1, 1 << 40]),
        (DType::Idx, [-1, 1 << 32, 1 << 40]),
    ];
    for (dtype, values) in outside {
        assert!(palw_tir_lanes_le_v1(dtype, &values).is_err(), "{dtype:?}: the strict builder refuses");
        let lanes = palw_tir_lanes_wire_v1(dtype, &values).unwrap_or_else(|e| panic!("{dtype:?}: {e}"));
        let want: Vec<u8> = values
            .iter()
            .flat_map(|v| if dtype == DType::Idx { (*v as u32).to_le_bytes() } else { (*v as i32).to_le_bytes() })
            .collect();
        assert_eq!(lanes, want, "{dtype:?}: four little-endian bytes, the value's low 32 bits");
        // The leaf's hash is over exactly those lanes (no error, no dtype judgement) ...
        let leaf = coord_of(dtype);
        let hash = palw_gen_step_leaf_hash_v1(&leaf, &values).unwrap_or_else(|e| panic!("{dtype:?}: {e}"));
        // ... and equals the hash of the values a court reads back from those lanes (the wire round trip).
        let read = palw_tir_lane_values_v1(dtype, &lanes).unwrap();
        assert_eq!(palw_gen_step_leaf_hash_v1(&leaf, &read).unwrap(), hash, "{dtype:?}: the opened leaf hashes as committed");
    }
    // Never a dtype that is not committed.
    assert!(palw_tir_lanes_wire_v1(DType::I64, &[0]).is_err());
}

/// Values a forger plants in the output node's step tile: each past its dtype's range, in the range of
/// a lane's four bytes or beyond it.
fn forged_lanes(dtype: DType) -> Vec<i128> {
    let (lo, hi) = match dtype {
        DType::I8 => (-128i128, 127i128),
        DType::I16 => (-32_768, 32_767),
        DType::I32 => (i32::MIN as i128, i32::MAX as i128),
        DType::Idx => (0, u32::MAX as i128),
        other => panic!("{other:?} is never committed"),
    };
    vec![hi + 1, lo - 1, 100_000, -100_000]
}

#[test]
fn an_executor_that_commits_a_lane_outside_its_dtype_is_convicted_at_the_output_close() {
    // E's 311 forged closes: a lane outside the output node's dtype, committed into a tree the executor
    // hashed itself. It opens, proves under its stage's root and is convicted by PALW-TIR-33.
    let mut convicted = 0;
    for f in [vision(), image()] {
        let p = prompt();
        let (job, ids) = if f.image.is_some() {
            (vision_job(&f), no_ids())
        } else {
            (image_job(&f, 0, 0, [0x33; 32]), PalwGenIdsV1 { prompt: &p, negative: &[] })
        };
        let (honest, honest_binding) = run(&f, &job, ids);
        let out_stage = f.pipeline.output_stage as usize;
        let program = &f.programs[f.pipeline.stages[out_stage].program as usize];
        let interval = misaka_palw_tir::interval_v2::output_interval_v2(program).expect("the output node's proven interval");
        let tile_len = honest.output.as_ref().unwrap().tile_len;
        let coord = palw_gen_output_step_coord_v1(&honest.space.stages[out_stage], 0, tile_len).expect("tile 0 has a step tile");
        let leaf_index = honest.space.stages[out_stage].leaf_index(&coord).unwrap() as usize;
        let dtype = honest.space.stages[out_stage].leaves()[leaf_index].dtype;
        for value in forged_lanes(dtype) {
            // What a court reads back from the lane: the value's low 32 bits.
            let read = if dtype == DType::Idx { value as u32 as i128 } else { value as i32 as i128 };
            if read >= interval.lo && read <= interval.hi {
                continue; // the wrapped lane is a value the node may hold: not a G21 case
            }
            let lied = lie_in_the_step_tile(&honest, &coord, 1, value);
            assert_ne!(lied.claim.step_root, honest.claim.step_root, "the forged tree has its own root");
            let accused = PalwGenTensorBindingV1::of(&job, &lied.claim, lied.space.leaf_count(), lied.claim.output_root.unwrap());
            let ev = evidence(&f, &lied, &accused, ids);
            let close = ev.output_close(0).unwrap_or_else(|e| panic!("{dtype:?} {value}: the forged tile does not ride: {e}"));
            assert_eq!(
                check(&f, &close, &accused, None),
                Ok(Some(PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 1 })),
                "{dtype:?}: a lane of {value} (read as {read}) is outside [{}, {}] and convicts",
                interval.lo,
                interval.hi
            );
            // In a session the same close is the narrowed leaf's.
            let global = lied.space.global_index(&close.step_tile.coord).unwrap();
            assert_eq!(check(&f, &close, &accused, Some(global)), Ok(Some(PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 1 })));
            convicted += 1;
        }
        // The honest claim is what it was: the same root, acquitted at every tile.
        let ev = evidence(&f, &honest, &honest_binding, ids);
        for tile in 0..output_tiles(&honest) {
            assert_eq!(check(&f, &ev.output_close(tile).unwrap(), &honest_binding, None), Ok(None));
        }
    }
    assert!(convicted >= 4, "at least one toy node's dtype is narrower than a four-byte lane: {convicted} forged closes convicted");
}

#[test]
fn a_cone_close_at_a_leaf_holding_a_lane_outside_its_dtype_convicts_on_the_lane_alone() {
    // The challenger's other move: dispute the leaf itself. Its cone is never read — PALW-TIR-33 convicts
    // before any operand, param or image is — so the close is the leaf and nothing else.
    let f = image();
    let p = prompt();
    let ids = PalwGenIdsV1 { prompt: &p, negative: &[] };
    let job = image_job(&f, 0, 0, [0x33; 32]);
    let (honest, _) = run(&f, &job, ids);
    let out_stage = f.pipeline.output_stage as usize;
    let tile_len = honest.output.as_ref().unwrap().tile_len;
    let coord = palw_gen_output_step_coord_v1(&honest.space.stages[out_stage], 0, tile_len).unwrap();
    let lied = lie_in_the_step_tile(&honest, &coord, 0, 100_000);
    let accused = PalwGenTensorBindingV1::of(&job, &lied.claim, lied.space.leaf_count(), lied.claim.output_root.unwrap());
    let ev = evidence(&f, &lied, &accused, ids);
    let global = lied.space.global_index(&coord).unwrap();
    let close = ev.cone_close(global, &LIMITS).expect("a lane outside the interval is a close, not a refusal to build one");
    assert!(close.operands.is_empty() && close.params.is_empty() && close.image_tiles.is_empty(), "the leaf alone");
    let verdict = check_gen_cone_close_v1(&close, &f.row, &f.row.class_id, &accused.committed_execution_root, Some(global), FORM, &LIMITS);
    assert_eq!(verdict, Ok(Some(PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 0 })));

    // An honest leaf still gets the full cone close and is acquitted (unchanged).
    let (_, honest_binding) = run(&f, &job, ids);
    let honest_ev = evidence(&f, &honest, &honest_binding, ids);
    let full = honest_ev.cone_close(global, &LIMITS).expect("the honest cone close builds");
    assert!(!full.operands.is_empty() || !full.params.is_empty(), "an honest leaf's close carries what its cone reads");
    assert_eq!(
        check_gen_cone_close_v1(&full, &f.row, &f.row.class_id, &honest_binding.committed_execution_root, Some(global), FORM, &LIMITS),
        Ok(None)
    );

    // A leaf that does not prove under its stage's root convicts nobody, whatever its lanes.
    let mut forged = close.clone();
    forged.disputed.lanes_le[0] ^= 1;
    assert!(
        matches!(
            check_gen_cone_close_v1(&forged, &f.row, &f.row.class_id, &accused.committed_execution_root, Some(global), FORM, &LIMITS),
            Err(_)
        ),
        "an unproven leaf is a refusal, not a conviction"
    );
}
