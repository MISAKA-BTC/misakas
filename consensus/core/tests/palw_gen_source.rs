//! **RFC-0003 §II.2.2: the source input and the forced prefix**, on the golden toy encoder–decoder
//! (`consensus-vectors/tir-v2/pipelines/toy-encdec.json`): the encoder stage reads the job's source
//! (`TokenSource::Source`), the text stage a prompt that starts with the class's forced start id.
//!
//! * the class offers a source exactly when a rule reads one, and fitting every reader; its forced
//!   prefix is a text class's, inside the text stage's token bound and the longest offered prompt;
//! * the V5 job carries the source's reference; a claim's binding recomputes the job's facts from
//!   the carried ids, or from zeros of their length where a close does not carry them;
//! * a close carries the source exactly when its disputed stage reads it — an encoder leaf's close
//!   the source and not the prompt, a decoder leaf's the prompt and not the source — each whole and
//!   held to the job's hash; an honest leaf is acquitted and a lie at an encoder leaf convicted;
//! * the prompt starts with the forced prefix, and the source is priced as prompt tokens (OQ15,
//!   PENDING USER CONFIRMATION with OQ13).

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2;
use kaspa_consensus_core::palw_fp_job_v5::*;
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_V4_VERSION, PalwFreePromptJobV3};
use kaspa_consensus_core::palw_gen_artifact_v1::palw_gen_inventory_root_v1;
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_close_v1::*;
use kaspa_consensus_core::palw_gen_step_v1::{palw_gen_stage_root_v1, palw_gen_step_leaf_hash_v1, palw_gen_step_root_v1};
use kaspa_consensus_core::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenDecodeV1, PalwGenExecutionV1, palw_gen_execute_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{PipelineJob, PipelineParams, TirPipelineV1};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::tensor::Tensor;

const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };
const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;
/// The class's forced start id (T5's `decoder_start_token_id`: its pad, 0).
const START: u32 = 0;
/// The least a job's source is charged, in prompt tokens (at or above admission's floor).
const SOURCE_FLOOR: u32 = 6;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

struct Params(Vec<MapParams>);
impl PipelineParams for Params {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.0[program as usize]
    }
}

fn ids(v: &serde_json::Value) -> Vec<u32> {
    v.as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as u32).collect()
}

/// The toy encoder–decoder as a registered Text class: the class, its row, its programs, its
/// params, and the vector's prompt and source.
struct Fixture {
    class: PalwGenClassV1,
    row: PalwGenClassRecordV1,
    pipeline: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    params: Params,
    prompt: Vec<u32>,
    source: Vec<u32>,
}

/// The class's offers: a prompt of up to 8 ids from the forced start, a source of up to 4.
fn offers() -> PalwGenOffersV1 {
    PalwGenOffersV1 {
        steps: vec![],
        scalars: vec![],
        max_prompt_tokens: 8,
        max_negative_tokens: 0,
        images: vec![],
        max_source_tokens: 4,
        forced_prompt_prefix: vec![START],
        source_token_floor: SOURCE_FLOOR,
    }
}

fn fixture_with(offers: PalwGenOffersV1) -> Result<Fixture, PalwGenClassErrorV1> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines/toy-encdec.json");
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
    let class = PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        pipeline: pipeline_bytes,
        programs: program_bytes,
        layouts,
        output: OutputSpecV1::tokens(pipeline.stages[pipeline.output_stage as usize].max_trip),
        offers,
        tokenizer_id: Hash64::from_bytes([0x73; 64]),
    };
    let (root, _) = palw_gen_inventory_root_v1(&programs, &params).unwrap();
    let row = palw_gen_class_record_v1(&class, &root)?;
    Ok(Fixture { class, row, pipeline, programs, params, prompt: ids(&v["job"]["prompt"]), source: ids(&v["job"]["source"]) })
}

fn fixture() -> Fixture {
    fixture_with(offers()).expect("the toy encoder–decoder registers")
}

fn source_ref(source: &[u32]) -> PalwGenSourceRefV1 {
    PalwGenSourceRefV1 { token_ids_hash: prompt_token_ids_commitment_v1(FORM, source).unwrap(), tokens: source.len() as u32 }
}

/// An FP Job V4 of the golden vectors, retargeted at the class: its id, its tokenizer, the prompt,
/// greedy with no controls, a four-id budget — and the source's reference.
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
    v4.prompt_tokens = f.prompt.len() as u32;
    v4.prompt_token_ids_hash = prompt_token_ids_commitment_v1(FORM, &f.prompt).unwrap();
    v4.decode_token_limit = 4;
    v4.decode = Some(DecodeConfigV4::NOOP);
    v4.temperature_q = PalwDecodeSamplingV2::GREEDY.temperature_q;
    PalwFreePromptJobV5 { v4, images: vec![], source: Some(source_ref(&f.source)) }
}

/// The worker's run of the job, and its binding.
fn execute(f: &Fixture, job: &PalwFreePromptJobV5) -> (PalwGenExecutionV1, PalwGenStepBindingV1) {
    let run_job = PipelineJob { prompt: f.prompt.clone(), source: f.source.clone(), ..PipelineJob::default() };
    let decode = PalwGenDecodeV1::of(job).unwrap();
    let e =
        palw_gen_execute_v1(&f.pipeline, &f.programs, &f.class.layouts, &f.params, &run_job, &decode, job.v4.sampling_seed).unwrap();
    let binding = PalwGenStepBindingV1::of(job, &e.claim, e.space.leaf_count());
    (e, binding)
}

fn evidence<'a>(f: &'a Fixture, e: &'a PalwGenExecutionV1, binding: &'a PalwGenStepBindingV1) -> PalwGenEvidenceV1<'a> {
    PalwGenEvidenceV1 { row: &f.row, params: &f.params, execution: e, binding, prompt: &f.prompt, images: &[], source: &f.source }
}

/// An execution whose one leaf is changed, its roots recomputed over it: the executor's lie,
/// committed consistently.
fn lie(e: &PalwGenExecutionV1, stage: usize, index: usize, delta: i128) -> PalwGenExecutionV1 {
    let mut l = e.clone();
    l.leaf_values[stage][index][0] += delta;
    let leaf = l.space.stages[stage].leaves()[index];
    l.leaf_hashes[stage][index] = palw_gen_step_leaf_hash_v1(&leaf, &l.leaf_values[stage][index]).unwrap();
    l.claim.stage_roots[stage] = palw_gen_stage_root_v1(stage as u8, &l.leaf_hashes[stage]);
    l.claim.step_root = palw_gen_step_root_v1(&l.claim.stage_roots);
    l
}

fn check(f: &Fixture, close: &PalwGenConeCloseV1, claim_root: &Hash64, narrowed: Option<u64>) -> PalwGenCloseOutcomeV1 {
    check_gen_cone_close_v1(close, &f.row, &f.row.class_id, claim_root, narrowed, FORM, &LIMITS)
}

#[test]
fn a_class_offers_a_source_exactly_when_a_rule_reads_one_and_forces_a_text_prefix() {
    let f = fixture();
    let fence = PalwGenFenceV1::drill_v1(ForkActivation::new(5_000));
    let report = palw_gen_class_preflight_v1(&f.class, &fence).unwrap_or_else(|e| panic!("the toy encoder–decoder: {e}"));
    assert_eq!(report.profile, PalwGenProfileV1::Text);
    // The encoder is source work: its admitted work over the text stage's per-token work, rounded up.
    let per_token = palw_gen_work_units_v1(&report.admission.stages[1].admission.view.position.cost);
    let encoder = palw_gen_work_units_v1(&report.admission.stages[0].job_cost);
    assert_eq!(report.source_token_floor as u128, encoder.div_ceil(per_token), "⌈{encoder} / {per_token}⌉");
    assert!(report.source_token_floor >= 1 && report.source_token_floor <= SOURCE_FLOOR, "{}", report.source_token_floor);
    assert!(report.image_token_floor.is_empty());
    assert!(palw_gen_class_reads_source_v1(&f.row.class.offers));
    assert_eq!(
        palw_fp_job_version_offered_v1(&f.row.class.offers, false),
        Err(PalwFpV5Error::JobVersionNotOffered { job: "FP Job V4" })
    );
    let refused = |edit: &dyn Fn(&mut PalwGenOffersV1)| {
        let mut class = f.class.clone();
        edit(&mut class.offers);
        palw_gen_class_preflight_v1(&class, &fence).err().map(|e| e.to_string()).unwrap_or_default()
    };
    let none = refused(&|o| o.max_source_tokens = 0);
    assert!(none.contains("a rule reads the source and no source is offered"), "{none}");
    // `[1] ‖ source ‖ [2]` pads to 6: a source of 5 does not fit.
    let long = refused(&|o| o.max_source_tokens = 5);
    assert!(long.contains("the longest source (5 ids)"), "{long}");
    let past = refused(&|o| o.forced_prompt_prefix = vec![START, 16]);
    assert!(past.contains("past the text stage's token bound 16"), "{past}");
    let longer = refused(&|o| o.forced_prompt_prefix = vec![START; 9]);
    assert!(longer.contains("longer than the longest offered prompt"), "{longer}");
    assert_eq!(refused(&|o| o.forced_prompt_prefix = vec![]), "", "a class may force nothing");
    assert_eq!(refused(&|o| o.max_source_tokens = 1), "", "or offer a shorter source");
    // The source's price floor: at or above admission's, and only on a class that reads a source.
    let floor = report.source_token_floor;
    let under = refused(&|o| o.source_token_floor = floor - 1);
    assert!(under.contains(&format!("below its floor {floor}")), "{under}");
    assert_eq!(refused(&|o| o.source_token_floor = floor), "", "at the floor");
}

#[test]
fn the_binding_recomputes_the_jobs_facts_from_its_source() {
    let f = fixture();
    let job = v5_job(&f);
    assert_eq!(palw_fp_v5_resolve_class_v1(&job, Some(&f.row), true).map(|r| r.class_id), Ok(f.row.class_id));
    let (e, binding) = execute(&f, &job);
    assert_eq!(binding.execution_root(), binding.committed_execution_root);
    let root = binding.committed_execution_root;
    let verify = |ids: PalwGenJobIdsV1<'_>| verify_gen_binding_v1(&binding, &f.row, &f.row.class_id, &root, ids);
    let carried = PalwGenJobIdsV1 { prompt: Some(&f.prompt), source: Some(&f.source) };
    let Ok(PalwGenBindingOutcomeV1::Verified(v)) = verify(carried) else { panic!("the honest binding verifies") };
    assert_eq!(v.facts[0].inputs[&0].data, vec![1, 7, 9, 11, 2, 0], "the encoder's ids: the template over the source");
    assert_eq!(v.facts[0].inputs[&1].data, vec![5], "and their count");
    assert_eq!(v.space.leaf_count(), e.space.leaf_count());
    // Not carried: zeros of the source's length stand in, and the space is the same.
    let Ok(PalwGenBindingOutcomeV1::Verified(z)) = verify(PalwGenJobIdsV1::default()) else { panic!("verifies") };
    assert_eq!(z.facts[0].inputs[&0].data, vec![1, 0, 0, 0, 2, 0]);
    assert_eq!(z.facts[0].inputs[&1].data, vec![5], "the count is the job's");
    assert_eq!(z.space.leaf_count(), v.space.leaf_count());
    // A carried source that is not the job's length is refused.
    let short = PalwGenJobIdsV1 { prompt: None, source: Some(&f.source[..2]) };
    assert!(matches!(verify(short), Err(PalwGenCloseErrorV1::SourceNotTheJobs)));
    // A job of this class without a source is not the class's job.
    let mut no_source = job.clone();
    no_source.source = None;
    assert_eq!(
        palw_fp_v5_resolve_class_v1(&no_source, Some(&f.row), true).err(),
        Some(PalwFpV5Error::ImageCount(0)),
        "no images and no source: a V4 job's shape"
    );
    let mut long = job.clone();
    long.source = Some(source_ref(&[3, 4, 5, 6, 7]));
    assert_eq!(palw_fp_v5_resolve_class_v1(&long, Some(&f.row), true).err(), Some(PalwFpV5Error::SourceLength { tokens: 5, max: 4 }));
}

#[test]
fn a_close_carries_the_source_exactly_when_its_stage_reads_it() {
    let f = fixture();
    let job = v5_job(&f);
    let (e, binding) = execute(&f, &job);
    let root = binding.committed_execution_root;
    assert!(palw_gen_stage_reads_source_v1(&f.pipeline, 0) && !palw_gen_stage_reads_prompt_v1(&f.pipeline, 0), "the encoder");
    assert!(!palw_gen_stage_reads_source_v1(&f.pipeline, 1) && palw_gen_stage_reads_prompt_v1(&f.pipeline, 1), "the decoder");
    let ev = evidence(&f, &e, &binding);
    let encoder_leaves = e.space.stages[0].leaves().len() as u64;
    // Every encoder leaf's close carries the source and not the prompt, and acquits.
    for index in 0..encoder_leaves {
        let close = ev.cone_close(index, &LIMITS).unwrap();
        assert_eq!(close.source_ids, f.source, "leaf {index}: the source, whole");
        assert!(close.prompt_ids.is_empty(), "leaf {index}: nothing of the prompt");
        assert_eq!(check(&f, &close, &root, Some(index)), Ok(None), "leaf {index}: honest");
    }
    // A decoder leaf's close carries the prompt and not the source.
    let dec = ev.cone_close(encoder_leaves, &LIMITS).unwrap();
    assert_eq!(dec.prompt_ids, f.prompt);
    assert!(dec.source_ids.is_empty(), "a decoder dispute reveals nothing of the source");
    assert_eq!(check(&f, &dec, &root, Some(encoder_leaves)), Ok(None));
    // The carriage rule, by name.
    let enc = ev.cone_close(0, &LIMITS).unwrap();
    let mut bare = enc.clone();
    bare.source_ids.clear();
    assert_eq!(check(&f, &bare, &root, Some(0)), Err(PalwGenCloseErrorV1::SourceNotCarried));
    let mut other = enc.clone();
    other.source_ids[0] += 1;
    assert_eq!(check(&f, &other, &root, Some(0)), Err(PalwGenCloseErrorV1::SourceNotTheJobs), "not the job's source");
    let mut shorter = enc.clone();
    shorter.source_ids.pop();
    assert_eq!(check(&f, &shorter, &root, Some(0)), Err(PalwGenCloseErrorV1::SourceNotTheJobs));
    let mut revealed = dec.clone();
    revealed.source_ids = f.source.clone();
    assert_eq!(check(&f, &revealed, &root, Some(encoder_leaves)), Err(PalwGenCloseErrorV1::SourceNotTheJobs));
    // The decode close reads neither list's values: it verifies over zeros of their lengths.
    assert_eq!(check_gen_decode_close_v1(&ev.decode_close(0).unwrap(), &f.row, &f.row.class_id, &root, None), Ok(None));
    // A lie at an encoder leaf, committed consistently, is convicted by a close carrying the source.
    let l = lie(&e, 0, 0, 1);
    let lb = PalwGenStepBindingV1::of(&job, &l.claim, l.space.leaf_count());
    let lied = evidence(&f, &l, &lb).cone_close(0, &LIMITS).unwrap();
    assert_eq!(lied.source_ids, f.source);
    assert!(matches!(check(&f, &lied, &lb.committed_execution_root, Some(0)), Ok(Some(_))), "the encoder's lie is convicted");
}

#[test]
fn the_prompt_starts_with_the_forced_prefix_and_the_source_is_priced_as_prompt_tokens() {
    let f = fixture();
    let offers = &f.row.class.offers;
    assert_eq!(palw_fp_v5_prompt_head_admitted_v1(offers, &f.prompt), Ok(()));
    assert_eq!(f.prompt[0], START);
    assert_eq!(palw_fp_v5_prompt_head_admitted_v1(offers, &[5, START]), Err(PalwFpV5Error::PromptPrefix));
    let job = v5_job(&f);
    assert_eq!(job.source.map(|s| s.tokens), Some(3));
    assert_eq!(palw_fp_v5_source_tokens_v1(offers, &job), SOURCE_FLOOR as u64, "three ids pay the floor");
    assert_eq!(palw_fp_v5_input_tokens_v1(offers, &job), SOURCE_FLOOR as u64, "no image slots: the source alone");
    assert_eq!(palw_fp_v5_input_charge_v1(offers, &job, 250), 250 * SOURCE_FLOOR as u128);
}

/// A commitment payload carrying `job` (the lane's shape, the wire test's fixture fields) and, for
/// `PublicDa`, the prompt's ids.
fn payload(job: &PalwFreePromptJobV5, prompt: &[u32]) -> kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 {
    use kaspa_consensus_core::palw_freeprompt_v3::*;
    let h = |w: u64| Hash64::from_u64_word(w);
    let commitment = PalwFreePromptCommitmentV3 {
        job: job.into_carried(),
        trace_root: h(0x7A),
        output_root: h(0x0B),
        schedule_root: h(0x5C),
        execution_root: h(0x4E),
        decode_tokens_executed: 3,
        stop_reason: PalwFpStopReasonV3::EndOfGeneration,
        work_leaves: 64,
        trace_manifest_root: h(0x3F),
        trace_chunk_count: 1,
        trace_retention_daa: 505_000,
    };
    let ids = palw_fp_carried_prompt_ids_v1(&job.v4, prompt);
    PalwFpCommitmentTxPayloadV3 { version: PALW_FP_V3_VERSION, commitment, prompt_token_ids: ids, signature: vec![0x5A; 4627] }
}

/// **RFC-0001 §A.3's unit, RFC-0003 §I.3's convention, at V5 acceptance.** The temperature, the
/// frequency and presence penalties and the logit bias are measured in the class's logit units (Q24).
/// A generative text class's logits are in whatever unit its lowerer calibrated until the fence that
/// guarantees Q24 by construction opens the lane to classes registered past it, so a V5 job is offered
/// greedy selection, the repeat penalty, stop sequences and constraints, and a unit-dependent control
/// is refused by name.
#[test]
fn acceptance_offers_a_generative_class_only_the_unit_free_decode_controls() {
    use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
    use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PUBLIC_DA, PalwFpV3Error};
    let f = fixture();
    let mut base = v5_job(&f);
    base.v4.privacy_mode = PALW_FP_PRIVACY_PUBLIC_DA;
    let accept = |job: &PalwFreePromptJobV5| {
        let p = payload(job, &f.prompt);
        palw_fp_v5_accept_payload_v1(&p, Some(&f.row), job.v4.network_domain, true, 1 << 26, None, FORM, true).map(|(j, _)| j)
    };
    assert_eq!(accept(&base), Ok(base.clone()), "the fixture's greedy job is offered");
    let with = |temperature_q: u32, decode: DecodeConfigV4| {
        let mut j = base.clone();
        j.v4.decode = Some(decode);
        j.v4.temperature_q = temperature_q;
        if temperature_q != 0 {
            j.v4.sampling_seed = [0x33; 32];
        }
        j
    };
    for (what, job) in [
        ("the repeat penalty", with(0, DecodeConfigV4 { repeat_penalty_q: 98_304, penalty_window: 6, ..DecodeConfigV4::NOOP })),
        ("a stop sequence", with(0, DecodeConfigV4 { stop_sequences: vec![vec![3]], ..DecodeConfigV4::NOOP })),
    ] {
        assert_eq!(accept(&job), Ok(job.clone()), "{what} is unit-free");
    }
    for (what, control, job) in [
        ("the temperature", "temperature_q", with(1 << 24, DecodeConfigV4::NOOP)),
        (
            "the frequency penalty",
            "frequency_penalty_q",
            with(0, DecodeConfigV4 { frequency_penalty_q: 1 << 23, penalty_window: 4, ..DecodeConfigV4::NOOP }),
        ),
        (
            "the presence penalty",
            "presence_penalty_q",
            with(0, DecodeConfigV4 { presence_penalty_q: -(1 << 22), penalty_window: 4, ..DecodeConfigV4::NOOP }),
        ),
        ("a logit bias", "logit_bias", with(0, DecodeConfigV4 { logit_bias: vec![(3, 1 << 24)], ..DecodeConfigV4::NOOP })),
    ] {
        assert_eq!(
            accept(&job),
            Err(PalwFpV5Error::V4(PalwFpV3Error::DecodeControlNeedsQ24Logits { control })),
            "{what} is in a unit the class does not declare"
        );
    }
}

/// **Acceptance on `PublicDa`**: the prompt rides the payload, and it starts with the class's forced
/// prefix or the claim is refused by name; the source never rides it.
#[test]
fn acceptance_holds_a_public_prompt_to_the_forced_prefix() {
    use kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA;
    let f = fixture();
    let mut job = v5_job(&f);
    job.v4.privacy_mode = PALW_FP_PRIVACY_PUBLIC_DA;
    let accept = |job: &PalwFreePromptJobV5, prompt: &[u32]| {
        let p = payload(job, prompt);
        palw_fp_v5_accept_payload_v1(&p, Some(&f.row), job.v4.network_domain, true, 1 << 26, None, FORM, true).map(|(j, _)| j)
    };
    let stateless = palw_fp_v5_validate_payload_v1(&payload(&job, &f.prompt), job.v4.network_domain, true, 1 << 26, None, FORM, true);
    assert_eq!(stateless.as_ref().map(|j| j.source), Ok(job.source), "the fixture payload passes V4's rules: {stateless:?}");
    assert_eq!(accept(&job, &f.prompt), Ok(job.clone()));
    assert_eq!(payload(&job, &f.prompt).prompt_token_ids, f.prompt, "the prompt rides; the source does not");
    let unforced = vec![5, 15, 15, 5];
    let mut other = job.clone();
    other.v4.prompt_token_ids_hash = prompt_token_ids_commitment_v1(FORM, &unforced).unwrap();
    assert_eq!(accept(&other, &unforced), Err(PalwFpV5Error::PromptPrefix));
    let mut unknown = job.clone();
    unknown.v4.class_id = Hash64::from_bytes([0x11; 64]);
    assert!(matches!(
        palw_fp_v5_accept_payload_v1(&payload(&unknown, &f.prompt), None, job.v4.network_domain, true, 1 << 26, None, FORM, true),
        Err(PalwFpV5Error::UnknownClass(_))
    ));
}
