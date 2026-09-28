//! **RFC-0003 §II.2.1: the node's generative worker, seat and court halves** (`gen_worker`), on the
//! golden toy VLM and on tir-lower's lowered tiny LLaVA (`rfc3/lower` 4c25416a5).

use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2;
use kaspa_consensus_core::palw_fp_job_v5::PalwFreePromptJobV5;
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_V4_VERSION, PalwFreePromptJobV3};
use kaspa_consensus_core::palw_gen_artifact_v1::palw_gen_inventory_root_v1;
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_close_v1::*;
use kaspa_consensus_core::palw_gen_v1::PalwGenProfileV1;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_dissect_v1::{PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1};
use kaspa_hashes::Hash64;
use misaka_palw_base0::gen_worker::*;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{JobImageV1, PipelineParams, TirPipelineV1};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::tensor::Tensor;

const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 22, max_terms: 1 << 26 };
const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

#[derive(Clone)]
struct Params(Vec<MapParams>);
impl PipelineParams for Params {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.0[program as usize]
    }
}

/// A golden pipeline vector as a registered Text class with one image slot: the row, this node's
/// weights, the job's image and prompt.
struct Vector {
    row: PalwGenClassRecordV1,
    params: Params,
    image: JobImageV1,
    prompt: Vec<u32>,
}

fn vector(name: &str, tile: u32, h_tile: u32, checkpoint: u32, image_tile: u32, prompt: Option<Vec<u32>>) -> Vector {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v2/pipelines").join(name);
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
                checkpoint_interval: checkpoint,
                h_tile,
                commit_tiles: vec![tile; commits],
                state_tiles: vec![tile; p.states.len()],
            }
        })
        .collect();
    let img = &v["job"]["images"][0];
    let image = JobImageV1 {
        h: img["h"].as_u64().unwrap() as u32,
        w: img["w"].as_u64().unwrap() as u32,
        rgb: unhex(img["rgb_hex"].as_str().unwrap()),
    };
    let prompt = prompt.unwrap_or_else(|| v["job"]["prompt"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as u32).collect());
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
            max_prompt_tokens: 8.max(prompt.len() as u32),
            max_negative_tokens: 0,
            images: vec![PalwGenImageOfferV1 { h: image.h, w: image.w, tile_len: image_tile, token_equivalents: 1_000_000 }],
        },
        tokenizer_id: Hash64::from_bytes([0x74; 64]),
    };
    let (root, _) = palw_gen_inventory_root_v1(&programs, &params).unwrap();
    let row = palw_gen_class_record_v1(&class, &root).unwrap();
    Vector { row, params, image, prompt }
}

/// An FP Job V4 of the golden vectors, retargeted at the class: its id, tokenizer, prompt, a greedy
/// budget of `budget` and the image's reference.
fn v5_job(v: &Vector, budget: u32) -> PalwFreePromptJobV5 {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/fp-v4/job_v4_encoding.json");
    let j: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut v4: PalwFreePromptJobV3 = j["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| borsh::from_slice::<PalwFreePromptJobV3>(&unhex(c["borsh_hex"].as_str().unwrap())).unwrap())
        .find(|job| job.version == PALW_FP_V4_VERSION)
        .unwrap();
    v4.class_id = v.row.class_id;
    v4.tokenizer_id = v.row.tokenizer_id;
    v4.prompt_tokens = v.prompt.len() as u32;
    v4.prompt_token_ids_hash = prompt_token_ids_commitment_v1(FORM, &v.prompt).unwrap();
    v4.decode_token_limit = budget;
    v4.decode = Some(DecodeConfigV4::NOOP);
    v4.temperature_q = PalwDecodeSamplingV2::GREEDY.temperature_q;
    let slot = &v.row.class.offers.images[0];
    PalwFreePromptJobV5 { v4, images: vec![palw_gen_image_input_ref_v1(&v.image, slot.tile_len).unwrap()] }
}

fn toy() -> Vector {
    vector("toy-vlm.json", 4, 16, 1, 4, Some(vec![3, 15, 15, 5]))
}

fn llava() -> Vector {
    vector("vlm-llava-tiny.json", 16, 8, 4, 64, None)
}

#[test]
fn a_held_class_refuses_weights_that_are_not_its_root() {
    let v = toy();
    assert!(GenHeldClassV1::hold(v.row.clone(), v.params.clone()).is_ok());
    let mut other = v.params.clone();
    let t = other.0[0].tensors.values_mut().next().unwrap();
    t.data[0] = if t.data[0] > 0 { t.data[0] - 1 } else { t.data[0] + 1 };
    let refused = GenHeldClassV1::hold(v.row.clone(), other).err().expect("refused");
    assert!(refused.contains("artifact root"), "{refused}");
}

#[test]
fn the_worker_answers_a_request_frame_with_the_claims_binding() {
    let v = toy();
    let held = GenHeldClassV1::hold(v.row.clone(), v.params.clone()).unwrap();
    let job = v5_job(&v, 4);
    let request = PalwGenWorkerRequestV1 {
        job: job.clone(),
        prompt_ids: v.prompt.clone(),
        images: vec![GenWireImageV1 { h: v.image.h, w: v.image.w, rgb: v.image.rgb.clone() }],
    };
    let (answer, work) = gen_worker_answer_v1(&held, &borsh::to_vec(&request).unwrap(), FORM);
    let PalwGenWorkerAnswerV1::Result { binding } = &answer else { panic!("{answer:?}") };
    assert_eq!(binding.execution_root(), binding.committed_execution_root);
    assert_eq!(work.unwrap().execution_root(), binding.committed_execution_root);
    assert!(!binding.generated.is_empty());
    // Refusals name the rule; the class stays held.
    let mut other = request.clone();
    other.images[0].rgb[0] ^= 1;
    let (refused, _) = gen_worker_answer_v1(&held, &borsh::to_vec(&other).unwrap(), FORM);
    assert!(matches!(&refused, PalwGenWorkerAnswerV1::Refused { why } if why.contains("image 0")), "{refused:?}");
    let mut other = request.clone();
    other.prompt_ids[0] += 1;
    assert!(matches!(gen_worker_answer_v1(&held, &borsh::to_vec(&other).unwrap(), FORM).0, PalwGenWorkerAnswerV1::Refused { .. }));
    assert!(matches!(gen_worker_answer_v1(&held, b"garbage", FORM).0, PalwGenWorkerAnswerV1::Refused { .. }));
    let (again, _) = gen_worker_answer_v1(&held, &borsh::to_vec(&request).unwrap(), FORM);
    assert_eq!(again, answer, "the same request, the same binding");
}

#[test]
fn a_seat_judges_a_claim_from_its_material() {
    let v = toy();
    let held = GenHeldClassV1::hold(v.row.clone(), v.params.clone()).unwrap();
    let job = v5_job(&v, 4);
    let work = held.run_v5(&job, &v.prompt, std::slice::from_ref(&v.image), FORM).unwrap();
    let root = work.execution_root();
    let judge = |root: &Hash64, prompt: &[u32]| gen_seat_judge_v1(&held, root, &job, prompt, std::slice::from_ref(&v.image), FORM);
    assert_eq!(judge(&root, &v.prompt), GenSeatJudgmentV1::Valid);
    assert!(matches!(judge(&Hash64::from_bytes([1; 64]), &v.prompt), GenSeatJudgmentV1::Differs(_)));
    let mut other = v.prompt.clone();
    other[0] += 1;
    assert!(matches!(judge(&root, &other), GenSeatJudgmentV1::Unjudgeable(_)), "material that is not the job's");
}

#[test]
fn a_capture_rebuilds_the_accused_and_its_lie_is_convicted_at_the_first_divergence() {
    let v = toy();
    let held = GenHeldClassV1::hold(v.row.clone(), v.params.clone()).unwrap();
    let job = v5_job(&v, 4);
    let honest = held.run_v5(&job, &v.prompt, std::slice::from_ref(&v.image), FORM).unwrap();
    let capture = GenCaptureV1::of(&honest).unwrap();
    let bytes = borsh::to_vec(&capture).unwrap();
    let back: GenCaptureV1 = borsh::from_slice(&bytes).unwrap();
    let rebuilt = back.rebuild(&held).unwrap();
    assert_eq!(rebuilt.binding, honest.binding, "the capture is the run's commitments");
    assert_eq!(gen_first_divergence_v1(&rebuilt, &honest), None);
    // A lying executor: one lane of a vision leaf changed in what it committed.
    let mut lying = capture.clone();
    lying.leaves[0][2][0] ^= 1;
    let accused = lying.rebuild(&held).unwrap();
    let index = gen_first_divergence_v1(&accused, &honest).expect("the lie parts from the honest run");
    assert_eq!(index, 2);
    let root = accused.execution_root();
    for (label, candidate) in gen_court_candidates_v1(&held, &accused, index, true, &LIMITS) {
        let GenCourtMoveV1::Close(PalwCourtVerdictProofV2::GenCone { close }) = candidate.unwrap_or_else(|e| panic!("{label}: {e}"))
        else {
            continue;
        };
        let full = close.operands.len();
        let verdict = check_gen_cone_close_v1(&close, &held.row, &held.row.class_id, &root, Some(index), FORM, &LIMITS);
        assert!(matches!(verdict, Ok(Some(_))), "{label}: {verdict:?}");
        eprintln!("the challenger's cone close at leaf {index} carries {full} leaves and {} param leaves", close.params.len());
    }
    // The honest executor's own cone close at the same leaf is acquitted.
    let (_, honest_close) = gen_court_candidates_v1(&held, &honest, index, false, &LIMITS).remove(0);
    let Ok(GenCourtMoveV1::Close(PalwCourtVerdictProofV2::GenCone { close })) = honest_close else { panic!("a cone close") };
    assert_eq!(
        check_gen_cone_close_v1(&close, &held.row, &held.row.class_id, &honest.execution_root(), Some(index), FORM, &LIMITS),
        Ok(None)
    );
}

#[test]
fn a_dissected_leaf_is_answered_with_a_root_claim_and_argued_with_minimal_carriage() {
    let v = llava();
    let held = GenHeldClassV1::hold(v.row.clone(), v.params.clone()).unwrap();
    let job = v5_job(&v, 8);
    let work = held.run_v5(&job, &v.prompt, std::slice::from_ref(&v.image), FORM).unwrap();
    let exec = work.execution_root();
    // The dissected leaf at the stream's last position.
    let (stage, block, node) = held.row.dissected[0];
    let sp = &work.execution.space.stages[stage as usize];
    let leaf = sp
        .leaves()
        .iter()
        .find(|l| {
            l.coord.pos == sp.trip - 1
                && matches!(l.coord.kind, kaspa_consensus_core::palw_gen_step_v1::PalwGenLeafKindV1::Commit { occurrence, node: n }
                    if n == node && sp.occurrence_block(occurrence) == Some(block))
        })
        .unwrap();
    let index = work.execution.space.global_index(&leaf.coord).unwrap();
    assert!(gen_court_candidates_v1(&held, &work, index, true, &LIMITS).is_empty(), "the challenger waits for the root claim");
    let mut moves = gen_court_candidates_v1(&held, &work, index, false, &LIMITS);
    let (label, root) = moves.remove(0);
    let GenCourtMoveV1::RootClaim(root) = root.unwrap_or_else(|e| panic!("{label}: {e}")) else { panic!("{label}: a root claim") };
    let site = check_gen_root_claim_v1(&root, &held.row, &held.row.class_id, &exec, index, FORM, &LIMITS).expect("admitted");
    let total_before = index as usize;
    eprintln!(
        "LLaVA root claim at leaf {index}: its finalize carries {} of the {total_before} leaves before it and {} param leaves",
        root.finalize.operands.len(),
        root.finalize.params.len()
    );
    assert!(root.finalize.operands.len() < total_before, "the finalize carries what it reads, not the whole execution");
    // Played to the bottom with the evidence's rounds and a honest challenger's choices.
    let evidence = work.evidence(&held);
    let mut phase = PalwTirDissectPhaseV1::open_parts(
        Hash64::from_bytes([7; 64]),
        index,
        &site,
        root.version,
        &root.elements,
        &root.totals,
        2,
        1,
        100,
    )
    .unwrap();
    let mut daa = 2;
    while phase.turn() == kaspa_consensus_core::palw_bisect::PalwBisectTurnV1::AwaitDisclosure {
        let round = evidence.round(&phase, &LIMITS).expect("a round");
        phase.apply_round(&round, daa, 100).expect("the honest children fold");
        let choice = PalwTirDissectChoiceV1 {
            version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
            session_id: phase.session_id(),
            round: phase.round(),
            child: (round.children.len() - 1) as u8,
        };
        phase.apply_choice(&choice, daa + 1, 100).expect("a legal choice");
        daa += 2;
    }
    let bottom = evidence.bottom(&phase, &LIMITS).expect("the bottom");
    assert!(bottom.operands.len() < total_before);
    assert_eq!(check_gen_dissect_bottom_v1(&phase, &bottom, &held.row, &held.row.class_id, &exec, index, FORM, &LIMITS), Ok(None));
}

#[test]
fn a_seat_files_valid_only_on_a_matching_root_and_a_party_only_the_move_that_wins_its_side() {
    use kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2;
    use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwCourtVerdictV2};
    assert_eq!(gen_seat_receipt_v1(&GenSeatJudgmentV1::Valid), Ok(PalwReceiptVerdictV2::Valid));
    assert!(gen_seat_receipt_v1(&GenSeatJudgmentV1::Differs("x".into())).is_err(), "a difference is the court's, not a seat's");
    assert!(gen_seat_receipt_v1(&GenSeatJudgmentV1::Unjudgeable("x".into())).is_err());
    let v = toy();
    let held = GenHeldClassV1::hold(v.row.clone(), v.params.clone()).unwrap();
    let job = v5_job(&v, 4);
    let honest = held.run_v5(&job, &v.prompt, std::slice::from_ref(&v.image), FORM).unwrap();
    let mut lying = GenCaptureV1::of(&honest).unwrap();
    lying.leaves[0][2][0] ^= 1;
    let accused = lying.rebuild(&held).unwrap();
    let index = gen_first_divergence_v1(&accused, &honest).unwrap();
    let court = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &court.palw_consensus_mode else { panic!("V2") };
    let session = Hash64::from_bytes([9; 64]);
    let file = |work: &GenWorkV1, challenger: bool| {
        let (_, mv) = gen_court_candidates_v1(&held, work, index, challenger, &LIMITS).remove(0);
        gen_court_object_v1(
            &held.row,
            &work.execution_root(),
            session,
            index,
            2,
            challenger,
            FORM,
            &bundle.court,
            mv.unwrap(),
            &|_| Some(vec![1]),
        )
        .unwrap()
    };
    let conviction = file(&accused, true);
    assert!(
        matches!(conviction, Some(PalwConsensusObjectV2::CourtClosed { verdict: PalwCourtVerdictV2::ExecutorGuilty, .. })),
        "the challenger files the conviction"
    );
    assert_eq!(file(&accused, false), None, "the accused responder has no acquittal to file");
    assert!(
        matches!(
            file(&honest, false),
            Some(PalwConsensusObjectV2::CourtClosed { verdict: PalwCourtVerdictV2::ChallengerDefeated, .. })
        ),
        "an honest responder files its acquittal"
    );
    assert_eq!(file(&honest, true), None, "a challenger files nothing against an honest leaf");
}
