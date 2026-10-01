//! **The generative classes the int-11 drill registers** (RFC-0003 activation step 8, "Plan B"): three small
//! pipeline classes built here, held to the registration gate a node runs, and written as `PALWTIR2` containers.
//!
//! * **toy-image** — the golden toy text-to-image pipeline of `consensus-vectors/tir-v2/pipelines/toy-image.json`
//!   (encoder, a two-layer denoiser whose latent is a `Fixed` state, a decoder to `ImageRgb8` 2×2): kilobytes;
//! * **toy-embed** — tir-lower's tiny BERT fixture (2 layers, hidden 32, mean pooling, L2-normalised) as an
//!   Embedding class;
//! * **wide-embed** — a synthetic Embedding class with one wide `i8` matrix (hidden 512, output tiles of 256
//!   lanes): its cone closes exceed one lifecycle carrier, so PALW-GEN-21 refuses it until
//!   `palw_held_close_chunks_v1` is armed and a lie at its leaves can be convicted only through the held leaf
//!   challenge (decision 22). It is the drill's stress class for tag 90 (DG-6, DG-7).
//!
//! The ordinary test holds each class to the gate (the wide one under a ruleset that also arms the held leaf
//! challenge), runs a job, plants a lie (`GenPlantV1`, what `misaka palw gen-claim --plant` commits) and checks the
//! court convicts it, and measures the close the court would carry: toy-image and toy-embed fit one carrier
//! (the one-move accusation), wide-embed does not. The `#[ignore]`d test writes the artifacts:
//!
//! ```text
//! PALW_GEN_DRILL_OUT=<dir> cargo test -p misaka-palw-sdk --test gen_drill_classes -- --ignored --nocapture
//! ```
//!
//! `<dir>/<name>.class.palwtir2` per class and `<dir>/drill-classes.json` (model id, class id, artifact root,
//! bytes, the canonical and widest step-leaf counts, and a `gen-claim` request template).

use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_court_v2::{PalwCourtVerdictProofV2, palw_gen_close_verdict_for_row_v1};
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER};
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_job_v1::*;
use kaspa_consensus_core::palw_gen_v1::{PALW_DRILL_GEN_V1_ENTRY, PalwGenProfileV1};
use kaspa_consensus_core::palw_held_close_v1::PALW_HELD_CLOSE_CHUNKS_ENTRY_V1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_state_v2::{PALW_OBJECT_CHUNK_MAX_BYTES, PalwCourtVerdictV2};
use kaspa_consensus_core::palw_tir_v1::PALW_T12_TIR_V1_ENTRY;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_base0::gen_tensor_worker::{GenPlantV1, GenTensorCaptureV1, GenTensorWorkV1, gen_tensor_court_candidates_v1};
use misaka_palw_base0::gen_worker::{GenCourtMoveV1, GenHeldClassV1};
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_sdk::gen_class::*;
use misaka_palw_tir as tir;
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{
    Binding, PipelineParams, StageDecl, TIR_PIPELINE_VERSION_V1, TirPipelineV1, TokenPad, TokenRule, TokenSource, TripRule,
};
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::program_v2::{InputSource, OutputDecl, TirProgramV2};
use misaka_palw_tir::tensor::Tensor;
use misaka_palw_tir::{Cmp, DType, Dim, Ref, Rounding};
use misaka_palw_tir_lower::embedding::{EmbeddingLowerOptsV1, lower_bidir_embedding_v1};
use misaka_palw_tir_lower::lower::bidir::Pooling;
use std::path::{Path, PathBuf};

const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;
const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };

// ---------------------------------------------------------------------------------------------
// The classes
// ---------------------------------------------------------------------------------------------

/// Weights of a pipeline's programs, by program index.
struct Weights(Vec<Box<dyn ParamSource + Send + Sync>>);

impl PipelineParams for Weights {
    fn params(&self, program: u16) -> &dyn ParamSource {
        self.0[program as usize].as_ref()
    }
}

/// One drill class: the declared class, its weights and the facts a `gen-claim` request is made from.
struct DrillClass {
    name: &'static str,
    model_id: &'static str,
    weights: Weights,
    declared: GenDeclaredV1,
    row: PalwGenClassRecordV1,
    /// A job over the class (the worker's own run: `(job, prompt ids)`).
    job: PalwGenJobV1,
    ids: Vec<u32>,
    /// The `gen-claim` request this class's job is made from, with placeholders for the chain's anchor.
    request: serde_json::Value,
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

/// A ruleset under which a class is judged as it would be once every fence of the lane is armed: the IR fence,
/// the generative fence and the held leaf challenge, all at DAA 1. (An operator's offline gate asks the network's
/// own; the drill's classes are registered after `palw_gen_v1` at 28 and, for the wide one, after
/// `palw_held_close_chunks_v1` at 140.)
fn all_armed() -> Params {
    let mut p = palw_t12_shipped_params();
    (PALW_T12_TIR_V1_ENTRY.set)(&mut p, Some(ForkActivation::new(1)));
    (PALW_DRILL_GEN_V1_ENTRY.set)(&mut p, Some(ForkActivation::new(1)));
    (PALW_HELD_CLOSE_CHUNKS_ENTRY_V1.set)(&mut p, Some(ForkActivation::new(1)));
    p
}

fn declare(
    params: &Params,
    spec: &GenClassSpecV1,
    weights: &Weights,
    choice: &GenLayoutChoiceV1,
) -> (GenDeclaredV1, PalwGenClassRecordV1) {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let declared = gen_declare_layout_v1(params, bundle, spec, weights, choice).unwrap_or_else(|e| panic!("declare: {e}"));
    let row = declared.row.clone().expect("the class derives a registry row");
    (declared, row)
}

fn job_envelope(row: &PalwGenClassRecordV1, privacy: u8) -> PalwJobEnvelopeV1 {
    PalwJobEnvelopeV1 {
        network_domain: Hash64::from_bytes([0xD0; 64]),
        class_id: row.class_id,
        executor_bond: TransactionOutpoint::new(TransactionId::from_bytes([7; 64]), 0),
        executor_pubkey: vec![1; 8],
        operator_id: Hash64::from_bytes([2; 64]),
        anchor_block: Hash64::from_bytes([3; 64]),
        anchor_daa: 100,
        job_nonce: [4; 32],
        privacy_mode: privacy,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
    }
}

fn request_template(profile: &str, ids: &[u32], extra: serde_json::Value) -> serde_json::Value {
    let mut v = serde_json::json!({
        "profile": profile,
        "anchor_block": "<128 hex: a recent block of the chain>",
        "anchor_daa": "<its DAA score>",
        "job_nonce": "<64 hex: unique per claim>",
        "seed": "<64 hex>",
        "privacy": "public",
        "prompt_token_ids": ids,
    });
    if let (Some(base), Some(add)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in add {
            base.insert(k.clone(), x.clone());
        }
    }
    v
}

// ---- toy-image ------------------------------------------------------------------------------------

fn toy_image() -> DrillClass {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v2/pipelines/toy-image.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the golden vector")).expect("json");
    let program_bytes: Vec<Vec<u8>> =
        v["programs"].as_array().unwrap().iter().map(|p| unhex(p["program_borsh_hex"].as_str().unwrap())).collect();
    let programs: Vec<TirProgramV2> = program_bytes.iter().map(|b| TirProgramV2::decode_canonical(b).unwrap()).collect();
    let pipeline = TirPipelineV1::decode_canonical(&unhex(v["pipeline_borsh_hex"].as_str().unwrap()), &programs).unwrap();
    let maps: Vec<Box<dyn ParamSource + Send + Sync>> = v["programs"]
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
            Box::new(m) as Box<dyn ParamSource + Send + Sync>
        })
        .collect();
    let weights = Weights(maps);
    let steps_max = pipeline.stages.iter().filter(|st| matches!(st.trip, TripRule::JobSteps)).map(|st| st.max_trip).min().unwrap();
    let mut scalars: Vec<PalwGenScalarOfferV1> = Vec::new();
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
    let encoder = pipeline.stages.iter().find(|st| st.tokens.is_some()).unwrap();
    let guidance = PalwGenGuidanceOfferV1 { scalar: 0, lo: scalars[0].lo as u16, hi: scalars[0].hi as u16 };
    let spec = GenClassSpecV1 {
        profile: PalwGenProfileV1::Image,
        pipeline,
        programs,
        tokenizer_id: Hash64::from_bytes([0x71; 64]),
        output: OutputSpecV1::image_rgb8(2, 2),
        offers: PalwGenOffersV1 {
            steps: vec![steps_max - 1, steps_max],
            profile: PalwGenProfileOffersV1::Image(PalwGenImageOffersV1 {
                sampler_id: Hash64::from_bytes([0x5A; 64]),
                guidance: Some(guidance.clone()),
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
    };
    let (declared, row) = declare(
        &all_armed(),
        &spec,
        &weights,
        &GenLayoutChoiceV1 { tile_len: 4, output_tile: None, h_chunk: 16, checkpoint_interval: 1 },
    );
    let ids = vec![2, 5, 1];
    let steps = spec.offers.steps[0] as u16;
    let job = PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: job_envelope(&row, PALW_FP_PRIVACY_PUBLIC_DA),
        seed: [0x33; 32],
        body: PalwGenBodyV1::Image(PalwGenImageBodyV1 {
            prompt_token_ids_hash: prompt_token_ids_commitment_v1(FORM, &ids).unwrap(),
            prompt_tokens: ids.len() as u32,
            negative_token_ids_hash: Hash64::default(),
            negative_tokens: 0,
            guidance_q: guidance.lo,
            image_index: 0,
            sampler_id: Hash64::from_bytes([0x5A; 64]),
            steps,
            width: 2,
            height: 2,
            output: misaka_palw_gen::OutputKindV1::ImageRgb8.tag(),
        }),
    };
    let request = request_template("image", &ids, serde_json::json!({"steps": steps, "guidance_q": guidance.lo, "image_index": 0}));
    DrillClass { name: "toy-image", model_id: "palw-drill/toy-image", weights, declared, row, job, ids, request }
}

// ---- toy-embed ------------------------------------------------------------------------------------

fn toy_embed() -> DrillClass {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/hf-enc/bert");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).expect("outputs")).unwrap();
    let seqs = v["sequences"].as_array().unwrap();
    let ids_of =
        |s: &serde_json::Value| -> Vec<u32> { s["padded"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as u32).collect() };
    let first = ids_of(&seqs[0]);
    let count = seqs[0]["count"].as_u64().unwrap() as usize;
    let (cls, sep, pad) = (first[0], first[count - 1], 0u32);
    let lowered = lower_bidir_embedding_v1(
        &dir,
        &EmbeddingLowerOptsV1 {
            pooling: Pooling::Mean,
            normalize: true,
            lmax: 12,
            pad,
            cls,
            sep,
            calibration_sequences: 8,
            calibration_seed: 11,
        },
    )
    .unwrap_or_else(|e| panic!("the lowering: {e}"));
    let q = lowered.q.expect("a normalised embedding is a power-of-two unit (Q30)");
    // `Weights` holds boxed param sources; the lowering's params are `IntParams` (a `ParamSource`).
    let weights = Weights(vec![Box::new(lowered.params.clone()) as Box<dyn ParamSource + Send + Sync>]);
    let spec = GenClassSpecV1 {
        profile: PalwGenProfileV1::Embedding,
        pipeline: lowered.pipeline.clone(),
        programs: vec![lowered.program.clone()],
        tokenizer_id: Hash64::from_bytes([0xE5; 64]),
        output: OutputSpecV1::embedding_i32(1, lowered.dims, q, lowered.normalised),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: lowered.max_prompt_tokens,
            max_negative_tokens: 0,
            images: vec![],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
            profile: PalwGenProfileOffersV1::Embedding(PalwGenEmbeddingOffersV1 {
                pooling: PALW_GEN_POOLING_MEAN_V1,
                dims: vec![lowered.dims],
            }),
        },
    };
    let (declared, row) = declare(
        &all_armed(),
        &spec,
        &weights,
        &GenLayoutChoiceV1 { tile_len: 16, output_tile: None, h_chunk: 16, checkpoint_interval: 1 },
    );
    let ids: Vec<u32> = vec![11, 25, 7];
    let job = embedding_job(&row, &ids, lowered.dims, PALW_FP_PRIVACY_PANEL_DA);
    let request = request_template(
        "embedding",
        &ids,
        serde_json::json!({"pooling": PALW_GEN_POOLING_MEAN_V1, "dims": lowered.dims, "privacy": "panel"}),
    );
    DrillClass { name: "toy-embed", model_id: "palw-drill/toy-embed", weights, declared, row, job, ids, request }
}

fn embedding_job(row: &PalwGenClassRecordV1, ids: &[u32], dims: u32, privacy: u8) -> PalwGenJobV1 {
    PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: job_envelope(row, privacy),
        seed: [0; 32],
        body: PalwGenBodyV1::Embedding(PalwGenEmbeddingBodyV1 {
            input: PalwGenEmbeddingInputV1::Text {
                token_ids_hash: prompt_token_ids_commitment_v1(FORM, ids).unwrap(),
                tokens: ids.len() as u32,
            },
            pooling: PALW_GEN_POOLING_MEAN_V1,
            dims,
            output: misaka_palw_gen::OutputKindV1::EmbeddingI32.tag(),
        }),
    }
}

// ---- wide-embed -----------------------------------------------------------------------------------

const WIDE_D: u32 = 512;
const WIDE_VOCAB: u32 = 64;
const WIDE_LEN: u32 = 8;
const WIDE_PAD: u32 = 0;

/// A 64-bit LCG, so no artifact depends on an RNG crate's stream.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 ^ (self.0 >> 29)
    }
    fn range(&mut self, lo: i128, hi: i128) -> i128 {
        lo + (self.next() as u128 % (hi - lo + 1) as u128) as i128
    }
}

/// The wide program: the embeddings of the tokens the count admits, summed to A16 codes, times one wide `i8`
/// matrix (`[D, D]`), shifted back: one wide commit point, then the output.
fn wide_program() -> TirProgramV2 {
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let tokens = pb.param("wide.tokens", DType::Idx, &[WIDE_LEN], false);
    let count = pb.param("wide.count", DType::Idx, &[], false);
    let embed = pb.param("wide.embed", DType::I16, &[WIDE_VOCAB, WIDE_D], false);
    let w = pb.param("wide.w", DType::I8, &[WIDE_D, WIDE_D], false);
    let pre = {
        let mut b = pb.block("wide.pre", vec![]);
        let e = b.gather(embed, tokens, 0, 0);
        let iota = b.iota(DType::Idx, &[Dim::Fixed(WIDE_LEN)], 0, 0, 1);
        let mask = b.compare(iota, count, Cmp::Lt);
        let mask = b.reshape_fixed(mask, &[WIDE_LEN, 1]);
        let e32 = b.cast(e, DType::I32);
        let zero = b.c(DType::I32, 0);
        let kept = b.select(mask, e32, zero, DType::I32);
        let s = b.reduce_sum(kept, 0, DType::I32);
        let codes = b.clamp(s, -32767, 32767, DType::I16);
        let y = b.matmul(codes, w, DType::I64);
        let y = b.shr(y, 8, Rounding::HalfAwayFromZero, DType::I64);
        let y = b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.finish(&[y])
    };
    let carry = {
        let b = &pb.blocks[pre as usize];
        vec![b.nodes[b.carry_out[0] as usize].out.clone()]
    };
    let (post, out) = {
        let mut b = pb.block("wide.post", carry);
        let o = b.clamp(Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(o);
        let Ref::Node(n) = o else { unreachable!("a clamp is a node") };
        (b.finish(&[]), n)
    };
    let v1 = pb.finish(pre, vec![], post, out);
    TirProgramV2::from_v1_lifting_params(
        &v1,
        &[(0, InputSource::External { lo: 0, hi: WIDE_VOCAB as i64 - 1 }), (1, InputSource::External { lo: 0, hi: WIDE_LEN as i64 })],
        OutputDecl::Final { node: out },
    )
    .expect("the wide program lifts")
}

fn wide_embed() -> DrillClass {
    let program = wide_program();
    let rule = TokenRule {
        prefix: vec![1],
        source: TokenSource::Prompt,
        suffix: vec![2],
        pad: Some(TokenPad { id: WIDE_PAD, to_len: WIDE_LEN }),
    };
    let pipeline = TirPipelineV1 {
        version: TIR_PIPELINE_VERSION_V1,
        stages: vec![StageDecl {
            name: "encode".into(),
            program: 0,
            trip: TripRule::Fixed { n: 1 },
            max_trip: 1,
            tokens: None,
            bind: vec![Binding::JobTokens { rule: rule.clone() }, Binding::JobTokenCount { rule }],
        }],
        output_stage: 0,
    };
    // Deterministic weights.
    let mut rng = Lcg(0x57_1DE);
    let mut m = MapParams::default();
    for (j, d) in program.params.iter().enumerate() {
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        let (lo, hi) = match d.dtype {
            DType::I8 => (-60, 60),
            DType::I16 => (-2000, 2000),
            _ => (0, 1),
        };
        let data = (0..n).map(|_| rng.range(lo, hi)).collect();
        m.tensors.insert((j as u16, None), Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).unwrap());
    }
    let weights = Weights(vec![Box::new(m) as Box<dyn ParamSource + Send + Sync>]);
    let spec = GenClassSpecV1 {
        profile: PalwGenProfileV1::Embedding,
        pipeline,
        programs: vec![program],
        tokenizer_id: Hash64::from_bytes([0x77; 64]),
        output: OutputSpecV1::embedding_i32(1, WIDE_D, 16, false),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: WIDE_LEN - 2,
            max_negative_tokens: 0,
            images: vec![],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
            profile: PalwGenProfileOffersV1::Embedding(PalwGenEmbeddingOffersV1 {
                pooling: PALW_GEN_POOLING_MEAN_V1,
                dims: vec![WIDE_D],
            }),
        },
    };
    // Tiles of 256 lanes: the wide commit point is two tiles, and a cone close of either opens the whole matrix.
    let (declared, row) = declare(
        &all_armed(),
        &spec,
        &weights,
        &GenLayoutChoiceV1 { tile_len: 256, output_tile: None, h_chunk: 16, checkpoint_interval: 1 },
    );
    let ids: Vec<u32> = vec![11, 25, 7];
    let job = embedding_job(&row, &ids, WIDE_D, PALW_FP_PRIVACY_PUBLIC_DA);
    let request = request_template("embedding", &ids, serde_json::json!({"pooling": PALW_GEN_POOLING_MEAN_V1, "dims": WIDE_D}));
    DrillClass { name: "wide-embed", model_id: "palw-drill/wide-embed", weights, declared, row, job, ids, request }
}

// ---------------------------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------------------------

fn court() -> kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2 {
    match &palw_t12_shipped_params().palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.court.clone(),
        _ => unreachable!("testnet-12 runs V2"),
    }
}

fn container_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("misaka-gen-drill-{}-{name}.palwtir2", std::process::id()))
}

struct Guard(PathBuf);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The class held from its container (the path a node takes).
fn held(c: &DrillClass) -> (GenHeldClassV1<misaka_palw_tir_artifact::PalwTirContainerV2>, Guard) {
    let path = container_path(c.name);
    let guard = Guard(path.clone());
    gen_write_declared_container_v1(&path, &c.declared, &c.weights, format!("{{\"model_id\":\"{}\"}}", c.model_id))
        .expect("the container");
    let held = GenHeldClassV1::hold_container(c.row.clone(), &path).expect("the worker holds the class");
    (held, guard)
}

/// The cone close of the accused run at global leaf `leaf`, as a challenger builds it, with its serialized size.
fn cone_close(
    held: &GenHeldClassV1<misaka_palw_tir_artifact::PalwTirContainerV2>,
    accused: &GenTensorWorkV1,
    leaf: u64,
) -> (PalwCourtVerdictProofV2, usize) {
    let moves = gen_tensor_court_candidates_v1(held, accused, leaf, true, &LIMITS);
    let (_, cone) = moves.iter().find(|(n, _)| *n == "cone").expect("a cone close is a candidate at a leaf that is not dissected");
    let GenCourtMoveV1::Close(proof) = cone.clone().expect("the cone close builds") else { panic!("a close") };
    let bytes = borsh::to_vec(&proof).expect("a close serializes").len();
    (proof, bytes)
}

#[test]
fn the_drill_classes_pass_the_gate_and_a_planted_lie_is_convicted_with_the_close_the_lane_expects() {
    // The one-move accusation carries the close in one lifecycle carrier less its framing (PALW-GEN-21).
    let one_move_max = kaspa_consensus_core::palw_gen_one_move_v1::palw_gen_one_move_max_proof_bytes_v1() as usize;
    assert!(one_move_max < PALW_OBJECT_CHUNK_MAX_BYTES);
    for c in [toy_image(), toy_embed(), wide_embed()] {
        c.declared.admission.as_ref().unwrap_or_else(|why| panic!("{}: the registration gate refuses the class: {why}", c.name));
        let (held, _file) = held(&c);
        // The job runs; its capture rebuilds to the same commitments (what a seat is served).
        let honest =
            held.run_tensor(&c.job, &c.ids, &[], &[], FORM).unwrap_or_else(|e| panic!("{}: the worker refuses the job: {e}", c.name));
        let capture = GenTensorCaptureV1::of(&honest).expect("a capture");
        assert_eq!(capture.rebuild(&held).unwrap().binding, honest.binding, "{}: the capture is the run's commitments", c.name);
        // A planted lie at a step leaf: consistent, so only the court can tell; the cone close there convicts.
        let leaves = honest.leaf_listing();
        let site = leaves.iter().find(|l| l.lanes > 0).expect("a leaf with lanes");
        let lying = honest.planted(&GenPlantV1::StepLane { leaf: site.global, lane: 0, delta: 3 }).expect("the lie plants");
        assert_ne!(lying.execution_root(), honest.execution_root(), "{}: the lie changes the claim", c.name);
        let (proof, bytes) = cone_close(&held, &lying, site.global);
        let verdict = palw_gen_close_verdict_for_row_v1(
            &c.row,
            &c.row.class_id,
            &lying.execution_root(),
            &proof,
            Some(site.global),
            &court(),
            FORM,
            false,
        );
        assert_eq!(verdict, Ok(PalwCourtVerdictV2::ExecutorGuilty), "{}: the cone close convicts the planted lane", c.name);
        // The honest run is acquitted at the same leaf.
        let (honest_proof, _) = cone_close(&held, &honest, site.global);
        let acquit = palw_gen_close_verdict_for_row_v1(
            &c.row,
            &c.row.class_id,
            &honest.execution_root(),
            &honest_proof,
            Some(site.global),
            &court(),
            FORM,
            false,
        );
        assert_eq!(acquit, Ok(PalwCourtVerdictV2::ChallengerDefeated), "{}: an honest claim is not convicted", c.name);
        // The output lie: the step tree honest, the claimed output not its own.
        let lying_output = honest.planted(&GenPlantV1::OutputLane { lane: 0, delta: 1 }).expect("the output lie plants");
        assert_ne!(lying_output.execution_root(), honest.execution_root());
        eprintln!(
            "{}: class {} — {} step leaves; cone close at leaf {} is {bytes} bytes ({} the one-move carrier's {one_move_max})",
            c.name,
            c.row.class_id,
            honest.execution.space.leaf_count(),
            site.global,
            if bytes <= one_move_max { "within" } else { "OVER" },
        );
        // The premise of the two kinds of class: toy classes fit the one-move accusation; the wide one does not
        // and is the held leaf challenge's (decision 22).
        if c.name == "wide-embed" {
            assert!(bytes > PALW_OBJECT_CHUNK_MAX_BYTES, "wide-embed's cone close ({bytes} bytes) must exceed one carrier");
            assert!(bytes < 3_200_000, "…and fit the carried cap (PALW-TIR-38's 3.2 MB)");
        } else {
            assert!(bytes <= one_move_max, "{}: a toy class's cone close ({bytes} bytes) fits one carrier", c.name);
        }
    }
}

#[test]
fn the_wide_class_is_refused_below_the_held_leaf_challenge() {
    // The same class under a ruleset that does not arm `palw_held_close_chunks_v1` is refused by PALW-GEN-21: a
    // close it cannot carry in one move would be a lie nobody can be slashed for.
    let mut p = palw_t12_shipped_params();
    (PALW_T12_TIR_V1_ENTRY.set)(&mut p, Some(ForkActivation::new(1)));
    (PALW_DRILL_GEN_V1_ENTRY.set)(&mut p, Some(ForkActivation::new(1)));
    let wide = wide_embed();
    // `wide_embed()` was declared under `all_armed()`; ask the gate again under `p`.
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { unreachable!() };
    let refused = gen_class_admission_offline_v1(&GenOfflineGateV1::of(&p), bundle, &wide.declared.class, wide.declared.artifact_root);
    assert!(refused.is_err(), "wide-embed is not registrable before palw_held_close_chunks_v1: {refused:?}");
}

#[test]
#[ignore = "writes the drill's artifacts: PALW_GEN_DRILL_OUT=<dir> cargo test -p misaka-palw-sdk --test gen_drill_classes -- --ignored --nocapture"]
fn write_the_drill_classes() {
    let dir = PathBuf::from(std::env::var_os("PALW_GEN_DRILL_OUT").expect("set PALW_GEN_DRILL_OUT to a directory"));
    std::fs::create_dir_all(&dir).expect("the output directory");
    let mut manifest = Vec::new();
    for c in [toy_image(), toy_embed(), wide_embed()] {
        let admitted =
            c.declared.admission.as_ref().unwrap_or_else(|why| panic!("{}: the registration gate refuses the class: {why}", c.name));
        let file = dir.join(format!("{}.class.palwtir2", c.name));
        let digest = gen_write_declared_container_v1(&file, &c.declared, &c.weights, format!("{{\"model_id\":\"{}\"}}", c.model_id))
            .expect("the container");
        let bytes = std::fs::metadata(&file).expect("written").len();
        let (held, _guard) = held(&c);
        let honest = held.run_tensor(&c.job, &c.ids, &[], &[], FORM).expect("the job runs");
        let capture = GenTensorCaptureV1::of(&honest).expect("a capture");
        let material = misaka_palw_base0::gen_tensor_worker::gen_tensor_material_encode_v1(&capture);
        manifest.push(serde_json::json!({
            "name": c.name,
            "model_id": c.model_id,
            "file": file.file_name().unwrap().to_string_lossy(),
            "bytes": bytes,
            "file_digest": digest.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "class_id": c.row.class_id.to_string(),
            "artifact_root": c.row.artifact_root.to_string(),
            "profile": format!("{:?}", c.declared.class.profile),
            "canonical_step_leaves": admitted.entry.canonical_step_leaf_count,
            "max_step_leaves": admitted.entry.max_step_leaf_count,
            "sample_job_step_leaves": honest.execution.space.leaf_count(),
            "sample_material_bytes": material.len(),
            "gen_claim_request_template": c.request,
        }));
        eprintln!("wrote {} ({bytes} bytes), class {}", file.display(), c.row.class_id);
    }
    std::fs::write(dir.join("drill-classes.json"), serde_json::to_string_pretty(&serde_json::json!({ "classes": manifest })).unwrap())
        .expect("the manifest");
}
