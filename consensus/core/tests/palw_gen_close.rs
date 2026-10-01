//! **RFC-0003: the generative court's consensus objects** — a pipeline claim's binding and the two
//! closes a `CourtClosed` carries for one (`GenCone`, tag 10; `GenDecodeToken`, tag 11), checked
//! against the class the chain registered (`gen_classes`), the claim's execution root and the leaf
//! the ladder narrowed to.
//!
//! The claim is the golden toy VLM (`consensus-vectors/tir-v2/pipelines/toy-vlm.json`) run on an FP
//! Job V5 — an FP Job V4 vector retargeted at the class, with the vector's image.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2;
use kaspa_consensus_core::palw_fp_job_v5::PalwFreePromptJobV5;
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_V4_VERSION, PalwFreePromptJobV3};
use kaspa_consensus_core::palw_gen_artifact_v1::{palw_gen_inventory_root_v1, palw_gen_open_leaves_v1};
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_close_v1::*;
use kaspa_consensus_core::palw_gen_court_v1::PalwGenImageTileV1;
use kaspa_consensus_core::palw_gen_step_v1::{PalwGenLeafKindV1, palw_gen_stage_root_v1, palw_gen_step_leaf_hash_v1};
use kaspa_consensus_core::palw_gen_v1::PalwGenProfileV1;
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenDecodeV1, PalwGenExecutionV1, palw_gen_execute_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{JobImageV1, PipelineJob, PipelineParams, TirPipelineV1};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::tensor::Tensor;

const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };
const PLACEHOLDER: u32 = 15;
const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

struct Params(Vec<MapParams>);
impl PipelineParams for Params {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.0[program as usize]
    }
}

/// The toy VLM as a registered Text class: its class, its row, its decoded programs, its params, and
/// the vector's image.
struct Fixture {
    class: PalwGenClassV1,
    row: PalwGenClassRecordV1,
    pipeline: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    params: Params,
    image: JobImageV1,
}

fn fixture() -> Fixture {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines/toy-vlm.json");
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
    let img = &v["job"]["images"][0];
    let image = JobImageV1 {
        h: img["h"].as_u64().unwrap() as u32,
        w: img["w"].as_u64().unwrap() as u32,
        rgb: unhex(img["rgb_hex"].as_str().unwrap()),
    };
    let class = PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        pipeline: pipeline_bytes,
        programs: program_bytes,
        layouts,
        output: OutputSpecV1::tokens(pipeline.stages[pipeline.output_stage as usize].max_trip),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: 8,
            max_negative_tokens: 0,
            images: vec![PalwGenImageOfferV1 { h: image.h, w: image.w, tile_len: 4, token_equivalents: 1_000_000 }],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
        },
        tokenizer_id: Hash64::from_bytes([0x72; 64]),
    };
    let (root, _) = palw_gen_inventory_root_v1(&programs, &params).unwrap();
    let row = palw_gen_class_record_v1(&class, &root).unwrap();
    Fixture { class, row, pipeline, programs, params, image }
}

/// The prompt: text, two image placeholders, text.
fn prompt() -> Vec<u32> {
    vec![3, PLACEHOLDER, PLACEHOLDER, 5]
}

/// An FP Job V4 of the golden vectors, retargeted at the class: its id, its tokenizer, the prompt,
/// greedy with no controls, a four-id budget, and the image's reference.
fn v5_job(f: &Fixture) -> PalwFreePromptJobV5 {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/fp-v4/job_v4_encoding.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut v4: PalwFreePromptJobV3 = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| borsh::from_slice::<PalwFreePromptJobV3>(&unhex(c["borsh_hex"].as_str().unwrap())).unwrap())
        .find(|j| j.version == PALW_FP_V4_VERSION)
        .unwrap();
    v4.class_id = f.row.class_id;
    v4.tokenizer_id = f.class.tokenizer_id;
    v4.prompt_tokens = prompt().len() as u32;
    v4.prompt_token_ids_hash = prompt_token_ids_commitment_v1(FORM, &prompt()).unwrap();
    v4.decode_token_limit = 4;
    v4.decode = Some(DecodeConfigV4::NOOP);
    v4.temperature_q = PalwDecodeSamplingV2::GREEDY.temperature_q;
    let input_root = misaka_palw_gen::output::input_image_root_v1(f.image.h, f.image.w, 4, &f.image.rgb).unwrap();
    let images = vec![PalwGenImageInputRefV1 { input_root: Hash64::from_bytes(input_root), h: f.image.h, w: f.image.w }];
    PalwFreePromptJobV5 { v4, images, source: None }
}

/// The worker's run of the job, and its binding.
fn execute(f: &Fixture, job: &PalwFreePromptJobV5) -> (PalwGenExecutionV1, PalwGenStepBindingV1) {
    let run_job = PipelineJob { prompt: prompt(), images: vec![f.image.clone()], ..PipelineJob::default() };
    let decode = PalwGenDecodeV1::of(job).unwrap();
    let e =
        palw_gen_execute_v1(&f.pipeline, &f.programs, &f.class.layouts, &f.params, &run_job, &decode, job.v4.sampling_seed).unwrap();
    let binding = PalwGenStepBindingV1::of(job, &e.claim, e.space.leaf_count());
    (e, binding)
}

/// An execution whose one leaf is changed, its roots recomputed over it: the executor's lie,
/// committed consistently.
fn lie(e: &PalwGenExecutionV1, stage: usize, index: usize, delta: i128) -> PalwGenExecutionV1 {
    let mut l = e.clone();
    l.leaf_values[stage][index][0] += delta;
    let leaf = l.space.stages[stage].leaves()[index];
    l.leaf_hashes[stage][index] = palw_gen_step_leaf_hash_v1(&leaf, &l.leaf_values[stage][index]).unwrap();
    l.claim.stage_roots[stage] = palw_gen_stage_root_v1(stage as u8, &l.leaf_hashes[stage]);
    l.claim.step_root = kaspa_consensus_core::palw_gen_step_v1::palw_gen_step_root_v1(&l.claim.stage_roots);
    l
}

/// The cone close of leaf `(stage, index)`: every leaf before it, every image tile, every param leaf,
/// and the prompt where the stage reads it.
fn cone(f: &Fixture, e: &PalwGenExecutionV1, binding: &PalwGenStepBindingV1, stage: usize, index: usize) -> PalwGenConeCloseV1 {
    let wire = |s: usize, i: usize| PalwGenLeafOpeningV1::of(&e.space, &e.open(s as u8, i as u64).unwrap()).unwrap();
    let mut operands = Vec::new();
    for s in 0..=stage {
        let n = if s == stage { index } else { e.space.stages[s].leaves().len() };
        operands.extend((0..n).map(|i| wire(s, i)));
    }
    let tiles = misaka_palw_gen::output::input_image_tiles_v1(f.image.h, f.image.w, 4, &f.image.rgb).unwrap();
    let image_tiles = tiles
        .into_iter()
        .enumerate()
        .map(|(t, (bytes, proof))| PalwGenImageTileV1 { image: 0, tile: t as u64, bytes, proof })
        .collect();
    let count = kaspa_consensus_core::palw_gen_artifact_v1::PalwGenInventoryIndexV1::new(&f.programs).unwrap().leaf_count();
    let params = palw_gen_open_leaves_v1(&f.programs, &f.params, 0..count).unwrap();
    let reads = palw_gen_stage_reads_prompt_v1(&f.pipeline, stage);
    PalwGenConeCloseV1 {
        version: PALW_GEN_CLOSE_VERSION_V1,
        binding: binding.clone(),
        prompt_ids: if reads { prompt() } else { vec![] },
        disputed: wire(stage, index),
        operands,
        image_tiles,
        params,
        source_ids: vec![],
    }
}

fn check(f: &Fixture, close: &PalwGenConeCloseV1, claim_root: &Hash64, narrowed: Option<u64>) -> PalwGenCloseOutcomeV1 {
    check_gen_cone_close_v1(close, &f.row, &f.row.class_id, claim_root, narrowed, FORM, &LIMITS)
}

#[test]
fn the_two_closes_are_appended_after_every_court_proof() {
    let f = fixture();
    let job = v5_job(&f);
    let (e, binding) = execute(&f, &job);
    let close = cone(&f, &e, &binding, 0, 0);
    let proof = PalwCourtVerdictProofV2::GenCone { close: Box::new(close.clone()) };
    assert_eq!(borsh::to_vec(&proof).unwrap()[0], 10, "tag 10, Phase F's allocation after TirDissection's 9");
    let decode = PalwGenDecodeCloseV1 { version: PALW_GEN_CLOSE_VERSION_V1, binding: binding.clone(), t: 0, row: vec![] };
    let proof11 = PalwCourtVerdictProofV2::GenDecodeToken { close: Box::new(decode) };
    assert_eq!(borsh::to_vec(&proof11).unwrap()[0], 11);
    for p in [&proof, &proof11] {
        assert!(p.is_gen_v1() && !p.is_tir_v1());
        let back: PalwCourtVerdictProofV2 = borsh::from_slice(&borsh::to_vec(p).unwrap()).unwrap();
        assert_eq!(&back, p, "round trip");
        let object = PalwConsensusObjectV2::CourtClosed {
            session_id: Hash64::from_bytes([1; 64]),
            verdict: kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty,
            proof: p.clone(),
        };
        assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_gen_v1(&object), "dropped by name below palw_gen_v1");
    }
}

#[test]
fn the_binding_is_the_claims_execution_root() {
    let f = fixture();
    let job = v5_job(&f);
    let (e, binding) = execute(&f, &job);
    assert_eq!(binding.execution_root(), binding.committed_execution_root);
    assert_eq!(binding.step_root(), e.claim.step_root);
    let moved = |edit: &dyn Fn(&mut PalwGenStepBindingV1)| {
        let mut b = binding.clone();
        edit(&mut b);
        b.execution_root()
    };
    let root = binding.committed_execution_root;
    assert_ne!(moved(&|b| b.job.images[0].input_root = Hash64::from_bytes([9; 64])), root, "the images (in the job id)");
    assert_ne!(moved(&|b| b.stage_roots[0] = Hash64::from_bytes([9; 64])), root, "a stage root");
    assert_ne!(moved(&|b| b.generated.push(1)), root, "the answer");
    assert_ne!(moved(&|b| b.step_leaf_count += 1), root, "the leaf count");
    // Another claim's root: nothing is read.
    let close = cone(&f, &e, &binding, 0, 0);
    assert!(matches!(check(&f, &close, &Hash64::from_bytes([3; 64]), None), Err(PalwGenCloseErrorV1::NotTheClaims(_))));
    // A leaf count that is not the job's canonical one convicts from the binding alone.
    let mut lying = binding.clone();
    lying.step_leaf_count += 1;
    lying.committed_execution_root = lying.execution_root();
    let mut close = cone(&f, &e, &lying, 0, 0);
    close.binding = lying.clone();
    assert_eq!(check(&f, &close, &lying.committed_execution_root, None), Ok(Some(PalwStepFaultV1::StepLeafCountNotCanonical)));
    // A job the class does not take: another image size.
    let mut other = binding.clone();
    other.job.images[0].w += 1;
    other.committed_execution_root = other.execution_root();
    let mut close = cone(&f, &e, &other, 0, 0);
    close.binding = other.clone();
    assert!(matches!(check(&f, &close, &other.committed_execution_root, None), Err(PalwGenCloseErrorV1::Binding(_))));
}

#[test]
fn every_honest_leaf_is_acquitted_at_its_index_and_a_lie_is_convicted() {
    let f = fixture();
    let job = v5_job(&f);
    let (e, binding) = execute(&f, &job);
    let root = binding.committed_execution_root;
    let mut global = 0u64;
    for s in 0..e.space.stages.len() {
        for i in 0..e.space.stages[s].leaves().len() {
            let close = cone(&f, &e, &binding, s, i);
            assert_eq!(check(&f, &close, &root, Some(global)), Ok(None), "stage {s} leaf {i}");
            global += 1;
        }
    }
    assert_eq!(global, binding.step_leaf_count);
    // The ladder's index is the claim's one stage-major order.
    let close = cone(&f, &e, &binding, 1, 0);
    let first_text = e.space.stages[0].leaves().len() as u64;
    assert_eq!(check(&f, &close, &root, Some(first_text)), Ok(None));
    assert_eq!(
        check(&f, &close, &root, Some(first_text + 1)),
        Err(PalwGenCloseErrorV1::NotTheNarrowedLeaf { opened: first_text, narrowed: first_text + 1 })
    );
    // A lie in either stage, committed consistently, is convicted at its leaf.
    for (stage, index) in [(0usize, 0usize), (1, 3)] {
        let lied = lie(&e, stage, index, 1);
        let lying = PalwGenStepBindingV1::of(&job, &lied.claim, lied.space.leaf_count());
        let close = cone(&f, &lied, &lying, stage, index);
        let verdict = check(&f, &close, &lying.committed_execution_root, None);
        assert!(matches!(verdict, Ok(Some(_))), "stage {stage} leaf {index}: {verdict:?}");
    }
}

#[test]
fn the_prompt_rides_exactly_when_the_disputed_stage_reads_it() {
    let f = fixture();
    let job = v5_job(&f);
    let (e, binding) = execute(&f, &job);
    let root = binding.committed_execution_root;
    assert!(!palw_gen_stage_reads_prompt_v1(&f.pipeline, 0), "the vision stage reads no prompt");
    assert!(palw_gen_stage_reads_prompt_v1(&f.pipeline, 1), "the text stage's stream starts with it");
    let text = cone(&f, &e, &binding, 1, 0);
    let mut bare = text.clone();
    bare.prompt_ids.clear();
    assert_eq!(check(&f, &bare, &root, None), Err(PalwGenCloseErrorV1::PromptNotCarried));
    let mut other = text.clone();
    other.prompt_ids[0] += 1;
    assert_eq!(check(&f, &other, &root, None), Err(PalwGenCloseErrorV1::PromptNotTheJobs), "not the job's prompt");
    let mut revealed = cone(&f, &e, &binding, 0, 0);
    revealed.prompt_ids = prompt();
    assert_eq!(check(&f, &revealed, &root, None), Err(PalwGenCloseErrorV1::PromptNotTheJobs), "a vision dispute reveals nothing");
}

#[test]
fn the_decode_close_holds_each_id_to_its_committed_row() {
    let f = fixture();
    let job = v5_job(&f);
    let (e, binding) = execute(&f, &job);
    let root = binding.committed_execution_root;
    let text = 1usize;
    let prog = &f.programs[f.pipeline.stages[text].program as usize];
    let post = (prog.occurrences().len() - 1) as u16;
    let kind = PalwGenLeafKindV1::Commit { occurrence: post, node: prog.output.node() };
    let row_of = |e: &PalwGenExecutionV1, t: u32| -> (Vec<PalwGenLeafOpeningV1>, u64) {
        let pos = prompt().len() as u32 - 1 + t;
        let leaves = e.space.stages[text].leaves();
        let picks: Vec<usize> = (0..leaves.len()).filter(|i| leaves[*i].coord.pos == pos && leaves[*i].coord.kind == kind).collect();
        let row = picks.iter().map(|i| PalwGenLeafOpeningV1::of(&e.space, &e.open(text as u8, *i as u64).unwrap()).unwrap()).collect();
        (row, e.space.global_index(&leaves[picks[0]].coord).unwrap())
    };
    for t in 0..e.claim.generated.len() as u32 {
        let (row, head) = row_of(&e, t);
        let close = PalwGenDecodeCloseV1 { version: PALW_GEN_CLOSE_VERSION_V1, binding: binding.clone(), t, row };
        assert_eq!(check_gen_decode_close_v1(&close, &f.row, &f.row.class_id, &root, Some(head)), Ok(None), "id {t}");
        assert!(matches!(
            check_gen_decode_close_v1(&close, &f.row, &f.row.class_id, &root, Some(head + 1)),
            Err(PalwGenCloseErrorV1::NotTheNarrowedLeaf { .. }) | Ok(None)
        ));
    }
    // An executor that commits another id than the decode selects from its own row is convicted.
    let mut lying = binding.clone();
    lying.generated[0] = lying.generated[0].wrapping_add(1) % 16;
    lying.committed_execution_root = lying.execution_root();
    let (row, _) = row_of(&e, 0);
    let close = PalwGenDecodeCloseV1 { version: PALW_GEN_CLOSE_VERSION_V1, binding: lying.clone(), t: 0, row };
    let verdict = check_gen_decode_close_v1(&close, &f.row, &f.row.class_id, &lying.committed_execution_root, None);
    assert_eq!(verdict, Ok(Some(PalwStepFaultV1::DecodeTokenMismatch { position: 0 })));
}
