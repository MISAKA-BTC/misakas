//! **The image-generation request as a canonical generative job** (RFC-0003 §II.1 + §I.1; delivery task 6): `POST /v1/images/generations`'s
//! entrance, in the gateway's discipline — parse a request, refuse by name what the class does not offer, canonicalise it into the
//! `PalwGenJobV1` the chain's acceptance re-reads, run it on a generative worker, and commit the output.
//!
//! ```text
//!  request ─▶ parse (refuse by name) ─▶ plan: one PalwGenJobV1 per image  ─▶ palw_gen_job_resolve_class_v1 (the chain's own acceptance)
//!                                           │   seed = the requester's, or H(request) — never a beacon, never a clock
//!                                           │   image_index k = R's `position`
//!                                           ▼
//!   worker: run_tensor(job, prompt ids, negative ids) ─▶ stage roots ─▶ step root ─▶ canonical output bytes ─▶ output_root
//!                                           ▼
//!   commitment: FP job version 10 (palw_gen_payload_v1): trace_root = step root, output_root, execution_root of its own parts
//! ```
//!
//! **The generation seed R is the requester's, or a pure function of the request.** RFC-0003 §I.1: R's key is the job's `seed` and
//! `image_index` is R's position, so the same (class, prompt, parameters, seed, index) is the same image in any job. Nothing of the
//! chain enters it — not the anchor, not a nonce, not a block hash — and it is never `misaka-palw-challenge`'s verification beacon,
//! which is drawn AFTER a claim exists to pick who checks it. A seed R that depended on that beacon would let the checker's draw
//! shape the work being checked; a seed that depends on nothing but the request cannot.
//!
//! **What this module is not.** The gateway binary holds no generative class and has no generative worker process to talk to, so there is no
//! HTTP route that runs a job: this is the library the route calls (DORMANT_NOT_INTEGRATED), and `POST /v1/images/generations` answers 501
//! by name until a class is configured. Prompts are token ids: the entrance has no image-class tokenizer (the FP workers tokenize; no
//! generative worker does yet), so a text `prompt` is refused with what to send instead.
#![allow(dead_code)]

use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PROMPT_MODE_USER, PalwFpCommitmentTxPayloadV3};
use kaspa_consensus_core::palw_gen_class_v1::PalwGenClassRecordV1;
use kaspa_consensus_core::palw_gen_close_v1::PalwGenTensorBindingV1;
use kaspa_consensus_core::palw_gen_job_v1::{
    PalwGenBodyV1, PalwGenImageBodyV1, PalwGenJobV1, PalwJobEnvelopeV1, palw_gen_job_resolve_class_v1, PALW_GEN_JOB_VERSION_V1,
};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use misaka_palw_base0::gen_tensor_worker::GenTensorWorkV1;
use misaka_palw_base0::gen_worker::GenHeldClassV1;
use misaka_palw_tir::pipeline::PipelineParams;

/// Most images one request may ask for (each is its own job, claim and run).
pub const MAX_IMAGES_PER_REQUEST: u16 = 4;
const SEED_DOMAIN: &[u8] = b"misaka-palw/gateway/image-seed/v1";
const KNOWN_KEYS: &[&str] = &["prompt_token_ids", "negative_prompt_token_ids", "n", "size", "steps", "guidance_scale", "seed", "model", "response_format", "user"];

/// What a person asked for, parsed (nothing canonicalised yet).
#[derive(Clone, Debug, PartialEq)]
pub struct ImageRequest {
    pub prompt_ids: Vec<u32>,
    pub negative_ids: Vec<u32>,
    pub n: u16,
    pub size: Option<(u32, u32)>,
    pub steps: Option<u32>,
    pub guidance_scale: Option<f64>,
    pub seed: Option<[u8; 32]>,
}

fn ids(body: &serde_json::Value, key: &str) -> Result<Vec<u32>, String> {
    match body.get(key) {
        None | Some(serde_json::Value::Null) => Ok(Vec::new()),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, v)| v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| format!("{key}[{i}] is not a token id (a u32)")))
            .collect(),
        Some(_) => Err(format!("{key} must be an array of token ids")),
    }
}

/// **Parse an image-generation request**, refusing by name: a text `prompt` (no tokenizer here), a field this lane has no rule for, a size
/// that is not `WxH`, a seed that is not 64 hex characters, a count outside `1..=4`.
pub fn parse(body: &serde_json::Value) -> Result<ImageRequest, String> {
    let object = body.as_object().ok_or("the request body is not a JSON object")?;
    if object.contains_key("prompt") {
        return Err("`prompt` (text) is not accepted: this entrance holds no image-class tokenizer, so a person's words cannot be turned into the \
                    class's token ids here. Send `prompt_token_ids` (the class tokenizer's ids without the class's template tokens)"
            .to_string());
    }
    for key in object.keys() {
        if !KNOWN_KEYS.contains(&key.as_str()) {
            return Err(format!("`{key}` is not a field of an image request on this lane ({})", KNOWN_KEYS.join(", ")));
        }
    }
    let n = match object.get("n") {
        None => 1,
        Some(v) => v.as_u64().filter(|n| (1..=u64::from(MAX_IMAGES_PER_REQUEST)).contains(n)).ok_or(format!("n must be 1..={MAX_IMAGES_PER_REQUEST}"))? as u16,
    };
    let size = match object.get("size") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => {
            let text = v.as_str().ok_or("size must be a string like \"16x16\"")?;
            let (w, h) = text.split_once('x').ok_or("size must be WxH, like \"16x16\"")?;
            Some((h.parse::<u32>().map_err(|_| "size height is not a number")?, w.parse::<u32>().map_err(|_| "size width is not a number")?))
        }
    };
    let steps = match object.get("steps") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => Some(v.as_u64().and_then(|s| u32::try_from(s).ok()).ok_or("steps must be a whole number")?),
    };
    let guidance_scale = match object.get("guidance_scale") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => Some(v.as_f64().filter(|g| g.is_finite() && *g >= 0.0).ok_or("guidance_scale must be a non-negative number")?),
    };
    let seed = match object.get("seed") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => {
            let text = v.as_str().ok_or("seed must be 64 hex characters")?;
            let mut out = [0u8; 32];
            if text.len() != 64 || faster_hex::hex_decode(text.as_bytes(), &mut out).is_err() {
                return Err("seed must be 64 hex characters".to_string());
            }
            Some(out)
        }
    };
    Ok(ImageRequest { prompt_ids: ids(body, "prompt_token_ids")?, negative_ids: ids(body, "negative_prompt_token_ids")?, n, size, steps, guidance_scale, seed })
}

/// **R's key for a request that named none**: a pure function of the request's canonical digest. Deterministic, so a retry is the same image;
/// the chain, the clock and every beacon are absent from its inputs.
pub fn derived_seed(request_digest: &Hash64) -> [u8; 32] {
    let digest = kaspa_hashes::blake2b_512_keyed(SEED_DOMAIN, request_digest.as_byte_slice());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest.as_byte_slice()[..32]);
    out
}

/// The chain-facing fields a job needs (the same envelope the text lane fills).
#[derive(Clone, Debug)]
pub struct Envelope {
    pub network_domain: Hash64,
    pub executor_bond: TransactionOutpoint,
    pub executor_pubkey: Vec<u8>,
    pub operator_id: Hash64,
    pub anchor_block: Hash64,
    pub anchor_daa: u64,
    /// One nonce per job (uniqueness only; index `k` of the request uses `job_nonces[k]`).
    pub job_nonces: Vec<[u8; 32]>,
    pub privacy_mode: u8,
}

/// What the plan applied beyond what was asked — the response says it.
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    pub steps: u32,
    pub width: u32,
    pub height: u32,
    pub guidance_q: u16,
    pub seed_source: &'static str,
}

/// **Plan the jobs**: one `PalwGenJobV1` per requested image, each held to the class's offers by the chain's own acceptance
/// ([`palw_gen_job_resolve_class_v1`]) before anything runs.
pub fn plan(
    request: &ImageRequest,
    row: &PalwGenClassRecordV1,
    envelope: &Envelope,
    form: PalwPromptIdsFormV1,
    request_digest: &Hash64,
) -> Result<(Vec<PalwGenJobV1>, Applied), String> {
    use kaspa_consensus_core::palw_gen_class_v1::PalwGenProfileOffersV1;
    let class = &*row.class;
    let PalwGenProfileOffersV1::Image(image_offers) = &class.offers.profile else {
        return Err("this class is not an image class: its offers carry no image profile".to_string());
    };
    if request.prompt_ids.len() as u32 > class.offers.max_prompt_tokens {
        return Err(format!("{} prompt ids exceed the class's {}", request.prompt_ids.len(), class.offers.max_prompt_tokens));
    }
    if envelope.job_nonces.len() < request.n as usize {
        return Err("one job nonce per requested image".to_string());
    }
    // One resolution per class: the output's [H, W, 3] is the class's, and a request that names another is refused, not resized.
    let shape = &class.output.shape;
    let (height, width) = match shape.as_slice() {
        [h, w, 3] => (*h, *w),
        _ => return Err("the class's output is not an RGB image".to_string()),
    };
    if let Some((rh, rw)) = request.size
        && (rh, rw) != (height, width)
    {
        return Err(format!("size {rw}x{rh} is not offered: this class makes {width}x{height} images (one resolution per class)"));
    }
    let steps = match request.steps {
        Some(s) if class.offers.steps.contains(&s) => s,
        Some(s) => return Err(format!("steps {s} is not offered: this class offers {:?}", class.offers.steps)),
        None => *class.offers.steps.iter().min().ok_or("the class offers no step count")?,
    };
    let guidance_q = match (request.guidance_scale, &image_offers.guidance) {
        (None, None) => 0,
        (None, Some(g)) => g.lo,
        (Some(_), None) => return Err("guidance_scale is not offered: this class's programs read no guidance".to_string()),
        (Some(scale), Some(g)) => {
            let q = (scale * 16.0).round();
            if !(f64::from(g.lo)..=f64::from(g.hi)).contains(&q) {
                return Err(format!("guidance_scale {scale} is outside what the class offers ({}..={} in 1/16 units)", g.lo, g.hi));
            }
            q as u16
        }
    };
    let (seed, seed_source) = match request.seed {
        Some(seed) => (seed, "the request's"),
        None => (derived_seed(request_digest), "a pure function of the request (no chain input, no beacon)"),
    };
    let prompt_hash = if request.prompt_ids.is_empty() {
        Hash64::default()
    } else {
        prompt_token_ids_commitment_v1(form, &request.prompt_ids).map_err(|e| format!("the prompt does not commit in this network's form: {e:?}"))?
    };
    let negative_hash = if request.negative_ids.is_empty() {
        Hash64::default()
    } else {
        prompt_token_ids_commitment_v1(form, &request.negative_ids).map_err(|e| format!("the negative prompt does not commit: {e:?}"))?
    };
    let mut jobs = Vec::with_capacity(request.n as usize);
    for k in 0..request.n {
        let job = PalwGenJobV1 {
            version: PALW_GEN_JOB_VERSION_V1,
            envelope: PalwJobEnvelopeV1 {
                network_domain: envelope.network_domain,
                class_id: row.class_id,
                executor_bond: envelope.executor_bond,
                executor_pubkey: envelope.executor_pubkey.clone(),
                operator_id: envelope.operator_id,
                anchor_block: envelope.anchor_block,
                anchor_daa: envelope.anchor_daa,
                job_nonce: envelope.job_nonces[k as usize],
                privacy_mode: envelope.privacy_mode,
                prompt_mode: PALW_FP_PROMPT_MODE_USER,
            },
            seed,
            body: PalwGenBodyV1::Image(PalwGenImageBodyV1 {
                prompt_token_ids_hash: prompt_hash,
                prompt_tokens: request.prompt_ids.len() as u32,
                negative_token_ids_hash: negative_hash,
                negative_tokens: request.negative_ids.len() as u32,
                guidance_q,
                image_index: k,
                sampler_id: image_offers.sampler_id,
                steps: u16::try_from(steps).map_err(|_| "steps do not fit a job".to_string())?,
                width: u16::try_from(width).map_err(|_| "width does not fit a job".to_string())?,
                height: u16::try_from(height).map_err(|_| "height does not fit a job".to_string())?,
                output: misaka_palw_gen::OutputKindV1::ImageRgb8.tag(),
            }),
        };
        // The chain's own acceptance, before a single step runs: every field against the class's offers, the seed rule, the modes.
        palw_gen_job_resolve_class_v1(&job, row).map_err(|e| format!("image {k}: the job is not the class's: {e}"))?;
        jobs.push(job);
    }
    Ok((jobs, Applied { steps, width, height, guidance_q, seed_source }))
}

/// One finished image and its claim.
pub struct ImageClaim {
    pub job: PalwGenJobV1,
    pub work: GenTensorWorkV1,
    pub binding: PalwGenTensorBindingV1,
    /// The canonical output bytes (header + `u8` HWC RGB): raw tensor bytes, not a PNG (RFC-0003 §I.3.4).
    pub canonical_bytes: Vec<u8>,
    pub payload: PalwFpCommitmentTxPayloadV3,
}

/// **Run one planned job and commit its output**: the stage roots, the step root, the canonical output and its root, and the lane's
/// version-10 payload over them (the signature is the caller's — the rail's sidecar).
pub fn run<P: PipelineParams>(
    held: &GenHeldClassV1<P>,
    job: &PalwGenJobV1,
    request: &ImageRequest,
    form: PalwPromptIdsFormV1,
    signature: Vec<u8>,
) -> Result<ImageClaim, String> {
    let work = held.run_tensor(job, &request.prompt_ids, &request.negative_ids, &[], form)?;
    let binding = work.binding.clone();
    let output = work.output();
    let canonical_bytes = output.spec.canonical_bytes(&output.values).map_err(|e| format!("the output has no canonical bytes: {e:?}"))?;
    let carried: Vec<u32> = if job.envelope.privacy_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA {
        request.prompt_ids.iter().chain(&request.negative_ids).copied().collect()
    } else {
        Vec::new()
    };
    let payload = kaspa_consensus_core::palw_gen_claim_v1::palw_gen_payload_v1(
        job,
        binding.step_leaf_count,
        binding.step_root(),
        binding.output_root,
        carried,
        signature,
    );
    Ok(ImageClaim { job: job.clone(), work, binding, canonical_bytes, payload })
}

/// The OpenAI-shaped response for finished images: raw RGB bytes as hex (a canonical PNG is not a consensus object), each with its claim.
pub fn response_body(claims: &[ImageClaim], applied: &Applied) -> serde_json::Value {
    let hex = |h: &Hash64| faster_hex::hex_string(h.as_byte_slice());
    serde_json::json!({
        "created": 0,
        "data": claims.iter().map(|c| {
            let claim_id = kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&c.payload.commitment);
            serde_json::json!({
                "format": "rgb8_hwc_hex",
                "height": applied.height,
                "width": applied.width,
                "rgb8_hex": faster_hex::hex_string(&c.canonical_bytes),
                "misaka": {
                    "status": crate::status::RequestStatus::Committed.as_str(),
                    "final": false,
                    "fp_claim_id": hex(&claim_id),
                    "gen_job_id": hex(&c.job.id()),
                    "output_root": hex(&c.binding.output_root),
                    "execution_root": hex(&c.binding.committed_execution_root),
                    "image_index": match &c.job.body { PalwGenBodyV1::Image(b) => b.image_index, _ => 0 },
                },
            })
        }).collect::<Vec<_>>(),
        "misaka": {
            "applied": {
                "steps": applied.steps, "width": applied.width, "height": applied.height,
                "guidance_q": applied.guidance_q, "seed": applied.seed_source,
            },
            "note": "the images are canonical raw tensors; each is a claim only once the rail has submitted its commitment (FP job version 10, dormant behind palw_fp_job_v5)",
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
    use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PUBLIC_DA, fp_claim_id_v3};
    use kaspa_consensus_core::palw_gen_class_v1::*;
    use kaspa_consensus_core::palw_gen_v1::{PALW_DRILL_GEN_V1_ENTRY, PalwGenProfileV1};
    use kaspa_consensus_core::palw_held_close_v1::PALW_HELD_CLOSE_CHUNKS_ENTRY_V1;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_tir_v1::PALW_T12_TIR_V1_ENTRY;
    use kaspa_consensus_core::tx::TransactionId;
    use misaka_palw_base0::gen_tensor_worker::gen_tensor_seat_judge_v1;
    use misaka_palw_base0::gen_worker::GenSeatJudgmentV1;
    use misaka_palw_gen::OutputSpecV1;
    use misaka_palw_sdk::gen_class::*;
    use misaka_palw_tir::interp::ParamSource;
    use misaka_palw_tir_lower::diffusion::fixture::{COUNTS, L, build_sd3_tiny, fixture};

    const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;
    const IMAGE_HW: u32 = 16;

    struct Weights(Vec<Box<dyn ParamSource + Send + Sync>>);
    impl PipelineParams for Weights {
        fn params(&self, program: u16) -> &dyn ParamSource {
            self.0[program as usize].as_ref()
        }
    }

    struct Class {
        row: PalwGenClassRecordV1,
        declared: GenDeclaredV1,
        weights: Weights,
    }

    /// The SDK's own tiny SD3 class (`misaka-palw-sdk/tests/gen_sd3_class.rs`): lowered from the generated fixture, declared under a layout,
    /// registrable by construction. `None` (with a line) where the fixture has not been generated.
    fn sd3() -> Option<Class> {
        let dir = fixture()?;
        let fx = build_sd3_tiny(&dir).expect("the fixture lowers");
        let pipe = &fx.pipeline;
        let weights = Weights(pipe.params.iter().cloned().map(|p| Box::new(p) as Box<dyn ParamSource + Send + Sync>).collect());
        let spec = GenClassSpecV1 {
            profile: PalwGenProfileV1::Image,
            pipeline: pipe.pipeline.clone(),
            programs: pipe.programs.clone(),
            tokenizer_id: Hash64::from_bytes([0x73; 64]),
            output: OutputSpecV1::image_rgb8(IMAGE_HW, IMAGE_HW),
            offers: PalwGenOffersV1 {
                steps: COUNTS.to_vec(),
                profile: PalwGenProfileOffersV1::Image(PalwGenImageOffersV1 { sampler_id: Hash64::from_bytes([0x5D; 64]), guidance: None, steps_scalar: Some(0) }),
                scalars: vec![PalwGenScalarOfferV1 { lo: 0, hi: COUNTS.len() as i64 - 1 }],
                max_prompt_tokens: (L - 2) as u32,
                max_negative_tokens: 0,
                images: vec![],
                max_source_tokens: 0,
                forced_prompt_prefix: vec![],
                source_token_floor: 0,
            },
        };
        let mut params: Params = palw_t12_shipped_params();
        (PALW_T12_TIR_V1_ENTRY.set)(&mut params, Some(ForkActivation::new(1)));
        (PALW_DRILL_GEN_V1_ENTRY.set)(&mut params, Some(ForkActivation::new(1)));
        (PALW_HELD_CLOSE_CHUNKS_ENTRY_V1.set)(&mut params, Some(ForkActivation::new(1)));
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
        let declared = gen_declare_layout_v1(&params, bundle, &spec, &weights, &GenLayoutChoiceV1 { tile_len: 16, output_tile: None, h_chunk: 16, checkpoint_interval: 1 })
            .unwrap_or_else(|e| panic!("declare: {e}"));
        let row = declared.row.clone().expect("the class derives a registry row");
        Some(Class { row, declared, weights })
    }

    fn held(c: &Class, tag: &str) -> (GenHeldClassV1<misaka_palw_tir_artifact::PalwTirContainerV2>, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("misaka-gw-imagegen-{}-{tag}.palwtir2", std::process::id()));
        gen_write_declared_container_v1(&path, &c.declared, &c.weights, "{\"model_id\":\"palw-fixture/sd3-tiny\"}".to_string()).expect("the container");
        (GenHeldClassV1::hold_container(c.row.clone(), &path).expect("the worker holds the class"), path)
    }

    fn envelope(nonces: usize, anchor: u64) -> Envelope {
        Envelope {
            network_domain: Hash64::from_bytes([0xD0; 64]),
            executor_bond: TransactionOutpoint::new(TransactionId::from_bytes([7; 64]), 0),
            executor_pubkey: vec![1; kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN],
            operator_id: Hash64::from_bytes([2; 64]),
            anchor_block: Hash64::from_u64_word(anchor),
            anchor_daa: 100 + anchor,
            job_nonces: (0..nonces).map(|i| [i as u8 + 1; 32]).collect(),
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        }
    }

    fn request(extra: serde_json::Value) -> serde_json::Value {
        let mut base = serde_json::json!({ "prompt_token_ids": [5, 9, 13], "n": 2, "seed": "33".repeat(32) });
        for (k, v) in extra.as_object().unwrap() {
            base[k] = v.clone();
        }
        base
    }

    #[test]
    fn the_request_is_parsed_and_every_unoffered_thing_is_refused_by_name() {
        let ok = parse(&request(serde_json::json!({}))).unwrap();
        assert_eq!((ok.n, ok.prompt_ids.clone(), ok.seed), (2, vec![5, 9, 13], Some([0x33; 32])));
        let refusals = [
            (serde_json::json!({ "prompt": "a red cat" }), "tokenizer"),
            (serde_json::json!({ "style": "x" }), "not a field"),
            (serde_json::json!({ "n": 0 }), "n must be"),
            (serde_json::json!({ "n": 5 }), "n must be"),
            (serde_json::json!({ "seed": "xyz" }), "64 hex"),
            (serde_json::json!({ "size": "16" }), "WxH"),
            (serde_json::json!({ "prompt_token_ids": [-1] }), "not a token id"),
            (serde_json::json!({ "guidance_scale": -1.0 }), "non-negative"),
        ];
        for (extra, needle) in refusals {
            let err = parse(&request(extra.clone())).unwrap_err();
            assert!(err.contains(needle), "{extra}: {err}");
        }
    }

    /// **The whole path, in process, on the SDK's tiny SD3 class**: the request becomes canonical jobs the chain's acceptance re-reads, the
    /// worker runs them, the image is committed, a seat replays it, and the claim is admissible to the (test-armed) tensor lane.
    #[test]
    fn an_image_request_becomes_canonical_jobs_runs_on_the_sd3_class_and_commits_a_tensor_claim_a_seat_replays() {
        let Some(class) = sd3() else { return };
        let (held, path) = held(&class, "run");
        let digest = Hash64::from_u64_word(0xD16E57);
        let parsed = parse(&request(serde_json::json!({ "steps": COUNTS[0] }))).unwrap();
        let (jobs, applied) = plan(&parsed, &class.row, &envelope(2, 0), FORM, &digest).expect("the jobs are the class's");
        assert_eq!(jobs.len(), 2);
        assert_eq!((applied.width, applied.height, applied.steps), (IMAGE_HW, IMAGE_HW, COUNTS[0]));
        // R: both images share the request's seed; the index is R's position.
        let seeds: Vec<_> = jobs.iter().map(|j| j.seed).collect();
        assert_eq!(seeds, vec![[0x33; 32], [0x33; 32]]);
        let indices: Vec<u16> = jobs.iter().map(|j| match &j.body { PalwGenBodyV1::Image(b) => b.image_index, _ => unreachable!() }).collect();
        assert_eq!(indices, vec![0, 1]);
        assert_ne!(jobs[0].id(), jobs[1].id());
        let sig = vec![0x5A; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
        let claims: Vec<ImageClaim> = jobs.iter().map(|j| run(&held, j, &parsed, FORM, sig.clone()).expect("the worker runs it")).collect();
        for c in &claims {
            assert_eq!(c.binding.committed_execution_root, c.binding.execution_root());
            assert_eq!(c.payload.commitment.execution_root, c.binding.committed_execution_root, "the lane's execution root is the binding's");
            // The output is the canonical tensor: IMAGE_HW x IMAGE_HW x 3 bytes after the header.
            assert!(c.canonical_bytes.len() >= (IMAGE_HW * IMAGE_HW * 3) as usize);
            // A seat replays from the material the panel received and agrees.
            let judged = gen_tensor_seat_judge_v1(&held, &c.work.execution_root(), &c.job, &parsed.prompt_ids, &[], &[], FORM);
            assert_eq!(judged, GenSeatJudgmentV1::Valid);
        }
        // Different index, same seed: a different image (R's position is in the draw).
        assert_ne!(claims[0].canonical_bytes, claims[1].canonical_bytes, "image 0 and image 1 of one request are two images");
        assert_ne!(claims[0].binding.output_root, claims[1].binding.output_root);
        // Determinism: the same job runs to the same roots.
        let again = run(&held, &jobs[0], &parsed, FORM, sig.clone()).unwrap();
        assert_eq!((again.binding.committed_execution_root, again.binding.output_root), (claims[0].binding.committed_execution_root, claims[0].binding.output_root));
        assert_eq!(again.canonical_bytes, claims[0].canonical_bytes);
        // The tensor lane's stateless rules, test-armed: admissible to the door that carries the fence, refused by name by the one that does not.
        let payload = &claims[0].payload;
        let domain = payload.commitment.job.network_domain;
        let _ = domain;
        let bytes = borsh::to_vec(payload).unwrap();
        let ladder = 1 << 26;
        assert!(validate_gen_door(&bytes, ladder, true).is_ok(), "the tensor door admits it where the fence is armed");
        assert!(validate_gen_door(&bytes, ladder, false).is_err(), "and the lane's own door refuses version 10 where it is not");
        let response = response_body(&claims, &applied);
        assert_eq!(response["data"].as_array().unwrap().len(), 2);
        assert_eq!(response["data"][0]["misaka"]["status"], "committed");
        assert_eq!(response["data"][0]["misaka"]["final"], false);
        assert_eq!(response["data"][0]["misaka"]["fp_claim_id"], faster_hex::hex_string(fp_claim_id_v3(&claims[0].payload.commitment).as_byte_slice()).as_str());
        let _ = std::fs::remove_file(path);
    }

    fn validate_gen_door(bytes: &[u8], ladder: u64, gen_door: bool) -> Result<(), kaspa_consensus_core::palw_freeprompt_v3::PalwFpV3Error> {
        kaspa_consensus_core::palw_gen_claim_v1::validate_palw_fp_commitment_tx_gen_door_v1(
            bytes,
            false,
            FORM,
            ladder,
            kaspa_consensus_core::palw_freeprompt_v3::PalwFpDecodeRulesV1::Dormant,
            gen_door,
        )
    }

    /// **R is the requester's or a pure function of the request — never the chain's.** The anchor, the nonces and the DAA are not inputs of
    /// the seed; a changed request digest is a changed seed; a named seed is used verbatim.
    #[test]
    fn the_generation_seed_depends_on_the_request_alone_and_never_on_the_chain() {
        let Some(class) = sd3() else { return };
        let none = parse(&serde_json::json!({ "prompt_token_ids": [5, 9, 13] })).unwrap();
        let digest = Hash64::from_u64_word(1);
        let (a, applied) = plan(&none, &class.row, &envelope(1, 0), FORM, &digest).unwrap();
        assert!(applied.seed_source.contains("no chain input, no beacon"));
        // Move EVERYTHING the chain contributes to a job: the anchor block, its DAA, the nonce.
        let mut moved = envelope(1, 7);
        moved.job_nonces = vec![[0xEE; 32]];
        let (b, _) = plan(&none, &class.row, &moved, FORM, &digest).unwrap();
        assert_eq!(a[0].seed, b[0].seed, "the seed did not move with the chain");
        assert_ne!(a[0].id(), b[0].id(), "although the job did");
        assert_eq!(a[0].seed, derived_seed(&digest));
        assert_ne!(derived_seed(&digest), derived_seed(&Hash64::from_u64_word(2)), "another request, another seed");
        assert_ne!(a[0].seed, [0u8; 32], "a class that draws randomness is never handed the zero seed");
        // A named seed is verbatim, and another named seed is another image.
        let named = parse(&serde_json::json!({ "prompt_token_ids": [5, 9, 13], "seed": "44".repeat(32) })).unwrap();
        assert_eq!(plan(&named, &class.row, &envelope(1, 0), FORM, &digest).unwrap().0[0].seed, [0x44; 32]);
    }

    #[test]
    fn what_the_class_does_not_offer_is_refused_before_anything_runs() {
        let Some(class) = sd3() else { return };
        let d = Hash64::from_u64_word(1);
        let env = envelope(1, 0);
        let go = |extra: serde_json::Value| plan(&parse(&request(extra)).unwrap(), &class.row, &env, FORM, &d).map(|_| ());
        let bad = [
            (serde_json::json!({ "n": 1, "size": "32x32" }), "one resolution per class"),
            (serde_json::json!({ "n": 1, "steps": 3 }), "not offered"),
            (serde_json::json!({ "n": 1, "guidance_scale": 2.0 }), "read no guidance"),
            (serde_json::json!({ "n": 1, "prompt_token_ids": (0..40).collect::<Vec<u32>>() }), "exceed"),
            (serde_json::json!({ "n": 1, "negative_prompt_token_ids": [1] }), "not the class's"),
        ];
        for (extra, needle) in bad {
            let err = go(extra.clone()).unwrap_err();
            assert!(err.contains(needle), "{extra}: {err}");
        }
        assert!(go(serde_json::json!({ "n": 1 })).is_ok());
        // Two jobs need two nonces.
        assert!(plan(&parse(&request(serde_json::json!({ "n": 2 }))).unwrap(), &class.row, &envelope(1, 0), FORM, &d).is_err());
    }
}
