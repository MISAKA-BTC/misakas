//! **The vision-language path in the gateway** (RFC-0003 §II.2.1 + §II.4; delivery task 5): a person's picture and prompt become a V5 job, a
//! run on a pipeline class, and a claim whose every stage is bound — and the PREPROCESSING that made the picture the class's size is part of
//! what is bound.
//!
//! ```text
//!  raw picture (any size) ──preprocess──▶ canonical pixels at the slot's size ──▶ input_root (tiles, the class's tile_len)
//!        │                                   │                                         │
//!        └─ PreprocessRecordV1 (digest) ─────┘                                         ▼
//!  prompt ids ───────────────────────────────────────────────▶ FP Job V5 { v4, images: [input_root, h, w] } ── fp_job_id_v5
//!                                                                                      │
//!  vision stage (encoder) ──StageFinal edge──▶ text stage (decoder, TextStream) ──▶ generated ids
//!        stage root 0                                  stage root 1                      │
//!                                  step root = H(stage roots) ◀───────────────────────────┘
//!                                  execution root = H(job id, class id, leaf count, step root, generated ids)
//! ```
//!
//! **A text decoder alone is not VLM support.** The refusal that stood in `prepare_request` ("the class behind this gateway has no image
//! slots") is still right for the text worker; this module is the other door: it takes the class's declared slots, refuses a request whose
//! picture count is not the slot count, preprocesses each picture to its slot, builds the V5 job over the canonical pixels, and runs it on
//! a generative worker (`misaka_palw_base0::gen_worker`).
//!
//! What is bound, link by link, and where:
//!
//! | link | bound by |
//! |---|---|
//! | raw picture → canonical pixels | the [`PreprocessRecordV1`] digest, sealed in the [`VlmReceiptV1`] (NOT consensus: RFC-0003 §II.4 puts resampling outside it) |
//! | canonical pixels → job | `images[k].input_root` inside the V5 job, so inside `fp_job_id_v5` and the claim id |
//! | job → encoder → decoder | the stage roots (one per stage, edges carried under PALW-TIR-33) → the step root → the execution root |
//! | decoder → output | the generated ids inside the execution root |
//! | claim → user | the receipt: job id, class id, execution root, generated-ids digest, every record digest, the request digest |
//!
//! The chain's fold does not yet open V5 claims (`palw_fp_job_v5` is dormant everywhere; the free-prompt walk skips version 8), so the end of
//! this path in the tests is the lane's V5 stateless validator, test-armed, and a seat's replay — the same two checks a V5 lane would apply
//! first.
//!
//! The gateway BINARY has no generative worker route yet, so nothing in `main.rs` calls this module: it is the library the VLM route will
//! (DORMANT_NOT_INTEGRATED), exercised in process by the tests below.
#![allow(dead_code)]

use kaspa_consensus_core::palw_fp_job_v5::{PalwFreePromptJobV5, fp_job_id_v5};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PROMPT_MODE_USER, PALW_FP_V4_VERSION, PalwFpCommitmentTxPayloadV3, PalwFreePromptCommitmentV3, PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_gen_class_v1::{PalwGenClassRecordV1, PalwGenImageInputRefV1};
use kaspa_consensus_core::palw_gen_close_v1::PalwGenStepBindingV1;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;

use crate::preprocess::{Fit, PreprocessRecordV1, preprocess};
use crate::tensor::DecodedImageV1;

const DOMAIN_RECEIPT: &[u8] = b"misaka-palw/gateway/vlm-receipt/v1";
const DOMAIN_IDS: &[u8] = b"misaka-palw/gateway/vlm-generated-ids/v1";

/// The chain-facing fields a job needs besides the class and the content.
#[derive(Clone, Debug)]
pub struct JobEnvelope {
    pub network_domain: Hash64,
    pub executor_bond: TransactionOutpoint,
    pub executor_pubkey: Vec<u8>,
    pub operator_id: Hash64,
    pub anchor_block: Hash64,
    pub anchor_daa: u64,
    pub job_nonce: [u8; 32],
    pub privacy_mode: u8,
}

/// What a vision-language request is made of.
pub struct VlmRequest<'a> {
    pub envelope: JobEnvelope,
    pub row: &'a PalwGenClassRecordV1,
    /// The prompt's ids under the class tokenizer (the generative worker's frame carries ids; tokenizing is the caller's).
    pub prompt_ids: &'a [u32],
    pub form: PalwPromptIdsFormV1,
    pub decode_token_limit: u32,
    pub max_context_tokens: u32,
    /// The person's pictures, in the order the messages carried them (slot order).
    pub raw_images: &'a [DecodedImageV1],
    pub fit: Fit,
}

/// A planned job: ready to run, with the record of how each picture became its slot's pixels.
pub struct VlmPlan {
    pub job: PalwFreePromptJobV5,
    pub request: misaka_palw_base0::gen_worker::PalwGenWorkerRequestV1,
    pub records: Vec<PreprocessRecordV1>,
    pub canonical_images: Vec<DecodedImageV1>,
}

/// **Plan a vision-language job**: refuse the wrong number of pictures, preprocess each to its slot, build the V5 job over the canonical
/// pixels at each slot's own tile length. Greedy and no-op decode (the unit-free controls only: V5 is offered no Q24-dependent control
/// until the fence that guarantees Q24 logits opens).
pub fn plan(request: &VlmRequest<'_>) -> Result<VlmPlan, String> {
    let slots = &request.row.class.offers.images;
    if slots.is_empty() {
        return Err("this class declares no image slot: a text class takes FP Job V4 only (RFC-0003 open question 14)".to_string());
    }
    if request.raw_images.len() != slots.len() {
        return Err(format!(
            "the request carries {} picture(s) and the class declares {} image slot(s): exactly one picture per slot, in slot order",
            request.raw_images.len(),
            slots.len()
        ));
    }
    if request.prompt_ids.is_empty() {
        return Err("a vision-language prompt carries at least one id".to_string());
    }
    let mut canonical_images = Vec::with_capacity(slots.len());
    let mut records = Vec::with_capacity(slots.len());
    for (k, (raw, slot)) in request.raw_images.iter().zip(slots).enumerate() {
        let (canonical, record) = preprocess(raw, slot.h, slot.w, request.fit).map_err(|e| format!("picture {k}: {e}"))?;
        canonical_images.push(canonical);
        records.push(record);
    }
    let tile_lens: Vec<u32> = slots.iter().map(|s| s.tile_len).collect();
    let e = &request.envelope;
    let v4 = PalwFreePromptJobV3 {
        version: PALW_FP_V4_VERSION,
        network_domain: e.network_domain,
        class_id: request.row.class_id,
        executor_bond: e.executor_bond,
        executor_pubkey: e.executor_pubkey.clone(),
        operator_id: e.operator_id,
        anchor_block: e.anchor_block,
        anchor_daa: e.anchor_daa,
        job_nonce: e.job_nonce,
        tokenizer_id: request.row.tokenizer_id,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(request.form, request.prompt_ids)
            .map_err(|err| format!("the prompt does not commit in this network's form: {err:?}"))?,
        prompt_tokens: request.prompt_ids.len() as u32,
        decode_token_limit: request.decode_token_limit,
        max_context_tokens: request.max_context_tokens,
        privacy_mode: e.privacy_mode,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: Some(kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4::NOOP),
        tail: None,
    };
    let job = crate::tensor::v5_job_v1(v4, &canonical_images, &tile_lens)?;
    let worker_request = crate::tensor::gen_worker_request_v1(job.clone(), request.prompt_ids.to_vec(), &canonical_images);
    Ok(VlmPlan { job, request: worker_request, records, canonical_images })
}

/// **The commitment a run's binding makes** on the free-prompt lane: the V5 job carried as version 8, the step root as the trace root, the
/// binding's execution root, the generated count, the leaf count as the price. No output root (RFC-0003 §I.3.3: a text pipeline's
/// canonical output is the committed ids, which the execution root already hashes), one trace chunk and no DA manifest (the lane's
/// retention for V5 is open: DESIGN_GAP, recorded in the delivery record).
pub fn commitment_of(job: &PalwFreePromptJobV5, binding: &PalwGenStepBindingV1, retention_daa: u64) -> PalwFreePromptCommitmentV3 {
    let carried = job.into_carried();
    let facts = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_run_facts_for_executed_v1(&carried, binding.generated.len() as u32);
    PalwFreePromptCommitmentV3 {
        job: carried,
        trace_root: binding.step_root(),
        output_root: Hash64::default(),
        schedule_root: Hash64::default(),
        execution_root: binding.committed_execution_root,
        decode_tokens_executed: facts.decode_tokens_executed,
        stop_reason: facts.stop_reason,
        work_leaves: binding.step_leaf_count,
        trace_manifest_root: Hash64::default(),
        trace_chunk_count: 1,
        trace_retention_daa: retention_daa,
    }
}

/// The payload a rail would sign and carry (the signature is the caller's; `prompt_token_ids` ride only under `PublicDa`).
pub fn payload_of(commitment: PalwFreePromptCommitmentV3, prompt_ids: &[u32], signature: Vec<u8>) -> PalwFpCommitmentTxPayloadV3 {
    let public = commitment.job.privacy_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA;
    PalwFpCommitmentTxPayloadV3 {
        version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
        commitment,
        prompt_token_ids: if public { prompt_ids.to_vec() } else { Vec::new() },
        signature,
    }
}

fn ids_digest(ids: &[u32]) -> Hash64 {
    let mut pre = (ids.len() as u64).to_le_bytes().to_vec();
    for id in ids {
        pre.extend_from_slice(&id.to_le_bytes());
    }
    kaspa_hashes::blake2b_512_keyed(DOMAIN_IDS, &pre)
}

/// What the user keeps of a vision-language answer: the claim's identities and, per picture, the digest of how it was preprocessed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VlmReceiptV1 {
    pub job_id: Hash64,
    pub class_id: Hash64,
    pub execution_root: Hash64,
    pub generated_ids_hash: Hash64,
    pub generated_tokens: u32,
    pub image_input_roots: Vec<Hash64>,
    pub preprocess_digests: Vec<Hash64>,
    pub request_digest: Hash64,
    pub receipt_id: Hash64,
}

impl VlmReceiptV1 {
    fn id_of(&self) -> Hash64 {
        let mut pre = Vec::new();
        for h in [self.job_id, self.class_id, self.execution_root, self.generated_ids_hash, self.request_digest] {
            pre.extend_from_slice(h.as_byte_slice());
        }
        pre.extend_from_slice(&self.generated_tokens.to_le_bytes());
        for list in [&self.image_input_roots, &self.preprocess_digests] {
            pre.extend_from_slice(&(list.len() as u64).to_le_bytes());
            for h in list.iter() {
                pre.extend_from_slice(h.as_byte_slice());
            }
        }
        kaspa_hashes::blake2b_512_keyed(DOMAIN_RECEIPT, &pre)
    }

    pub fn seal(plan: &VlmPlan, binding: &PalwGenStepBindingV1, request_digest: Hash64) -> Self {
        let mut receipt = Self {
            job_id: fp_job_id_v5(&plan.job),
            class_id: plan.job.v4.class_id,
            execution_root: binding.committed_execution_root,
            generated_ids_hash: ids_digest(&binding.generated),
            generated_tokens: binding.generated.len() as u32,
            image_input_roots: plan.job.images.iter().map(|i| i.input_root).collect(),
            preprocess_digests: plan.records.iter().map(PreprocessRecordV1::digest).collect(),
            request_digest,
            receipt_id: Hash64::default(),
        };
        receipt.receipt_id = receipt.id_of();
        receipt
    }
}

/// Why a vision-language receipt is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VlmReceiptError {
    Unsealed,
    NotThisClaim(&'static str),
    /// The picture the user holds does not produce the committed pixels under the receipt's record.
    PictureDiffers { index: usize, why: &'static str },
}

/// **Verify a vision-language receipt** against the claim's binding and the pictures the user holds: the seal; the binding's identities;
/// and, per picture, that preprocessing the user's raw picture with the declared fit gives the record the receipt names AND the
/// `input_root` the job commits — so neither the pixels the class saw nor the way they were derived can be swapped afterwards.
pub fn verify_receipt(
    receipt: &VlmReceiptV1,
    job: &PalwFreePromptJobV5,
    binding: &PalwGenStepBindingV1,
    row: &PalwGenClassRecordV1,
    raw_images: &[DecodedImageV1],
    fit: Fit,
) -> Result<(), VlmReceiptError> {
    if receipt.id_of() != receipt.receipt_id {
        return Err(VlmReceiptError::Unsealed);
    }
    if receipt.job_id != fp_job_id_v5(job) || binding.job != *job {
        return Err(VlmReceiptError::NotThisClaim("job"));
    }
    if receipt.class_id != job.v4.class_id || receipt.class_id != row.class_id {
        return Err(VlmReceiptError::NotThisClaim("class"));
    }
    if receipt.execution_root != binding.committed_execution_root || binding.execution_root() != binding.committed_execution_root {
        return Err(VlmReceiptError::NotThisClaim("execution_root"));
    }
    if receipt.generated_ids_hash != ids_digest(&binding.generated) || receipt.generated_tokens as usize != binding.generated.len() {
        return Err(VlmReceiptError::NotThisClaim("generated ids"));
    }
    let slots = &row.class.offers.images;
    if raw_images.len() != slots.len() || receipt.preprocess_digests.len() != slots.len() || receipt.image_input_roots.len() != slots.len() {
        return Err(VlmReceiptError::NotThisClaim("picture count"));
    }
    for (k, (raw, slot)) in raw_images.iter().zip(slots).enumerate() {
        let Ok((canonical, record)) = preprocess(raw, slot.h, slot.w, fit) else {
            return Err(VlmReceiptError::PictureDiffers { index: k, why: "the picture cannot be preprocessed to the slot" });
        };
        if record.digest() != receipt.preprocess_digests[k] {
            return Err(VlmReceiptError::PictureDiffers { index: k, why: "its preprocessing record is not the receipt's" });
        }
        let reference: PalwGenImageInputRefV1 = kaspa_consensus_core::palw_gen_close_v1::palw_gen_image_input_ref_v1(
            &misaka_palw_tir::pipeline::JobImageV1 { h: canonical.h, w: canonical.w, rgb: canonical.rgb },
            slot.tile_len,
        )
        .map_err(|_| VlmReceiptError::PictureDiffers { index: k, why: "the canonical pixels have no input root" })?;
        if reference != job.images[k] || receipt.image_input_roots[k] != reference.input_root {
            return Err(VlmReceiptError::PictureDiffers { index: k, why: "its canonical pixels are not the job's input_root" });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA;
    use kaspa_consensus_core::tx::TransactionId;
    use misaka_palw_base0::gen_worker::{
        GenHeldClassV1, GenSeatJudgmentV1, PalwGenWorkerAnswerV1, gen_seat_judge_v1, gen_toy_vlm_fixture_v1, gen_worker_answer_v1,
    };

    const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;

    fn envelope(nonce: u8) -> JobEnvelope {
        JobEnvelope {
            network_domain: Hash64::from_bytes([1; 64]),
            executor_bond: TransactionOutpoint::new(TransactionId::from_bytes([3; 64]), 0),
            executor_pubkey: vec![4; 32],
            operator_id: Hash64::from_bytes([5; 64]),
            anchor_block: Hash64::from_bytes([6; 64]),
            anchor_daa: 100,
            job_nonce: [nonce; 32],
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        }
    }

    /// A person's picture: 5x13 (wide), not the slot's 2x3 — and wide enough that letterboxing it and stretching it give different pixels.
    fn picture(seed: u8) -> DecodedImageV1 {
        DecodedImageV1 { h: 5, w: 13, rgb: (0..5 * 13 * 3).map(|i| (i as u32 * 29 + u32::from(seed) * 17) as u8).collect() }
    }

    struct World {
        fx: misaka_palw_base0::gen_worker::GenToyVlmFixtureV1,
        held: GenHeldClassV1<misaka_palw_base0::gen_worker::GenToyParamsV1>,
    }

    fn world() -> World {
        let fx = gen_toy_vlm_fixture_v1().expect("the toy vision-language class");
        let held = GenHeldClassV1::hold(fx.row.clone(), fx.params.clone()).expect("the class's weights are its root");
        World { fx, held }
    }

    fn plan_of(w: &World, raw: &[DecodedImageV1], fit: Fit, nonce: u8) -> Result<VlmPlan, String> {
        plan(&VlmRequest {
            envelope: envelope(nonce),
            row: &w.fx.row,
            prompt_ids: &w.fx.prompt,
            form: FORM,
            decode_token_limit: 4,
            max_context_tokens: 64,
            raw_images: raw,
            fit,
        })
    }

    fn run(w: &World, plan: &VlmPlan) -> PalwGenStepBindingV1 {
        let (answer, work) = gen_worker_answer_v1(&w.held, &borsh::to_vec(&plan.request).unwrap(), FORM);
        let PalwGenWorkerAnswerV1::Result { binding } = answer else { panic!("the worker refused: {answer:?}") };
        assert_eq!(binding.execution_root(), binding.committed_execution_root);
        assert!(work.is_some());
        binding
    }

    /// **The path end to end, in process, on the repository's tiny vision-language class**: a picture that is NOT the slot's size is
    /// preprocessed, committed as `input_root`, encoded by the vision stage, read by the text stage, and the claim is judged `Valid` by
    /// a seat replaying from the canonical pixels.
    #[test]
    fn a_picture_of_another_size_is_preprocessed_run_through_both_stages_and_judged_valid() {
        let w = world();
        let slot = &w.fx.row.class.offers.images[0];
        let raw = picture(1);
        assert_ne!((raw.h, raw.w), (slot.h, slot.w), "the person's picture is not the slot's size");
        let p = plan_of(&w, std::slice::from_ref(&raw), Fit::Letterbox { pad: [0, 0, 0] }, 7).unwrap();
        // Preprocessing → the job: the V5 job commits the canonical pixels' input_root at the slot's tile length.
        assert_eq!((p.canonical_images[0].h, p.canonical_images[0].w), (slot.h, slot.w));
        let reference = kaspa_consensus_core::palw_gen_close_v1::palw_gen_image_input_ref_v1(
            &misaka_palw_tir::pipeline::JobImageV1 { h: slot.h, w: slot.w, rgb: p.canonical_images[0].rgb.clone() },
            slot.tile_len,
        )
        .unwrap();
        assert_eq!(p.job.images, vec![reference]);
        assert_eq!(p.records[0].output_digest, crate::preprocess::pixels_digest(slot.h, slot.w, &p.canonical_images[0].rgb));
        // The run: a vision stage and a text stage, each with its own root, and generated ids.
        let binding = run(&w, &p);
        assert_eq!(binding.stage_roots.len(), 2, "the encoder and the decoder are separate stages, each bound");
        assert!(!binding.generated.is_empty(), "the decoder generated from the picture and the prompt");
        // The seat replays from the canonical pixels and agrees.
        let judged = gen_seat_judge_v1(
            &w.held,
            &binding.committed_execution_root,
            &p.job,
            &w.fx.prompt,
            &[misaka_palw_tir::pipeline::JobImageV1 { h: slot.h, w: slot.w, rgb: p.canonical_images[0].rgb.clone() }],
            &[],
            FORM,
        );
        assert_eq!(judged, GenSeatJudgmentV1::Valid);
        // The same request twice (same nonce) is the same claim, bit for bit: preprocessing adds no entropy.
        let again = plan_of(&w, std::slice::from_ref(&raw), Fit::Letterbox { pad: [0, 0, 0] }, 7).unwrap();
        assert_eq!(run(&w, &again).committed_execution_root, binding.committed_execution_root);
    }

    /// Every link of preprocessing → encoder → decoder → output is bound: change the raw picture, the fit, the pad, the prompt, or the
    /// nonce, and the job id, the vision stage's root and the execution root move.
    #[test]
    fn changing_the_picture_the_fit_or_the_prompt_moves_the_job_the_stage_roots_and_the_execution_root() {
        let w = world();
        let raw = picture(1);
        let fit = Fit::Letterbox { pad: [0, 0, 0] };
        let base = plan_of(&w, std::slice::from_ref(&raw), fit, 7).unwrap();
        let base_binding = run(&w, &base);
        // The raw picture is 5x13 letterboxed into 2x3: the content is one row, sampled at source row 2. Flip that row's bytes.
        let mut other_pixel = raw.clone();
        for b in &mut other_pixel.rgb[2 * 13 * 3..3 * 13 * 3] {
            *b ^= 0x40;
        }
        let variants: Vec<(&str, VlmPlan)> = vec![
            ("a changed SAMPLED row of the RAW picture", plan_of(&w, std::slice::from_ref(&other_pixel), fit, 7).unwrap()),
            ("stretch instead of letterbox", plan_of(&w, std::slice::from_ref(&raw), Fit::Stretch, 7).unwrap()),
            ("another pad colour", plan_of(&w, std::slice::from_ref(&raw), Fit::Letterbox { pad: [255, 0, 0] }, 7).unwrap()),
            ("another nonce", plan_of(&w, std::slice::from_ref(&raw), fit, 8).unwrap()),
        ];
        for (what, plan) in &variants {
            assert_ne!(fp_job_id_v5(&plan.job), fp_job_id_v5(&base.job), "{what}: the job id");
            let binding = run(&w, plan);
            assert_ne!(binding.committed_execution_root, base_binding.committed_execution_root, "{what}: the execution root");
            if *what != "another nonce" {
                // The picture reaches the VISION stage's root (the encoder read it), not only the job id.
                assert_ne!(binding.stage_roots[0], base_binding.stage_roots[0], "{what}: the vision stage's root");
            }
        }
        // **A byte the algorithm never samples** (source row 0 here: a point-sampling bilinear is not an area filter) changes NEITHER the
        // canonical pixels NOR the job — which is why the preprocessing record carries the RAW picture's digest: the receipt still moves.
        let mut unsampled = raw.clone();
        unsampled.rgb[3] ^= 0x40;
        let unsampled_plan = plan_of(&w, std::slice::from_ref(&unsampled), fit, 7).unwrap();
        assert_eq!(fp_job_id_v5(&unsampled_plan.job), fp_job_id_v5(&base.job), "an unsampled raw byte is not in the job");
        assert_ne!(unsampled_plan.records[0].source_digest, base.records[0].source_digest, "but it IS in the record");
        assert_ne!(unsampled_plan.records[0].digest(), base.records[0].digest());
        let r_base = VlmReceiptV1::seal(&base, &base_binding, Hash64::default());
        let r_unsampled = VlmReceiptV1::seal(&unsampled_plan, &run(&w, &unsampled_plan), Hash64::default());
        assert_eq!(r_base.execution_root, r_unsampled.execution_root);
        assert_ne!(r_base.receipt_id, r_unsampled.receipt_id, "the receipt tells the two pictures apart even though the claim does not");
        // The prompt (the text stage's input) changes the job and the text stage, not the vision stage.
        let other_prompt = [w.fx.prompt[0], w.fx.prompt[1], w.fx.prompt[2], w.fx.prompt[3] ^ 1];
        let prompt_variant = plan(&VlmRequest {
            envelope: envelope(7),
            row: &w.fx.row,
            prompt_ids: &other_prompt,
            form: FORM,
            decode_token_limit: 4,
            max_context_tokens: 64,
            raw_images: std::slice::from_ref(&raw),
            fit,
        })
        .unwrap();
        assert_ne!(fp_job_id_v5(&prompt_variant.job), fp_job_id_v5(&base.job));
        let b = run(&w, &prompt_variant);
        assert_ne!(b.committed_execution_root, base_binding.committed_execution_root);
        assert_eq!(b.stage_roots[0], base_binding.stage_roots[0], "the vision stage read the same canonical pixels");
    }

    /// A worker or seat handed pixels that are not the job's `input_root` refuses by name: preprocessing cannot be redone differently later.
    #[test]
    fn pixels_that_are_not_the_jobs_input_root_are_refused_by_the_worker_and_the_seat() {
        let w = world();
        let raw = picture(1);
        let p = plan_of(&w, std::slice::from_ref(&raw), Fit::Letterbox { pad: [0, 0, 0] }, 7).unwrap();
        let binding = run(&w, &p);
        // The same raw picture preprocessed another way is other pixels: the worker will not run the job with them...
        let stretched = plan_of(&w, std::slice::from_ref(&raw), Fit::Stretch, 7).unwrap();
        let swapped = crate::tensor::gen_worker_request_v1(p.job.clone(), w.fx.prompt.clone(), &stretched.canonical_images);
        let (answer, _) = gen_worker_answer_v1(&w.held, &borsh::to_vec(&swapped).unwrap(), FORM);
        assert!(matches!(&answer, PalwGenWorkerAnswerV1::Refused { why } if why.contains("image 0")), "{answer:?}");
        // ... and a seat replaying with them cannot judge the claim.
        let slot = &w.fx.row.class.offers.images[0];
        let judged = gen_seat_judge_v1(
            &w.held,
            &binding.committed_execution_root,
            &p.job,
            &w.fx.prompt,
            &[misaka_palw_tir::pipeline::JobImageV1 { h: slot.h, w: slot.w, rgb: stretched.canonical_images[0].rgb.clone() }],
            &[],
            FORM,
        );
        assert!(!matches!(judged, GenSeatJudgmentV1::Valid), "{judged:?}");
    }

    #[test]
    fn the_wrong_number_of_pictures_and_a_text_class_are_refused_by_name() {
        let w = world();
        let raw = picture(1);
        let none = plan_of(&w, &[], Fit::Stretch, 7).err().expect("refused");
        assert!(none.contains("exactly one picture per slot"), "{none}");
        let two = plan_of(&w, &[raw.clone(), raw.clone()], Fit::Stretch, 7).err().expect("refused");
        assert!(two.contains("1 image slot"), "{two}");
        let mut text_class = w.fx.row.clone();
        std::sync::Arc::make_mut(&mut text_class.class).offers.images.clear();
        let err = plan(&VlmRequest {
            envelope: envelope(7),
            row: &text_class,
            prompt_ids: &w.fx.prompt,
            form: FORM,
            decode_token_limit: 4,
            max_context_tokens: 64,
            raw_images: &[raw],
            fit: Fit::Stretch,
        })
        .err()
        .expect("refused");
        assert!(err.contains("no image slot"), "{err}");
    }

    /// The receipt binds the whole chain and refuses a swapped picture, a swapped fit and a forged identity.
    #[test]
    fn the_receipt_binds_the_preprocessing_the_job_and_the_output_and_refuses_each_forgery() {
        let w = world();
        let raw = picture(1);
        let fit = Fit::Letterbox { pad: [0, 0, 0] };
        let p = plan_of(&w, std::slice::from_ref(&raw), fit, 7).unwrap();
        let binding = run(&w, &p);
        let receipt = VlmReceiptV1::seal(&p, &binding, Hash64::from_u64_word(0xAA));
        verify_receipt(&receipt, &p.job, &binding, &w.fx.row, std::slice::from_ref(&raw), fit).expect("the honest receipt verifies");
        // The user's picture is the only thing that makes the record true: another picture, another fit, another pad.
        let mut other = raw.clone();
        other.rgb[0] ^= 1;
        assert!(matches!(verify_receipt(&receipt, &p.job, &binding, &w.fx.row, &[other], fit), Err(VlmReceiptError::PictureDiffers { index: 0, .. })));
        assert!(matches!(verify_receipt(&receipt, &p.job, &binding, &w.fx.row, std::slice::from_ref(&raw), Fit::Stretch), Err(VlmReceiptError::PictureDiffers { .. })));
        assert!(matches!(
            verify_receipt(&receipt, &p.job, &binding, &w.fx.row, std::slice::from_ref(&raw), Fit::Letterbox { pad: [1, 1, 1] }),
            Err(VlmReceiptError::PictureDiffers { .. })
        ));
        // The receipt's own fields (re-sealed or not) against the claim.
        let reseal = |mut r: VlmReceiptV1, edit: &dyn Fn(&mut VlmReceiptV1)| {
            edit(&mut r);
            r.receipt_id = r.id_of();
            r
        };
        let mut hand_edited = receipt.clone();
        hand_edited.execution_root = Hash64::from_u64_word(1);
        assert_eq!(verify_receipt(&hand_edited, &p.job, &binding, &w.fx.row, std::slice::from_ref(&raw), fit), Err(VlmReceiptError::Unsealed));
        let h = Hash64::from_u64_word(1);
        for (field, edit) in [
            ("job", Box::new(move |r: &mut VlmReceiptV1| r.job_id = h) as Box<dyn Fn(&mut VlmReceiptV1)>),
            ("class", Box::new(move |r| r.class_id = h)),
            ("execution_root", Box::new(move |r| r.execution_root = h)),
            ("generated ids", Box::new(move |r| r.generated_ids_hash = h)),
            ("picture count", Box::new(|r| r.preprocess_digests.push(Hash64::default()))),
        ] {
            let forged = reseal(receipt.clone(), &*edit);
            assert_eq!(
                verify_receipt(&forged, &p.job, &binding, &w.fx.row, std::slice::from_ref(&raw), fit),
                Err(VlmReceiptError::NotThisClaim(field)),
                "{field}"
            );
        }
        // Changed generated ids in the binding are not the claim's.
        let mut lying = binding.clone();
        lying.generated[0] ^= 1;
        assert!(verify_receipt(&receipt, &p.job, &lying, &w.fx.row, std::slice::from_ref(&raw), fit).is_err());
    }

    /// The V5 lane's stateless validator, test-armed, admits the commitment this path builds — and refuses it unarmed and with any root forged.
    #[test]
    fn the_commitment_is_admissible_to_the_v5_lane_when_the_fence_is_test_armed_and_dormant_otherwise() {
        use kaspa_consensus_core::palw_fp_job_v5::{PalwFpV5Error, palw_fp_v5_accept_payload_v1, palw_fp_v5_validate_payload_v1};
        let w = world();
        let raw = picture(1);
        let p = plan_of(&w, std::slice::from_ref(&raw), Fit::Letterbox { pad: [0, 0, 0] }, 7).unwrap();
        let binding = run(&w, &p);
        let commitment = commitment_of(&p.job, &binding, 600_000);
        let payload = payload_of(commitment.clone(), &w.fx.prompt, vec![0x5A; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN]);
        let ladder = 1 << 26;
        let domain = p.job.v4.network_domain;
        // Dormant everywhere: `armed = false` is what every preset passes.
        assert_eq!(palw_fp_v5_validate_payload_v1(&payload, domain, false, ladder, None, FORM, false), Err(PalwFpV5Error::NotArmed));
        // Test-armed: the V5 job is recovered from the carried form and the V4 commitment rules pass over its V4 view.
        let admitted = palw_fp_v5_validate_payload_v1(&payload, domain, false, ladder, None, FORM, true).expect("admissible to a test-armed V5 lane");
        assert_eq!(admitted, p.job);
        let (resolved, row) = palw_fp_v5_accept_payload_v1(&payload, Some(&w.fx.row), domain, false, ladder, None, FORM, true).expect("and the class resolves");
        assert_eq!((resolved, row.class_id), (p.job.clone(), w.fx.row.class_id));
        // The claim id covers the pictures: another input_root, another claim.
        let other = plan_of(&w, &[picture(2)], Fit::Letterbox { pad: [0, 0, 0] }, 7).unwrap();
        let other_commitment = commitment_of(&other.job, &run(&w, &other), 600_000);
        assert_ne!(
            kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&commitment),
            kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&other_commitment)
        );
        // A forged execution root is no longer the binding's own (the receipt/seat catch it); a zero price is refused statelessly.
        let mut free = payload.clone();
        free.commitment.work_leaves = 0;
        assert!(palw_fp_v5_validate_payload_v1(&free, domain, false, ladder, None, FORM, true).is_err());
    }
}
