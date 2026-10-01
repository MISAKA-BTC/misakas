//! **RFC-0003 §I.4.5: the tensor claim in the fold** — a version-10 free-prompt commitment (an image, an
//! embedding) becomes a weightless claim of its class, through the real fold on testnet-12's own state with
//! `palw_gen_v1` and `palw_fp_job_v5` armed. Every block is checked by the shared chain harness three ways:
//! the delta re-applies and reverts, and the carriage reloads under its committed root.
//!
//! * **the claim**: no quanta, no pwu, no receipt rights, no escrow; its work is the chain's own count of the
//!   job's step space; it reserves `work_leaves × slash_value_per_pwu` against its bond; its work identity is
//!   the class, the job's tail and the bond;
//! * **refusals by name**: below either fence, a class that is not a tensor class, a job that is not the
//!   class's, a count that is not the chain's, a duplicate inference, the class's cap on claims in flight, the
//!   exposure ceiling, a bond that is not the signer's;
//! * **the lifecycle**: from the claim's creation it is any claim of its class — a panel at the bind, receipts,
//!   a licence that carries its outsider's `Valid`, `Final`, the retirement — and weighs nothing.

#[path = "rcore_common.rs"]
mod rcore;
use rcore::*;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER};
use kaspa_consensus_core::palw_gen_admission_v1::*;
use kaspa_consensus_core::palw_gen_claim_v1::*;
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_job_v1::*;
use kaspa_consensus_core::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_state_v2::{PalwClaimSourceV2, PalwStateV2Error, palw_object_is_gen_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
use kaspa_consensus_core::tx::Transaction;
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::pipeline::{Binding, TirPipelineV1, TokenSource, TripRule};
use misaka_palw_tir::program_v2::TirProgramV2;

const AT: u64 = 1_100;
const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;
/// The executor bonds the tests register (registry numbers beside the genesis bonds').
const EXECUTOR: u64 = 21;
const OTHER_EXECUTOR: u64 = 22;

fn net_domain() -> Hash64 {
    h(0xD0)
}

/// testnet-12 with the IR fence, the generative fence, the decode rules and the lane's job fence armed at [`AT`].
fn armed() -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    p.sync_palw_tir_v1();
    p.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(AT)));
    p.palw_fp_decode_rules = Some(ForkActivation::new(AT));
    p.sync_palw_fp_decode_rules();
    p.palw_fp_job_v5 = Some(ForkActivation::new(AT));
    p.sync_palw_gen_v1();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("the four fences at {AT}: {e}"));
    p
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn vector(name: &str) -> (serde_json::Value, Vec<u8>, Vec<Vec<u8>>, TirPipelineV1, Vec<TirProgramV2>) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines").join(name);
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the golden vector")).expect("json");
    let pipeline = unhex(v["pipeline_borsh_hex"].as_str().expect("pipeline bytes"));
    let programs: Vec<Vec<u8>> =
        v["programs"].as_array().expect("programs").iter().map(|p| unhex(p["program_borsh_hex"].as_str().expect("bytes"))).collect();
    let progs: Vec<TirProgramV2> = programs.iter().map(|b| TirProgramV2::decode_canonical(b).expect("a canonical program")).collect();
    let decoded = TirPipelineV1::decode_canonical(&pipeline, &progs).expect("a canonical pipeline");
    (v, pipeline, programs, decoded, progs)
}

fn layouts(pipeline: &TirPipelineV1, programs: &[TirProgramV2], checkpoint: u32, tile: u32) -> Vec<PalwTirLayoutV1> {
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
                h_tile: 16,
                commit_tiles: vec![tile; commits],
                state_tiles: vec![tile; p.states.len()],
            }
        })
        .collect()
}

/// The golden toy text-to-image pipeline as an Image class: a `2×2` RGB image, two offered step counts, a
/// guidance scalar and a prompt of up to the encoder's tokens, no negative prompt.
fn image_class() -> PalwGenClassV1 {
    let (_, pipeline_bytes, program_bytes, p, progs) = vector("toy-image.json");
    let steps_max = p.stages.iter().filter(|st| matches!(st.trip, TripRule::JobSteps)).map(|st| st.max_trip).min().unwrap();
    let mut scalars = Vec::new();
    for st in &p.stages {
        let ext = progs[st.program as usize].inputs.iter().filter(|d| d.is_external());
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
    let rule = p.stages.iter().find_map(|st| st.tokens.as_ref()).expect("the encoder reads the prompt");
    assert_eq!(rule.source, TokenSource::Prompt);
    let encoder = p.stages.iter().find(|st| st.tokens.is_some()).unwrap();
    PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Image as u8,
        layouts: layouts(&p, &progs, 1, 4),
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
    }
}

/// The golden toy image encoder as an Embedding class of one image: `[1, 4]` lanes of `i32`.
fn vision_class() -> PalwGenClassV1 {
    let (v, pipeline_bytes, program_bytes, p, progs) = vector("toy-vision.json");
    let img = &v["job"]["images"][0];
    let slot = PalwGenImageOfferV1 {
        h: img["h"].as_u64().unwrap() as u32,
        w: img["w"].as_u64().unwrap() as u32,
        tile_len: img["tile_len"].as_u64().unwrap() as u32,
        token_equivalents: 0,
    };
    PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Embedding as u8,
        layouts: layouts(&p, &progs, 1, 4),
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
    }
}

/// The text-to-text toy (a VLM) as a Text class: a tensor claim of it is refused (a text class takes V4/V5).
fn text_class() -> PalwGenClassV1 {
    let (_, pipeline_bytes, program_bytes, p, progs) = vector("toy-vlm.json");
    let max_trip = p.stages[p.output_stage as usize].max_trip;
    PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        layouts: layouts(&p, &progs, 1, 4),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: 8,
            max_negative_tokens: 0,
            images: vec![PalwGenImageOfferV1 { h: 2, w: 3, tile_len: 4, token_equivalents: 1_000_000 }],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
            profile: PalwGenProfileOffersV1::None,
        },
        output: OutputSpecV1::tokens(max_trip),
        pipeline: pipeline_bytes,
        programs: program_bytes,
        tokenizer_id: Hash64::from_bytes([0x72; 64]),
    }
}

/// One network: the chain with its fences armed, three classes registered at [`AT`] (an image class, an
/// embedding class, a text class) by the genesis registrant's bond, and two rich executor bonds.
struct Net {
    chain: Chain,
    registrant: PalwBondKeyV2,
    image: Hash64,
    vision: Hash64,
    text: Hash64,
}

fn registration(chain: &Chain, class: PalwGenClassV1, root: Hash64, registrant: PalwBondKeyV2) -> (PalwConsensusObjectV2, Hash64) {
    let (floor, _, target, slash) = genesis_classes(&chain.p)[0];
    let target = chain.s.class_target(&floor).map(|t| t.target).unwrap_or(target);
    let object = palw_gen_post_genesis_registration_v1(class, root, 0, target, slash, AT, registrant, vec![9; 16])
        .expect("the builder counts the yardstick job");
    let PalwConsensusObjectV2::ClassRegisteredGenV1 { class_id, .. } = &object else { unreachable!() };
    let id = *class_id;
    (object, id)
}

fn executor_bond(n: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: fp_pubkey_of(n),
        operator_pubkey: operator_pubkey_of(n),
        collateral: RICH,
        payout_payload: h(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

fn net() -> Net {
    let mut chain = Chain::new(armed());
    chain.room = true;
    let (registrant, _, _) = floor_producer(&chain.p);
    let (image_object, image) = registration(&chain, image_class(), h(0xA9), registrant);
    let (vision_object, vision) = registration(&chain, vision_class(), h(0xAB), registrant);
    let (text_object, text) = registration(&chain, text_class(), h(0xAA), registrant);
    // The bonds first, below the fences (a bond registers on any ruleset), then the classes at the fence.
    chain.step_at(AT - 1, &[executor_bond(EXECUTOR), executor_bond(OTHER_EXECUTOR)], PalwBlockWorkV3::None, Hash64::default(), 0);
    chain.step_at(AT, &[image_object, vision_object, text_object], PalwBlockWorkV3::None, Hash64::default(), 0);
    Net { chain, registrant, image, vision, text }
}

fn envelope(n: u64, class_id: Hash64, privacy: u8, nonce: u8) -> PalwJobEnvelopeV1 {
    PalwJobEnvelopeV1 {
        network_domain: net_domain(),
        class_id,
        executor_bond: bond_key(n).0,
        executor_pubkey: fp_pubkey_of(n),
        operator_id: h(0x0E),
        anchor_block: h(0xA1),
        anchor_daa: AT,
        job_nonce: [nonce; 32],
        privacy_mode: privacy,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
    }
}

fn prompt() -> Vec<u32> {
    vec![2, 5, 1]
}

/// An image job of the toy class: the prompt, the guidance at the grid's low end, offered step count `steps_position`.
fn image_job(class: &PalwGenClassV1, class_id: Hash64, n: u64, privacy: u8, nonce: u8, seed: u8, steps_position: usize) -> PalwGenJobV1 {
    let PalwGenProfileOffersV1::Image(offers) = &class.offers.profile else { unreachable!() };
    let g = offers.guidance.as_ref().unwrap();
    PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: envelope(n, class_id, privacy, nonce),
        seed: [seed; 32],
        body: PalwGenBodyV1::Image(PalwGenImageBodyV1 {
            prompt_token_ids_hash: prompt_token_ids_commitment_v1(FORM, &prompt()).unwrap(),
            prompt_tokens: prompt().len() as u32,
            negative_token_ids_hash: Hash64::default(),
            negative_tokens: 0,
            guidance_q: g.lo,
            image_index: 0,
            sampler_id: offers.sampler_id,
            steps: class.offers.steps[steps_position] as u16,
            width: 2,
            height: 2,
            output: misaka_palw_gen::OutputKindV1::ImageRgb8.tag(),
        }),
    }
}

/// An embedding job of the vision class: one image at the slot's size (its bytes never ride the chain).
fn vision_job(class: &PalwGenClassV1, class_id: Hash64, n: u64, nonce: u8) -> PalwGenJobV1 {
    let slot = &class.offers.images[0];
    PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: envelope(n, class_id, PALW_FP_PRIVACY_PANEL_DA, nonce),
        seed: [0; 32],
        body: PalwGenBodyV1::Embedding(PalwGenEmbeddingBodyV1 {
            input: PalwGenEmbeddingInputV1::Image(PalwGenImageInputRefV1 { input_root: h(0x1F + nonce as u64), h: slot.h, w: slot.w }),
            pooling: PALW_GEN_POOLING_CLS_V1,
            dims: 4,
            output: misaka_palw_gen::OutputKindV1::EmbeddingI32.tag(),
        }),
    }
}

/// **The leaves the chain counts for a job**, from the registered row.
fn leaves_of(net: &Net, job: &PalwGenJobV1) -> u64 {
    let row = net.chain.s.gen_class_v1(&job.envelope.class_id).expect("a registered generative class");
    let accepted = palw_gen_job_resolve_class_v1(job, row).unwrap_or_else(|e| panic!("the job is the class's: {e}"));
    palw_gen_job_step_leaves_v1(row, &accepted).expect("the chain counts the job")
}

/// The object the acceptance walk turns a tensor claim's payload into — the walk itself, at the network's
/// domain, with the signature verifier answering yes (the signature is the acceptance layer's).
fn claim_object(job: &PalwGenJobV1, ids: Vec<u32>, leaves: u64) -> PalwConsensusObjectV2 {
    let payload = palw_gen_payload_v1(job, leaves, h(0x5701), h(0x0A01), ids, vec![7u8; MLDSA87_SIGNATURE_LEN]);
    let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, borsh::to_vec(&payload).unwrap());
    let out = palw_fp_gen_objects_from_accepted_txs_v1(
        &[tx],
        net_domain(),
        true,
        |_| kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP,
        FORM,
        |_, _, _, _| true,
    );
    let [one] = &out.objects[..] else { panic!("the walk makes one object: {:?}", out.skipped) };
    assert!(palw_object_is_gen_v1(&one.object), "a tensor claim's object is a generative object");
    one.object.clone()
}

fn claim_id_of(object: &PalwConsensusObjectV2) -> Hash64 {
    let PalwConsensusObjectV2::GenTensorCommitted { claim, .. } = object else { panic!("a tensor claim's object") };
    *claim
}

/// A public image claim of bond `n` (a distinct job by `nonce` and `seed`), at the offered step count `steps_position`.
fn image_claim(net: &Net, n: u64, nonce: u8, seed: u8, steps_position: usize) -> (PalwConsensusObjectV2, PalwGenJobV1) {
    let class = image_class();
    let job = image_job(&class, net.image, n, PALW_FP_PRIVACY_PUBLIC_DA, nonce, seed, steps_position);
    let leaves = leaves_of(net, &job);
    (claim_object(&job, prompt(), leaves), job)
}

#[test]
fn a_tensor_claim_folds_as_a_weightless_claim_of_its_class() {
    let mut net = net();
    let (object, job) = image_claim(&net, EXECUTOR, 1, 0x33, 0);
    let id = claim_id_of(&object);
    let leaves = leaves_of(&net, &job);
    let (slash, bond) = (net.chain.s.class(&net.image).expect("the class").slash_value_per_pwu, bond_key(EXECUTOR));
    assert_eq!(net.chain.reserved(&bond), 0);
    net.chain.step(&[object]);
    let claim = net.chain.claim(&id);
    assert!(matches!(&claim.source, PalwClaimSourceV2::FreePrompt { quanta: 0, spent } if spent.is_empty()), "no quanta, no spend");
    assert_eq!((claim.pwu, claim.rights_reserved, claim.escrowed_reward, claim.immature_contribution), (0, 0, 0, 0), "weightless");
    assert_eq!((claim.class_id, claim.bond, claim.work_leaves), (net.image, bond, leaves), "the chain's own count of the job's leaves");
    assert!(matches!(claim.phase, PalwClaimPhaseV2::Provisional));
    assert_eq!((claim.trace_root, claim.output_root), (h(0x5701), h(0x0A01)), "the roots as committed");
    assert_eq!(claim.trace_chunk_count, 1);
    assert!(claim.trace_retention_daa > claim.accepted_daa, "the chain derives the retention");
    assert_eq!(
        claim.work_id,
        Some(palw_gen_work_id_v1(&net.image, &job.tail(), &bond.0)),
        "one inference, one claim per bond: the class, the job's tail and the bond"
    );
    assert_eq!(claim.reserved, (leaves as u128) * (slash as u128), "work_leaves × the class's registered slash value");
    assert_eq!(net.chain.reserved(&bond), claim.reserved, "held against the executor's bond");
    // The first claim of a class is the lane's: it holds no weight, and moves none.
    assert_eq!(net.chain.s.safe_weight(), 0);
}

#[test]
fn an_embedding_claim_of_a_private_job_folds_with_no_ids() {
    let mut net = net();
    let class = vision_class();
    let job = vision_job(&class, net.vision, EXECUTOR, 4);
    let leaves = leaves_of(&net, &job);
    let object = claim_object(&job, vec![], leaves);
    let PalwConsensusObjectV2::GenTensorCommitted { prompt_token_ids, .. } = &object else { unreachable!() };
    assert!(prompt_token_ids.is_empty());
    let id = claim_id_of(&object);
    net.chain.step(&[object]);
    let claim = net.chain.claim(&id);
    assert_eq!((claim.class_id, claim.work_leaves, claim.pwu), (net.vision, leaves, 0));
}

#[test]
fn below_either_fence_a_tensor_claim_is_refused_by_name() {
    let net = net();
    let (object, _) = image_claim(&net, EXECUTOR, 1, 0x33, 0);
    // Below the fence: the second lock behind the walk's drop (a block at AT - 1 on the pre-registration state).
    let fresh = Chain::new(armed());
    let below = fresh.try_fold(&fresh.s, &ctx(0xCA_0000 + AT - 1, AT - 1, AT - 1, 0), &[object.clone()], PalwBlockWorkV3::None, Hash64::default());
    assert!(matches!(&below, Err(PalwStateV2Error::GenObjectRefused(_))), "the generative fence's own lock fires first: {below:?}");
    // The generative fence alone (palw_fp_job_v5 pushed out): refused too — the lane's fence is the claim's.
    let mut p = armed();
    p.palw_fp_job_v5 = Some(ForkActivation::new(AT + 500));
    p.sync_palw_gen_v1();
    p.validate_palw_v2().expect("the lane's fence may sit above the registry's");
    let mut late = Chain::new(p);
    late.room = true;
    late.s = net.chain.s.clone();
    late.sp = bundle(&late.p).state.clone();
    let refused = late.try_fold(&late.s, &ctx(0xCA_0000 + AT + 1, AT + 1, AT + 1, 0), &[object], PalwBlockWorkV3::None, Hash64::default());
    assert!(matches!(&refused, Err(PalwStateV2Error::GenClaimRefused(_))), "{refused:?}");
}

// ---------------------------------------------------------------------------------------------
// Refusals by name
// ---------------------------------------------------------------------------------------------

/// The fold of `object` on the network's state, at the block after the last.
fn fold_one(net: &Net, object: PalwConsensusObjectV2) -> Result<(), PalwStateV2Error> {
    let at = net.chain.daa + 1;
    net.chain.try_fold(&net.chain.s, &ctx(0xCA_0000 + at, at, at, 0), &[object], PalwBlockWorkV3::None, Hash64::default()).map(|_| ())
}

fn refused_with(result: Result<(), PalwStateV2Error>, what: &str) {
    match result {
        Err(PalwStateV2Error::GenClaimRefused(why)) => assert!(why.contains(what), "refused, but not for {what:?}: {why}"),
        other => panic!("expected a refusal naming {what:?}, got {other:?}"),
    }
}

#[test]
fn every_way_a_tensor_claim_is_not_its_classs_is_refused_by_name() {
    let net = net();
    let class = image_class();
    let good = image_job(&class, net.image, EXECUTOR, PALW_FP_PRIVACY_PUBLIC_DA, 1, 0x33, 0);
    let leaves = leaves_of(&net, &good);
    assert!(fold_one(&net, claim_object(&good, prompt(), leaves)).is_ok(), "the control folds");

    // The class does not exist: the registry's own refusal, by name.
    let mut stranger = good.clone();
    stranger.envelope.class_id = h(0xEE);
    assert!(matches!(fold_one(&net, claim_object(&stranger, prompt(), leaves)), Err(PalwStateV2Error::MissingClass(id)) if id == h(0xEE)));

    // A text class takes V4/V5 jobs; a tensor job of it is refused.
    let mut text = good.clone();
    text.envelope.class_id = net.text;
    refused_with(fold_one(&net, claim_object(&text, prompt(), leaves)), "not the class's");

    // An image job for an embedding class, and an embedding job for an image class: the body is the profile.
    let mut wrong_body = good.clone();
    wrong_body.envelope.class_id = net.vision;
    refused_with(fold_one(&net, claim_object(&wrong_body, prompt(), leaves)), "not the class's");

    // A parameter the class does not offer: a step count outside its offers.
    let mut steps = good.clone();
    let PalwGenBodyV1::Image(body) = &mut steps.body else { unreachable!() };
    body.steps = 99;
    refused_with(fold_one(&net, claim_object(&steps, prompt(), leaves)), "not the class's");

    // The ids' bound: a prompt of more ids than the class's encoder reads.
    let long: Vec<u32> = (0..class.offers.max_prompt_tokens + 1).collect();
    let mut over = good.clone();
    let PalwGenBodyV1::Image(body) = &mut over.body else { unreachable!() };
    body.prompt_token_ids_hash = prompt_token_ids_commitment_v1(FORM, &long).unwrap();
    body.prompt_tokens = long.len() as u32;
    refused_with(fold_one(&net, claim_object(&over, long, leaves)), "not the class's");

    // The work is the chain's count, not the executor's word: one leaf more or fewer is refused, not corrected.
    for wrong in [leaves + 1, leaves - 1] {
        refused_with(fold_one(&net, claim_object(&good, prompt(), wrong)), "chain's count");
    }
}

#[test]
fn a_bond_that_is_not_the_signers_or_cannot_carry_the_claim_is_refused() {
    let net = net();
    let class = image_class();
    // The key the job names is not the bond's.
    let mut wrong_key = image_job(&class, net.image, EXECUTOR, PALW_FP_PRIVACY_PUBLIC_DA, 1, 0x33, 0);
    wrong_key.envelope.executor_pubkey = fp_pubkey_of(OTHER_EXECUTOR);
    let leaves = leaves_of(&net, &wrong_key);
    assert!(matches!(fold_one(&net, claim_object(&wrong_key, prompt(), leaves)), Err(PalwStateV2Error::BondKeyMismatch(_))));
    // A bond the registry does not hold.
    let mut missing = image_job(&class, net.image, 77, PALW_FP_PRIVACY_PUBLIC_DA, 1, 0x33, 0);
    missing.envelope.executor_pubkey = fp_pubkey_of(77);
    assert!(matches!(fold_one(&net, claim_object(&missing, prompt(), leaves)), Err(PalwStateV2Error::MissingBond(_))));
}

#[test]
fn one_inference_is_one_claim_per_bond_and_another_bonds_run_is_another() {
    let mut net = net();
    let (first, job) = image_claim(&net, EXECUTOR, 1, 0x33, 0);
    net.chain.step(&[first]);
    // The same inference under another nonce is the same work (another claim id, the same work identity).
    let class = image_class();
    let mut again = job.clone();
    again.envelope.job_nonce = [2; 32];
    let leaves = leaves_of(&net, &again);
    assert!(matches!(fold_one(&net, claim_object(&again, prompt(), leaves)), Err(PalwStateV2Error::DuplicateWork { .. })));
    // The same job's bytes again are the same claim.
    let (same, _) = image_claim(&net, EXECUTOR, 1, 0x33, 0);
    assert!(matches!(fold_one(&net, same), Err(PalwStateV2Error::DuplicateClaim(_))));
    // Another seed is another image, and another bond's run of the same job is another claim.
    let other_seed = image_job(&class, net.image, EXECUTOR, PALW_FP_PRIVACY_PUBLIC_DA, 3, 0x44, 0);
    assert!(fold_one(&net, claim_object(&other_seed, prompt(), leaves)).is_ok(), "another seed");
    let mut other_bond = job.clone();
    other_bond.envelope.executor_bond = bond_key(OTHER_EXECUTOR).0;
    other_bond.envelope.executor_pubkey = fp_pubkey_of(OTHER_EXECUTOR);
    assert!(fold_one(&net, claim_object(&other_bond, prompt(), leaves)).is_ok(), "another bond");
}

#[test]
fn a_class_takes_no_more_claims_in_flight_than_the_fences_cap() {
    let mut net = net();
    let cap = net.chain.sp.gen_max_inflight_claims(PalwGenProfileV1::Image);
    assert!(cap > 0, "the drill's fence carries a cap");
    // Claims of distinct seeds, spread over the executors so no single bond's share of a class's unlicensed
    // claims is what stops the run first.
    let class = image_class();
    let mut taken = 0u32;
    for i in 0..cap {
        let n = if i % 2 == 0 { EXECUTOR } else { OTHER_EXECUTOR };
        let job = image_job(&class, net.image, n, PALW_FP_PRIVACY_PUBLIC_DA, i as u8, i as u8 + 1, 0);
        let leaves = leaves_of(&net, &job);
        match fold_one(&net, claim_object(&job, prompt(), leaves)) {
            Ok(()) => {
                net.chain.step(&[claim_object(&job, prompt(), leaves)]);
                taken += 1;
            }
            Err(e) => panic!("claim {i} of {cap} is refused early: {e}"),
        }
    }
    assert_eq!(taken, cap);
    let job = image_job(&class, net.image, EXECUTOR, PALW_FP_PRIVACY_PUBLIC_DA, 0xEE, 0xEE, 0);
    let leaves = leaves_of(&net, &job);
    assert!(
        matches!(fold_one(&net, claim_object(&job, prompt(), leaves)), Err(PalwStateV2Error::GenClassInflightCapped { class, inflight, cap: c }) if class == net.image && inflight == cap as u64 && c == cap),
        "the cap + 1th claim of the class is refused by name"
    );
    // The cap is per class: the embedding class is untouched by the image class's.
    let vision = vision_job(&vision_class(), net.vision, EXECUTOR, 1);
    let leaves = leaves_of(&net, &vision);
    assert!(fold_one(&net, claim_object(&vision, vec![], leaves)).is_ok());
}
