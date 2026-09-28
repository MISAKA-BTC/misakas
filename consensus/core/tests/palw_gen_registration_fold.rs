//! **RFC-0003: the pipeline admission and the generative class registry.** A generative class
//! registration passes the pipeline admission (`verify_gen_class_admission_v1`: the preflight, the
//! step tree counted exactly, every cone against the court, the id, no weight) and folds — through
//! the legacy registration's own body, plus the `gen_classes` row — on testnet-12's fold with
//! `palw_gen_v1` armed; below the fence it is refused by name. A V5 claim's class resolves against
//! that registry (`PalwChainStateV2::fp_v5_class_v1`).
//!
//! Every block is checked by the shared chain harness three ways: the delta re-applies and reverts,
//! and the child's carriage (with the new `0xC2` tail) reloads under its committed root.

#[path = "rcore_common.rs"]
mod rcore;
use rcore::*;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
use kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError;
use kaspa_consensus_core::palw_fp_job_v5::{PalwFpV5Error, PalwFreePromptJobV5, palw_fp_v5_resolve_class_v1};
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_V4_VERSION, PalwFreePromptJobV3};
use kaspa_consensus_core::palw_gen_admission_v1::*;
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_step_v1::PalwGenStepSpaceV1;
use kaspa_consensus_core::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use kaspa_consensus_core::palw_state_v2::{PalwPwuRuleV2, PalwStateV2Error, palw_class_registration_buyer_v1, palw_object_is_gen_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::pipeline::{Binding, TirPipelineV1, TokenSource, TripRule};
use misaka_palw_tir::program_v2::TirProgramV2;

const AT: u64 = 1_100;

/// testnet-12 with the IR fence and the generative fence armed at [`AT`], both mirrored.
fn armed() -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    p.sync_palw_tir_v1();
    p.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(AT)));
    p.sync_palw_gen_v1();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("both fences at {AT}: {e}"));
    p
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

/// A golden pipeline vector's canonical bytes: `(pipeline, programs)`.
fn vector_bytes(name: &str) -> (Vec<u8>, Vec<Vec<u8>>) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines").join(name);
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the golden vector")).expect("json");
    let pipeline = unhex(v["pipeline_borsh_hex"].as_str().expect("pipeline bytes"));
    let programs =
        v["programs"].as_array().expect("programs").iter().map(|p| unhex(p["program_borsh_hex"].as_str().expect("bytes"))).collect();
    (pipeline, programs)
}

fn decoded(pipeline: &[u8], programs: &[Vec<u8>]) -> (TirPipelineV1, Vec<TirProgramV2>) {
    let progs: Vec<TirProgramV2> = programs.iter().map(|b| TirProgramV2::decode_canonical(b).expect("a canonical program")).collect();
    (TirPipelineV1::decode_canonical(pipeline, &progs).expect("a canonical pipeline"), progs)
}

/// A layout per stage: its context, `checkpoint` positions a checkpoint, one `tile`-lane tile per
/// commit point and state.
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

/// The golden toy image pipeline as an Image class, its widest offers.
fn image_class() -> PalwGenClassV1 {
    let (pipeline, programs) = vector_bytes("toy-image.json");
    let (p, progs) = decoded(&pipeline, &programs);
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
            steps: (1..=steps_max).collect(),
            scalars,
            max_prompt_tokens: encoder.max_trip - (rule.prefix.len() + rule.suffix.len()) as u32,
            max_negative_tokens: 0,
            images: vec![],
        },
        output: OutputSpecV1::image_rgb8(2, 2),
        pipeline,
        programs,
        tokenizer_id: Hash64::from_bytes([0x71; 64]),
    }
}

/// The golden toy VLM pipeline as a Text class with one 2×3 image slot, priced above its floor.
fn vlm_class() -> PalwGenClassV1 {
    let (pipeline, programs) = vector_bytes("toy-vlm.json");
    let (p, progs) = decoded(&pipeline, &programs);
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
        },
        output: OutputSpecV1::tokens(max_trip),
        pipeline,
        programs,
        tokenizer_id: Hash64::from_bytes([0x72; 64]),
    }
}

fn root_of(seed: u8) -> Hash64 {
    Hash64::from_bytes([seed; 64])
}

/// The registration a registrant makes: the chain's target and slash value, weightless, active at `daa`.
fn registration(chain: &Chain, class: PalwGenClassV1, root: Hash64, registrant: PalwBondKeyV2, daa: u64) -> PalwConsensusObjectV2 {
    let (floor, _, target, slash) = genesis_classes(&chain.p)[0];
    let target = chain.s.class_target(&floor).map(|t| t.target).unwrap_or(target);
    palw_gen_post_genesis_registration_v1(class, root, 0, target, slash, daa, registrant, vec![9; 16])
        .expect("the builder counts the yardstick job")
}

/// A registrant for the gate alone (the gate reads no bond; the processor and the fold do).
fn someone() -> PalwBondKeyV2 {
    PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(3))
}

fn rules(p: &Params) -> PalwGenAdmissionRulesV1 {
    PalwGenAdmissionRulesV1::at(p, AT).expect("the fence is in force at AT")
}

#[test]
fn the_closed_form_count_is_the_enumeration() {
    for (name, class) in [("toy-image", image_class()), ("toy-vlm", vlm_class())] {
        let (programs, pipeline) = class.decode().unwrap();
        for (checkpoint, tile) in [(1, 4), (2, 3), (3, 5), (5, 1)] {
            let layouts = layouts(&pipeline, &programs, checkpoint, tile);
            let maxes: Vec<u32> = pipeline.stages.iter().map(|st| st.max_trip).collect();
            for scale in [1u32, 2, 3] {
                let trips: Vec<u32> = maxes.iter().map(|m| (m * scale).div_ceil(3).max(1)).collect();
                for prompt_len in [1u32, 2, 5] {
                    let space = PalwGenStepSpaceV1::new(&pipeline, &programs, &layouts, &trips, prompt_len).unwrap();
                    let enumerated: usize = space.stages.iter().map(|s| s.leaves().len()).sum();
                    let closed = PalwGenStepSpaceV1::leaf_count_v1(&pipeline, &programs, &layouts, &trips, None, prompt_len).unwrap();
                    assert_eq!(closed, enumerated as u128, "{name} C={checkpoint} tile={tile} trips={trips:?} prompt={prompt_len}");
                }
            }
        }
    }
}

#[test]
fn the_pipeline_admission_admits_the_toy_classes_and_counts_their_yardstick() {
    let p = armed();
    let bundle = &bundle(&p);
    for (name, class, root) in [("toy-image", image_class(), root_of(0xA9)), ("toy-vlm", vlm_class(), root_of(0xAA))] {
        let object = palw_gen_post_genesis_registration_v1(class.clone(), root, 0, 1 << 100, 1, AT, someone(), vec![]).unwrap();
        let admitted = verify_gen_class_admission_v1(bundle, &rules(&p), &object).unwrap_or_else(|e| panic!("{name}: {e}"));
        let PalwConsensusObjectV2::ClassRegisteredGenV1 { class_id, pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference }, .. } =
            &object
        else {
            unreachable!()
        };
        assert_eq!(admitted.entry.class_id, *class_id);
        assert_eq!(admitted.entry.canonical_step_leaf_count, *pwu_per_inference, "{name}: pwu is the yardstick's count");
        assert!(admitted.entry.max_step_leaf_count >= *pwu_per_inference, "{name}: the widest job bounds the yardstick");
        assert!(!admitted.entry.reachable_kernels.is_empty());
        assert_eq!(admitted.record, palw_gen_class_record_v1(&class, &root).unwrap(), "{name}: the one derivation");
        assert_eq!(admitted.record.check_class_v1(), Ok(()));
        // The yardstick is the most expensive offered job: exactly its step tree's leaves.
        let (programs, pipeline) = class.decode().unwrap();
        let (trips, prompt) = palw_gen_yardstick_v1(&class, &pipeline, &programs).unwrap();
        let space = PalwGenStepSpaceV1::new(&pipeline, &programs, &class.layouts, &trips, prompt).unwrap();
        assert_eq!(space.stages.iter().map(|s| s.leaves().len() as u64).sum::<u64>(), *pwu_per_inference, "{name}");
        // The node's preflight is the gate at a height.
        assert!(palw_gen_registration_preflight_at_v1(&p, bundle, &object, AT).is_ok());
        assert!(matches!(
            palw_gen_registration_preflight_at_v1(&p, bundle, &object, AT - 1),
            Err(PalwClassAdmissionError::GenNeedsItsFence)
        ));
    }
    // The VLM class's record: a text class, its slot and its stream.
    let record = palw_gen_class_record_v1(&vlm_class(), &root_of(0xAA)).unwrap();
    assert_eq!((record.profile, record.text_max_trip, record.images.len()), (PalwGenProfileV1::Text as u8, Some(12), 1));
    let record = palw_gen_class_record_v1(&image_class(), &root_of(0xA9)).unwrap();
    assert_eq!((record.text_max_trip, record.images.len()), (None, 0), "an image class has no text stage");
}

#[test]
fn the_pipeline_admission_refuses_by_name() {
    let p = armed();
    let bundle = &bundle(&p);
    let good = palw_gen_post_genesis_registration_v1(vlm_class(), root_of(0xAA), 0, 1 << 100, 1, AT, someone(), vec![]).unwrap();
    let edit = |f: &dyn Fn(&mut PalwConsensusObjectV2)| {
        let mut o = good.clone();
        f(&mut o);
        verify_gen_class_admission_v1(bundle, &rules(&p), &o).expect_err("refused")
    };
    let e = edit(&|o| {
        if let PalwConsensusObjectV2::ClassRegisteredGenV1 { pwu_rule, .. } = o {
            *pwu_rule = PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 3 };
        }
    });
    assert!(matches!(e, PalwClassAdmissionError::PwuPerInferenceMismatch { declared: 3, .. }), "{e}");
    let e = edit(&|o| {
        if let PalwConsensusObjectV2::ClassRegisteredGenV1 { pwu_rule, .. } = o {
            *pwu_rule = PalwPwuRuleV2::MaxPerAttempt(1_000);
        }
    });
    assert!(matches!(e, PalwClassAdmissionError::ClassIsNotDerived), "{e}");
    let e = edit(&|o| {
        if let PalwConsensusObjectV2::ClassRegisteredGenV1 { share_permille, .. } = o {
            *share_permille = 5;
        }
    });
    assert!(matches!(e, PalwClassAdmissionError::GenEarnsNoWeight { share: 5 }), "{e}");
    let e = edit(&|o| {
        if let PalwConsensusObjectV2::ClassRegisteredGenV1 { class_id, .. } = o {
            *class_id = root_of(0x01);
        }
    });
    assert!(matches!(e, PalwClassAdmissionError::GenClassIdIsNotDerived { .. }), "{e}");
    let e = edit(&|o| {
        if let PalwConsensusObjectV2::ClassRegisteredGenV1 { admission, .. } = o {
            admission.class.offers.images[0].token_equivalents = 0;
        }
    });
    assert!(matches!(e, PalwClassAdmissionError::GenClass(_)), "the preflight's refusal, by name: {e}");
    // Nine distinct tile lengths in one stage: more IR admissions than the gate runs (the stage with
    // the most commit points; the toy image denoiser's).
    let class = image_class();
    let (stage, commits) = class.layouts.iter().enumerate().map(|(s, l)| (s, l.commit_tiles.len())).max_by_key(|x| x.1).unwrap();
    if commits >= 9 {
        let mut o = palw_gen_post_genesis_registration_v1(class, root_of(0xA9), 0, 1 << 100, 1, AT, someone(), vec![]).unwrap();
        if let PalwConsensusObjectV2::ClassRegisteredGenV1 { admission, .. } = &mut o {
            for (i, t) in admission.class.layouts[stage].commit_tiles.iter_mut().enumerate() {
                *t = 4 + (i % 9) as u32;
            }
        }
        let e = verify_gen_class_admission_v1(bundle, &rules(&p), &o).expect_err("refused");
        assert!(matches!(e, PalwClassAdmissionError::GenClass(ref why) if why.contains("distinct")), "{e}");
    }
    assert!(matches!(
        verify_gen_class_admission_v1(bundle, &rules(&p), &genesis_classes_first_object(&p)),
        Err(PalwClassAdmissionError::NotARegistration)
    ));
}

/// An object that is not a generative registration.
fn genesis_classes_first_object(p: &Params) -> PalwConsensusObjectV2 {
    bundle(p).genesis_objects[0].clone()
}

#[test]
fn a_generative_registration_folds_past_the_fence_and_is_refused_below_it() {
    let mut chain = Chain::new(armed());
    chain.room = true;
    let (registrant, _, _) = floor_producer(&chain.p);
    let object = registration(&chain, vlm_class(), root_of(0xAA), registrant, AT);
    let PalwConsensusObjectV2::ClassRegisteredGenV1 { class_id, artifact_root, admission, .. } = &object else { unreachable!() };
    let class_id = *class_id;
    assert!(palw_object_is_gen_v1(&object));
    assert_eq!(palw_class_registration_buyer_v1(&object), Some(registrant), "a bought registration: a slot and the burn");
    assert!(verify_gen_class_admission_v1(&bundle(&chain.p), &rules(&chain.p), &object).is_ok(), "the gate admits it");

    // Below the fence: refused by name, the second lock behind the acceptance walk's drop.
    let below = chain.try_fold(
        &chain.s,
        &ctx(0xCA_0000 + AT - 1, AT - 1, AT - 1, 0),
        &[object.clone()],
        PalwBlockWorkV3::None,
        Hash64::default(),
    );
    assert!(matches!(below, Err(PalwStateV2Error::GenRegistrationRefused(_))), "{below:?}");

    // At the fence: it folds (the harness re-applies, reverts and reloads the block).
    let collateral = chain.s.bond(&registrant).expect("the registrant").collateral;
    let root_before = chain.s.state_root();
    chain.step_at(AT, &[object.clone()], PalwBlockWorkV3::None, Hash64::default(), 0);
    let record = palw_gen_class_record_v1(&admission.class, artifact_root).expect("decodes");
    assert_eq!(chain.s.gen_class_v1(&class_id), Some(&record), "the record, derived as admission derives it");
    let class_state = chain.s.class(&class_id).expect("the class");
    assert_eq!(class_state.registrant_bond, Some(registrant));
    assert!(!class_state.fused_attention, "the generative court dissects no history in v1");
    assert!(chain.s.bond(&registrant).expect("the registrant").collateral < collateral, "the registration burn was taken");
    assert_ne!(chain.s.state_root(), root_before);
    assert!(chain.s.tir_class_v1(&class_id).is_none(), "a pipeline is not an IR class");

    // Rooted without the class's bytes; a carriage whose class does not hash to its row is refused.
    assert_eq!(record.check_class_v1(), Ok(()));
    let carriage = PalwStateCarriageV2::from_state(&chain.s);
    let mut bent = carriage.clone();
    let row = bent.gen_classes.get_mut(&class_id).expect("the row rides the carriage");
    let mut other = row.class.as_ref().clone();
    other.offers.max_prompt_tokens -= 1;
    row.class = std::sync::Arc::new(other);
    assert!(bent.into_state(&chain.sp, None).is_err(), "a class that is not its row's hashes is refused at load");
    let bytes = borsh::to_vec(&carriage).expect("encodes");
    let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("decodes");
    assert_eq!(back, carriage, "the 0xC2 tail round-trips");
    assert!(carriage.into_state(&chain.sp, Some(chain.s.state_root())).is_ok());

    // A second registration of a live class is a duplicate, as for any class.
    let again =
        chain.try_fold(&chain.s, &ctx(0xCA_0000 + AT + 1, AT + 1, AT + 1, 0), &[object], PalwBlockWorkV3::None, Hash64::default());
    assert!(matches!(again, Err(PalwStateV2Error::DuplicateClass(id)) if id == class_id), "{again:?}");
}

/// A V4 job of the golden vectors, retargeted at `class`: its tokenizer, a four-id prompt and a
/// four-id budget (the toy LM's stream holds twelve).
fn v5_job(class: &PalwGenClassV1, root: &Hash64) -> PalwFreePromptJobV5 {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/fp-v4/job_v4_encoding.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the vector")).expect("json");
    let mut v4: PalwFreePromptJobV3 = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| borsh::from_slice::<PalwFreePromptJobV3>(&unhex(c["borsh_hex"].as_str().unwrap())).unwrap())
        .find(|j| j.version == PALW_FP_V4_VERSION)
        .expect("a V4 job");
    v4.class_id = class.class_id(root);
    v4.tokenizer_id = class.tokenizer_id;
    v4.prompt_tokens = 4;
    v4.decode_token_limit = 4;
    PalwFreePromptJobV5 { v4, images: vec![PalwGenImageInputRefV1 { input_root: root_of(0x3b), h: 2, w: 3 }] }
}

#[test]
fn a_v5_claims_class_resolves_against_the_registry() {
    let mut chain = Chain::new(armed());
    chain.room = true;
    let (registrant, _, _) = floor_producer(&chain.p);
    let (vlm, image) = (vlm_class(), image_class());
    let (vlm_root, image_root) = (root_of(0xAA), root_of(0xA9));
    let objects =
        [registration(&chain, vlm.clone(), vlm_root, registrant, AT), registration(&chain, image.clone(), image_root, registrant, AT)];
    let job = v5_job(&vlm, &vlm_root);
    assert!(matches!(chain.s.fp_v5_class_v1(&job, true), Err(PalwFpV5Error::UnknownClass(_))), "nothing registered yet");
    chain.step_at(AT, &objects, PalwBlockWorkV3::None, Hash64::default(), 0);

    let row = chain.s.fp_v5_class_v1(&job, true).unwrap_or_else(|e| panic!("the VLM class resolves: {e}"));
    assert_eq!(row.class_id, vlm.class_id(&vlm_root));
    assert_eq!(row, chain.s.gen_class_v1(&row.class_id).unwrap());
    assert!(matches!(chain.s.fp_v5_class_v1(&job, false), Err(PalwFpV5Error::NotArmed)), "behind palw_fp_job_v5");

    let refused = |edit: &dyn Fn(&mut PalwFreePromptJobV5)| {
        let mut j = job.clone();
        edit(&mut j);
        chain.s.fp_v5_class_v1(&j, true).expect_err("refused")
    };
    assert!(matches!(refused(&|j| j.v4.class_id = root_of(0x07)), PalwFpV5Error::UnknownClass(_)));
    assert!(matches!(refused(&|j| j.v4.class_id = image.class_id(&image_root)), PalwFpV5Error::NotATextClass { .. }));
    assert!(matches!(refused(&|j| j.v4.tokenizer_id = root_of(0x08)), PalwFpV5Error::TokenizerNotTheClasss { .. }));
    assert!(matches!(refused(&|j| j.images[0].w = 2), PalwFpV5Error::Images(_)), "one image per slot, at its size");
    assert!(matches!(refused(&|j| j.v4.decode_token_limit = 10), PalwFpV5Error::ContextExceeded { .. }), "4 + 10 − 1 > 12");
    // The pure resolver is the accessor's body.
    assert_eq!(palw_fp_v5_resolve_class_v1(&job, chain.s.gen_class_v1(&row.class_id), true), Ok(row));
}

#[test]
fn a_chain_with_no_generative_class_roots_and_carries_as_before() {
    // The table is empty below the fence (and on every shipped network): its root block and carriage
    // tail are absent, so the state and its carriage are the ones a build without them produces.
    let chain = Chain::new(armed());
    assert!(chain.s.gen_class_v1(&Hash64::from_bytes([1; 64])).is_none());
    let carriage = PalwStateCarriageV2::from_state(&chain.s);
    assert!(carriage.gen_classes.is_empty());
    let bytes = borsh::to_vec(&carriage).expect("encodes");
    let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("decodes");
    assert_eq!(back, carriage);
    let shipped = Chain::new(palw_t12_shipped_params());
    assert_eq!(shipped.s.state_root(), chain.s.state_root(), "arming the fences moves no state root");
}
