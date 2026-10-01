//! **RFC-0003: the generative class, its identity, its preflight, and a registration object that
//! changes nothing below `palw_gen_v1`.**
//!
//! `ClassRegisteredGenV1` is APPENDED (tag 68, after the second IR fence's 67) so no earlier discriminant
//! moves; it rides the stateless gate at every height (a block carrying it must be valid on this build
//! and on an older one that skips it undecoded), rents nothing, is charged a registration slot like
//! any bought registration (past the fence only: the walk drops it by name first below), is not a
//! carrier a halt must let through, and below the fence the fold refuses it by name — so a block
//! carrying one folds exactly as the same block without it. Past the fence the pipeline admission and
//! the registry (`palw_gen_registration_fold.rs`).
//!
//! The class is the golden toy image pipeline (`consensus-vectors/tir-v2/pipelines/toy-image.json`:
//! a causal text encoder, a denoiser over the job's steps, a decoder to `ImageRgb8 [2, 2, 3]`).

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use kaspa_consensus_core::palw_heartbeat_carriers_v1::palw_h1_carrier_object_v1;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{
    PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2, palw_lifecycle_object_may_ride_v2, validate_palw_lifecycle_tx,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateV2Error,
    apply_palw_transition_v2, palw_class_registration_buyer_v1, palw_object_is_gen_v1, palw_object_is_tir_v1,
    palw_object_rent_ceiling_v1,
};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::pipeline::{Binding, TirPipelineV1, TokenSource, TripRule};
use misaka_palw_tir::program_v2::TirProgramV2;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

/// The golden toy image pipeline's canonical bytes: `(pipeline, programs)`.
fn toy_bytes() -> (Vec<u8>, Vec<Vec<u8>>) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines/toy-image.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the golden vector")).expect("json");
    let pipeline = unhex(v["pipeline_borsh_hex"].as_str().expect("pipeline bytes"));
    let programs = v["programs"]
        .as_array()
        .expect("programs")
        .iter()
        .map(|p| unhex(p["program_borsh_hex"].as_str().expect("program bytes")))
        .collect();
    (pipeline, programs)
}

fn decoded(pipeline: &[u8], programs: &[Vec<u8>]) -> (TirPipelineV1, Vec<TirProgramV2>) {
    let progs: Vec<TirProgramV2> = programs.iter().map(|b| TirProgramV2::decode_canonical(b).expect("a canonical program")).collect();
    (TirPipelineV1::decode_canonical(pipeline, &progs).expect("a canonical pipeline"), progs)
}

/// A layout per stage: its context, one 4-lane tile per commit point and state.
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

/// The widest offers the toy pipeline admits: every step count to the denoiser's `max_trip`, each
/// scalar's bound interval, and the longest prompt the encoder's run holds.
fn offers(pipeline: &TirPipelineV1, programs: &[TirProgramV2]) -> PalwGenOffersV1 {
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
    let max_prompt = encoder.max_trip - (rule.prefix.len() + rule.suffix.len()) as u32;
    PalwGenOffersV1 {
        steps: (1..=steps_max).collect(),
        scalars,
        max_prompt_tokens: max_prompt,
        max_negative_tokens: 0,
        images: vec![],
        max_source_tokens: 0,
        forced_prompt_prefix: vec![],
        source_token_floor: 0,
    }
}

fn class() -> PalwGenClassV1 {
    let (pipeline, programs) = toy_bytes();
    let (p, progs) = decoded(&pipeline, &programs);
    PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Image as u8,
        layouts: layouts(&p, &progs),
        offers: offers(&p, &progs),
        output: OutputSpecV1::image_rgb8(2, 2),
        pipeline,
        programs,
        tokenizer_id: Hash64::from_bytes([0x71; 64]),
    }
}

fn fence() -> PalwGenFenceV1 {
    PalwGenFenceV1::drill_v1(ForkActivation::new(5_000))
}

fn registration(class: PalwGenClassV1) -> PalwConsensusObjectV2 {
    let artifact_root = Hash64::from_bytes([0xA9; 64]);
    PalwConsensusObjectV2::ClassRegisteredGenV1 {
        class_id: class.class_id(&artifact_root),
        artifact_root,
        slash_value_per_pwu: 1,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 7 },
        initial_target: 1 << 100,
        share_permille: 0,
        activation_daa: 0,
        admission: Box::new(PalwGenAdmissionCarriageV1 {
            class,
            registrant_bond: PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(3)),
            signature: vec![9; 4627],
        }),
    }
}

#[test]
fn the_object_is_appended_after_every_existing_tag() {
    let object = registration(class());
    let bytes = borsh::to_vec(&object).expect("encodes");
    assert_eq!(bytes[0], 68, "tag 68, after the second IR fence's 67: no earlier discriminant moves");
    let last = PalwConsensusObjectV2::CourtTirChildChosen {
        session_id: Hash64::from_bytes([1; 64]),
        choice: kaspa_consensus_core::palw_tir_dissect_v1::PalwTirDissectChoiceV1 {
            version: 1,
            session_id: Hash64::from_bytes([1; 64]),
            round: 0,
            child: 0,
        },
        signature: Vec::new(),
    };
    assert_eq!(borsh::to_vec(&last).unwrap()[0], 66, "the previous last variant keeps its tag");
    let back: PalwConsensusObjectV2 = borsh::from_slice(&bytes).expect("decodes");
    assert_eq!(back, object);
    assert!(palw_object_is_gen_v1(&object) && !palw_object_is_tir_v1(&object), "a generative object, not an IR one");
    assert!(!palw_object_is_gen_v1(&last));
}

#[test]
fn it_rides_statelessly_rents_nothing_and_takes_no_slot() {
    let object = registration(class());
    assert_eq!(palw_lifecycle_object_may_ride_v2(&object), Ok(()), "a block carrying it is valid on this build");
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).unwrap();
    assert!(validate_palw_lifecycle_tx(&payload, false).is_ok());
    assert!(validate_palw_lifecycle_tx(&payload, true).is_ok());
    assert_eq!(palw_object_rent_ceiling_v1(&object), 0, "no rent: an older build that skips it burns nothing either");
    assert_eq!(
        palw_class_registration_buyer_v1(&object),
        Some(PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(3))),
        "a bought registration, as an IR one (the walk drops it by name below palw_gen_v1 before any slot)"
    );
    assert!(!palw_h1_carrier_object_v1(&object), "a registration, not a conviction a halt must let through");

    // What an OLDER build meets: a tag its enum does not have — tolerated only under A-2.
    let mut unknown = payload.clone();
    unknown[2] = 70;
    assert!(borsh::from_slice::<PalwLifecycleTxPayloadV2>(&unknown).is_err(), "tag 70 is past this build's enum");
    assert!(validate_palw_lifecycle_tx(&unknown, true).is_ok(), "A-2: tolerated where palw_audit_2026_09_11 is declared");
    assert!(validate_palw_lifecycle_tx(&unknown, false).is_err(), "…and refused where it is not, which is why the fence needs it");
}

#[test]
fn below_the_fence_the_fold_refuses_it_by_name() {
    let p = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is V2") };
    for daa_score in [10, 2_500, u64::MAX - 1] {
        let ctx = PalwBlockContextV2 { block: Default::default(), daa_score, blue_score: 10, subsidy: 0 };
        let refused = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &bundle.state, &ctx, &[registration(class())], None);
        assert!(matches!(refused, Err(PalwStateV2Error::GenObjectRefused(_))), "at {daa_score}: {refused:?}");
    }
}

#[test]
fn no_genesis_registers_a_generative_class() {
    let mut p = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode else { panic!("V2") };
    assert!(!palw_genesis_registers_gen_class_v1(bundle));
    bundle.genesis_objects.push(registration(class()));
    assert!(palw_genesis_registers_gen_class_v1(bundle));
    let refused = p.validate_palw_v2().expect_err("a genesis generative row is refused");
    assert!(refused.to_string().contains("generative class"), "refused by name: {refused}");
}

#[test]
fn the_class_id_commits_to_every_part_of_the_class() {
    let c = class();
    let root = Hash64::from_bytes([0xA9; 64]);
    let id = c.class_id(&root);
    let (programs, pipeline) = c.decode().expect("the toy class decodes");
    assert_eq!(pipeline.encode(), c.pipeline, "the bytes are the pipeline");
    assert_eq!(programs.len(), c.programs.len());
    assert_ne!(id, c.class_id(&Hash64::from_bytes([0xAA; 64])), "the weights");
    let edits: Vec<(&str, Box<dyn Fn(&mut PalwGenClassV1)>)> = vec![
        ("the version", Box::new(|c| c.version += 1)),
        ("the profile", Box::new(|c| c.profile = PalwGenProfileV1::Video as u8)),
        ("the tokenizer", Box::new(|c| c.tokenizer_id = Hash64::from_bytes([0x72; 64]))),
        ("a layout", Box::new(|c| c.layouts[1].checkpoint_interval = 2)),
        ("the output header", Box::new(|c| c.output = OutputSpecV1::image_rgb8(2, 3))),
        ("the offered steps", Box::new(|c| c.offers.steps.pop().map(|_| ()).unwrap_or(()))),
        ("the offered prompt", Box::new(|c| c.offers.max_prompt_tokens -= 1)),
        ("a program", Box::new(|c| c.programs[2].push(0))),
        ("the pipeline", Box::new(|c| c.pipeline.push(0))),
    ];
    for (what, edit) in edits {
        let mut other = c.clone();
        edit(&mut other);
        assert_ne!(id, other.class_id(&root), "{what}");
    }
    // The pipeline root is over the pipeline and every program's graph_ir_root, in order.
    let mut swapped = c.clone();
    swapped.programs.swap(0, 1);
    assert_ne!(c.pipeline_root(), swapped.pipeline_root(), "the program order");
    assert_eq!(c.terms_digest(), swapped.terms_digest(), "the terms are not the programs");
}

#[test]
fn the_registration_message_binds_every_field_and_the_class() {
    let c = class();
    let bond = PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(3));
    let rule = PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 7 };
    let root = Hash64::from_bytes([0xA9; 64]);
    let domain = Hash64::from_bytes([0xD0; 64]);
    let m = |c: &PalwGenClassV1, share: u16, target: u128| {
        palw_gen_class_registration_message_v1(domain, c.class_id(&root), share, 0, &bond, root, 1, target, &rule, c)
    };
    let base = m(&c, 0, 1 << 100);
    assert_ne!(base, m(&c, 1, 1 << 100), "the share");
    assert_ne!(base, m(&c, 0, 1 << 99), "the target");
    let mut other = c.clone();
    other.offers.steps.truncate(1);
    assert_ne!(base, m(&other, 0, 1 << 100), "the class");
    let tir = kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1;
    assert_ne!(PALW_GEN_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1, tir, "its own signing context");
}

#[test]
fn the_toy_image_class_passes_the_preflight() {
    let c = class();
    let report = palw_gen_class_preflight_v1(&c, &fence()).unwrap_or_else(|e| panic!("the toy class: {e}"));
    assert_eq!(report.profile, PalwGenProfileV1::Image);
    assert_eq!(report.output_tile_len, Some(4), "the decoder's output tile");
    assert!(report.draws_randomness, "the denoiser draws the initial noise and a per-step jitter");
    assert_eq!(report.admission.stages.len(), 3);
    assert_eq!((report.admission.output_interval.lo, report.admission.output_interval.hi), (0, 255));
}

#[test]
fn the_preflight_refuses_by_name() {
    let ok = class();
    let f = fence();
    let refused = |edit: &dyn Fn(&mut PalwGenClassV1)| {
        let mut c = ok.clone();
        edit(&mut c);
        palw_gen_class_preflight_v1(&c, &f).expect_err("refused")
    };
    let is = |e: PalwGenClassErrorV1, pat: fn(&PalwGenClassErrorV1) -> bool| assert!(pat(&e), "{e}");
    // Versions and the profile.
    is(refused(&|c| c.version = 2), |e| matches!(e, PalwGenClassErrorV1::Version(_)));
    is(refused(&|c| c.layouts[0].version = 2), |e| matches!(e, PalwGenClassErrorV1::Version(_)));
    is(refused(&|c| c.profile = 0), |e| matches!(e, PalwGenClassErrorV1::Profile(0)));
    // The profile's output kind: an image pipeline registered as an embedding class.
    is(refused(&|c| c.profile = PalwGenProfileV1::Embedding as u8), |e| matches!(e, PalwGenClassErrorV1::Output(_)));
    // Programs: undecodable, unused, carried twice.
    is(refused(&|c| c.programs[0].push(0)), |e| matches!(e, PalwGenClassErrorV1::Program(_)));
    is(refused(&|c| c.pipeline.pop().map(|_| ()).unwrap()), |e| matches!(e, PalwGenClassErrorV1::Program(_)));
    is(refused(&|c| c.programs.push(c.programs[0].clone())), |e| matches!(e, PalwGenClassErrorV1::Programs(_)));
    // Layouts: count, context, tiles.
    is(refused(&|c| c.layouts.truncate(2)), |e| matches!(e, PalwGenClassErrorV1::Layout(_)));
    is(refused(&|c| c.layouts[1].max_context += 1), |e| matches!(e, PalwGenClassErrorV1::Layout(_)));
    is(refused(&|c| c.layouts[2].commit_tiles.push(4)), |e| matches!(e, PalwGenClassErrorV1::Layout(_)));
    is(refused(&|c| c.layouts[2].state_tiles.push(4)), |e| matches!(e, PalwGenClassErrorV1::Layout(_)));
    is(refused(&|c| c.layouts[0].commit_tiles[0] = 0), |e| matches!(e, PalwGenClassErrorV1::Layout(_)));
    is(refused(&|c| c.layouts[0].h_tile = 3), |e| matches!(e, PalwGenClassErrorV1::Layout(_)));
    // The output header: another shape, and an output tile the digest cannot use.
    is(refused(&|c| c.output = OutputSpecV1::image_rgb8(4, 1)), |e| matches!(e, PalwGenClassErrorV1::Output(_)));
    is(
        refused(&|c| {
            let n = c.layouts[2].commit_tiles.len();
            c.layouts[2].commit_tiles = vec![2; n];
        }),
        |e| matches!(e, PalwGenClassErrorV1::Output(_)),
    );
    // Offers: steps, scalars, prompts.
    is(refused(&|c| c.offers.steps.clear()), |e| matches!(e, PalwGenClassErrorV1::Offers(_)));
    is(refused(&|c| c.offers.steps = vec![2, 1]), |e| matches!(e, PalwGenClassErrorV1::Offers(_)));
    is(refused(&|c| c.offers.steps = vec![0, 1]), |e| matches!(e, PalwGenClassErrorV1::Offers(_)));
    is(refused(&|c| c.offers.steps.push(c.offers.steps.last().unwrap() + 1)), |e| matches!(e, PalwGenClassErrorV1::Offers(_)));
    is(refused(&|c| c.offers.steps = (1..=9).collect()), |e| matches!(e, PalwGenClassErrorV1::Offers(_)));
    is(refused(&|c| c.offers.scalars.push(PalwGenScalarOfferV1 { lo: 0, hi: 0 })), |e| matches!(e, PalwGenClassErrorV1::Offers(_)));
    is(refused(&|c| c.offers.scalars.truncate(1)), |e| matches!(e, PalwGenClassErrorV1::Offers(_)));
    is(refused(&|c| c.offers.scalars[0].hi += 1), |e| matches!(e, PalwGenClassErrorV1::Offers(_)));
    is(refused(&|c| c.offers.scalars[0] = PalwGenScalarOfferV1 { lo: 5, hi: 4 }), |e| matches!(e, PalwGenClassErrorV1::Offers(_)));
    is(refused(&|c| c.offers.max_prompt_tokens += 1), |e| matches!(e, PalwGenClassErrorV1::Offers(_)));
    is(refused(&|c| c.offers.max_negative_tokens = 1), |e| matches!(e, PalwGenClassErrorV1::Offers(_)));
    // The profile's ceilings: bytes, stages, and the job through the IR's admission.
    let report = palw_gen_class_preflight_v1(&ok, &f).unwrap();
    let under = |edit: &dyn Fn(&mut PalwGenFenceV1)| {
        let mut g = f;
        edit(&mut g);
        palw_gen_class_preflight_v1(&ok, &g).expect_err("refused")
    };
    let bytes = ok.carried_bytes();
    is(under(&|g| g.ceilings.image.max_class_bytes = bytes as u32 - 1), |e| {
        matches!(e, PalwGenClassErrorV1::Exceeds { what: "class bytes", .. })
    });
    is(under(&|g| g.ceilings.image.max_stages = 2), |e| matches!(e, PalwGenClassErrorV1::Exceeds { what: "stages", .. }));
    let leaves = report.admission.job_step_leaves;
    is(under(&|g| g.ceilings.image.max_job_step_leaves = leaves - 1), |e| {
        matches!(e, PalwGenClassErrorV1::AdmissionExceeds { limit: "max_job_step_leaves", .. })
    });
    is(under(&|g| g.ceilings.image.max_position_step_leaves = 1), |e| {
        matches!(e, PalwGenClassErrorV1::AdmissionExceeds { limit: "max_step_leaves", .. })
    });
    // Another profile's ceilings do not apply to an image class.
    let mut g = f;
    g.ceilings.video.max_stages = 1;
    assert!(palw_gen_class_preflight_v1(&ok, &g).is_ok(), "the image class is judged under the image ceilings");
}

/// **G9 (the independent second implementation's finding), at the door a registrant reaches it
/// from.** A class's output header is the registrant's: every dimension legal (`≤ 2^24`) and the
/// element count past `u64` made `OutputSpecV1::layout` panic with a multiply overflow before the
/// `2^28` bound ran, and the class preflight calls it on the header as carried. A video class's
/// header is the one whose kind the preflight lets through to that call (an image class's rank-3
/// header cannot overflow `u64`), so the overflowing headers below are video and tensor headers; each
/// is a named `Output` refusal and never a panic.
#[test]
fn an_output_header_whose_element_count_overflows_is_refused_by_name() {
    let ok = class();
    let f = fence();
    let big = 1u32 << 24;
    for (what, profile, spec) in [
        ("video [2^24, 2^24, 2^24, 3]", PalwGenProfileV1::Video, OutputSpecV1::video_rgb8(big, big, big, 30, 1)),
        ("video [2^20, 2^20, 2^20, 3]", PalwGenProfileV1::Video, OutputSpecV1::video_rgb8(1 << 20, 1 << 20, 1 << 20, 30, 1)),
        ("an image [2^24, 2^24, 3]", PalwGenProfileV1::Image, OutputSpecV1::image_rgb8(big, big)),
        ("an embedding [2^24, 2^24]", PalwGenProfileV1::Embedding, OutputSpecV1::embedding_i32(big, big, 0, false)),
        ("audio [2^24, 2^24]", PalwGenProfileV1::Audio, OutputSpecV1::pcm_i16(big, big, 48_000)),
    ] {
        let mut c = ok.clone();
        c.profile = profile as u8;
        c.output = spec;
        let e = palw_gen_class_preflight_v1(&c, &f).expect_err(what);
        assert!(matches!(e, PalwGenClassErrorV1::Output(_)), "{what}: {e}");
    }
}

/// The toy vision pipeline (`toy-vision.json`) as an Embedding class with one image slot.
fn vision_class() -> PalwGenClassV1 {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines/toy-vision.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the vision vector")).expect("json");
    let pipeline = unhex(v["pipeline_borsh_hex"].as_str().unwrap());
    let programs: Vec<Vec<u8>> =
        v["programs"].as_array().unwrap().iter().map(|p| unhex(p["program_borsh_hex"].as_str().unwrap())).collect();
    let (p, progs) = decoded(&pipeline, &programs);
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
        layouts: layouts(&p, &progs),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: 0,
            max_negative_tokens: 0,
            images: vec![slot],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
        },
        output: OutputSpecV1::embedding_i32(1, 4, 0, false),
        pipeline,
        programs,
        tokenizer_id: Hash64::from_bytes([0x71; 64]),
    }
}

#[test]
fn an_image_class_declares_its_slots_and_its_id_covers_them() {
    let c = vision_class();
    let report = palw_gen_class_preflight_v1(&c, &fence()).unwrap_or_else(|e| panic!("the vision class: {e}"));
    assert_eq!(report.profile, PalwGenProfileV1::Embedding);
    assert!(!report.draws_randomness, "an encoder draws no R: its jobs' seeds are zero (PALW-GEN-6)");
    let slot = c.offers.images[0];
    assert_eq!((slot.h, slot.w, slot.tile_len), (2, 3, 4));
    // The slots are the pipeline's bound images exactly, each with a tile a digest can use.
    let refused = |edit: &dyn Fn(&mut PalwGenClassV1)| {
        let mut other = c.clone();
        edit(&mut other);
        palw_gen_class_preflight_v1(&other, &fence()).expect_err("refused")
    };
    for (what, e) in [
        ("no slot", refused(&|c| c.offers.images.clear())),
        (
            "a slot no stage reads",
            refused(&|c| c.offers.images.push(PalwGenImageOfferV1 { h: 2, w: 3, tile_len: 4, token_equivalents: 0 })),
        ),
        ("another size", refused(&|c| c.offers.images[0] = PalwGenImageOfferV1 { h: 3, w: 2, tile_len: 4, token_equivalents: 0 })),
        ("a tile under 4", refused(&|c| c.offers.images[0].tile_len = 3)),
        ("a tile over 2^16", refused(&|c| c.offers.images[0].tile_len = (1 << 16) + 1)),
    ] {
        assert!(matches!(e, PalwGenClassErrorV1::Offers(_)), "{what}: {e}");
    }
    // A slot declared on a class whose pipeline binds no image.
    let mut text = class();
    text.offers.images.push(slot);
    assert!(matches!(palw_gen_class_preflight_v1(&text, &fence()), Err(PalwGenClassErrorV1::Offers(_))));
    // The class id covers every slot's size and tile.
    let root = Hash64::from_bytes([0xA9; 64]);
    let id = c.class_id(&root);
    for edit in [|c: &mut PalwGenClassV1| c.offers.images[0].tile_len = 8, |c: &mut PalwGenClassV1| c.offers.images[0].h = 4] {
        let mut other = c.clone();
        edit(&mut other);
        assert_ne!(id, other.class_id(&root));
    }
}

#[test]
fn a_job_carries_exactly_one_image_per_slot_at_its_size() {
    let offers = vision_class().offers;
    let image = |h, w| PalwGenImageInputRefV1 { input_root: Hash64::from_bytes([0x33; 64]), h, w };
    assert_eq!(palw_gen_job_images_admitted_v1(&offers, &[image(2, 3)]), Ok(()));
    assert_eq!(palw_gen_job_images_admitted_v1(&offers, &[]), Err(PalwGenJobImageErrorV1::ImageCount { want: 1, got: 0 }));
    assert_eq!(
        palw_gen_job_images_admitted_v1(&offers, &[image(2, 3), image(2, 3)]),
        Err(PalwGenJobImageErrorV1::ImageCount { want: 1, got: 2 })
    );
    assert_eq!(
        palw_gen_job_images_admitted_v1(&offers, &[image(3, 2)]),
        Err(PalwGenJobImageErrorV1::ImageSizeNotOffered { index: 0, h: 2, w: 3, got_h: 3, got_w: 2 })
    );
    // The reference is what the job carries: its root and size, never the bytes.
    let bytes = borsh::to_vec(&image(2, 3)).unwrap();
    assert_eq!(bytes.len(), 64 + 4 + 4);
}

/// A pipeline of `toy-text.json`-shaped or `toy-vlm.json` vectors as a Text class.
fn text_class(vector: &str, images: Vec<PalwGenImageOfferV1>) -> PalwGenClassV1 {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines").join(vector);
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the vector")).expect("json");
    let pipeline = unhex(v["pipeline_borsh_hex"].as_str().unwrap());
    let programs: Vec<Vec<u8>> =
        v["programs"].as_array().unwrap().iter().map(|p| unhex(p["program_borsh_hex"].as_str().unwrap())).collect();
    let (p, progs) = decoded(&pipeline, &programs);
    let max_trip = p.stages[p.output_stage as usize].max_trip;
    PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        layouts: layouts(&p, &progs),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: 8,
            max_negative_tokens: 0,
            images,
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
        },
        output: OutputSpecV1::tokens(max_trip),
        pipeline,
        programs,
        tokenizer_id: Hash64::from_bytes([0x72; 64]),
    }
}

/// The toy VLM class, its image priced well above any floor.
fn vlm_class() -> PalwGenClassV1 {
    text_class("toy-vlm.json", vec![PalwGenImageOfferV1 { h: 2, w: 3, tile_len: 4, token_equivalents: 1_000_000 }])
}

#[test]
fn a_vision_language_class_is_a_text_class_with_image_slots() {
    let c = vlm_class();
    let report = palw_gen_class_preflight_v1(&c, &fence()).unwrap_or_else(|e| panic!("the VLM class: {e}"));
    assert_eq!(report.profile, PalwGenProfileV1::Text);
    assert_eq!(report.output_tile_len, None, "its output is the generated ids: no output root");
    assert_eq!(report.admission.stages.len(), 2);
    assert_eq!(report.admission.stages[1].max_trip, 12, "the text stage at max_trip positions");
    let refused = |edit: &dyn Fn(&mut PalwGenClassV1)| {
        let mut other = c.clone();
        edit(&mut other);
        palw_gen_class_preflight_v1(&other, &fence()).expect_err("refused")
    };
    for (what, e) in [
        ("as an Embedding class", refused(&|c| c.profile = PalwGenProfileV1::Embedding as u8)),
        ("its output as 11 ids", refused(&|c| c.output = OutputSpecV1::tokens(11))),
        ("its output as an embedding", refused(&|c| c.output = OutputSpecV1::embedding_i32(1, 4, 0, false))),
    ] {
        assert!(matches!(e, PalwGenClassErrorV1::Output(_)), "{what}: {e}");
    }
    for (what, e) in [
        ("a prompt longer than the stream", refused(&|c| c.offers.max_prompt_tokens = 13)),
        ("no prompt", refused(&|c| c.offers.max_prompt_tokens = 0)),
        ("no image slot", refused(&|c| c.offers.images.clear())),
    ] {
        assert!(matches!(e, PalwGenClassErrorV1::Offers(_)), "{what}: {e}");
    }
    // A pipeline without a text stage is not a Text class.
    let mut vision = vision_class();
    vision.profile = PalwGenProfileV1::Text as u8;
    vision.output = OutputSpecV1::tokens(1);
    assert!(matches!(palw_gen_class_preflight_v1(&vision, &fence()), Err(PalwGenClassErrorV1::Output(_))));
    // The class id covers the profile: the same pipeline as another profile is another class.
    let root = Hash64::from_bytes([0xA9; 64]);
    let mut other = c.clone();
    other.profile = PalwGenProfileV1::Image as u8;
    assert_ne!(c.class_id(&root), other.class_id(&root));
}

#[test]
fn a_text_only_pipeline_is_a_text_class() {
    let (p, progs) = {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/admission.json");
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let case =
            v["pipelines"].as_array().unwrap().iter().find(|c| c["name"].as_str().unwrap().starts_with("toy-text")).unwrap().clone();
        let programs: Vec<Vec<u8>> =
            case["programs_borsh_hex"].as_array().unwrap().iter().map(|h| unhex(h.as_str().unwrap())).collect();
        (unhex(case["pipeline_borsh_hex"].as_str().unwrap()), programs)
    };
    let (pipeline, decoded_progs) = decoded(&p, &progs);
    let c = PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        layouts: layouts(&pipeline, &decoded_progs),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: 12,
            max_negative_tokens: 0,
            images: vec![],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
        },
        output: OutputSpecV1::tokens(12),
        pipeline: p,
        programs: progs,
        tokenizer_id: Hash64::from_bytes([0x72; 64]),
    };
    let report = palw_gen_class_preflight_v1(&c, &fence()).unwrap_or_else(|e| panic!("the text class: {e}"));
    assert_eq!((report.profile, report.output_tile_len, report.draws_randomness), (PalwGenProfileV1::Text, None, false));
}

/// **An image's price in prompt tokens** (RFC-0003 open question 13's recommendation, pending user
/// confirmation): declared per slot, floored at `⌈admitted per-image work / per-token work⌉`, and
/// declared only by a text class.
#[test]
fn a_text_class_prices_each_image_at_or_above_its_token_floor() {
    let c = vlm_class();
    let report = palw_gen_class_preflight_v1(&c, &fence()).unwrap();
    let a = &report.admission;
    let per_image = palw_gen_work_units_v1(&a.stages[0].job_cost);
    let per_token = palw_gen_work_units_v1(&a.stages[1].admission.view.position.cost);
    let floor = per_image.div_ceil(per_token) as u32;
    assert!(per_image > 0 && per_token > 0);
    assert_eq!(report.image_token_floor, vec![floor], "⌈{per_image} / {per_token}⌉");
    let priced = |tokens: u32| {
        let mut p = c.clone();
        p.offers.images[0].token_equivalents = tokens;
        palw_gen_class_preflight_v1(&p, &fence())
    };
    assert!(priced(floor.max(1)).is_ok(), "at the floor");
    assert!(matches!(priced(floor.max(1) - 1), Err(PalwGenClassErrorV1::Offers(_))), "one token below it");
    // The price is in the class id: another price is another class.
    let root = Hash64::from_bytes([0xA9; 64]);
    let mut other = c.clone();
    other.offers.images[0].token_equivalents += 1;
    assert_ne!(c.class_id(&root), other.class_id(&root));
    // A class that is not a text class declares no token price.
    let mut vision = vision_class();
    vision.offers.images[0].token_equivalents = 1;
    assert!(matches!(palw_gen_class_preflight_v1(&vision, &fence()), Err(PalwGenClassErrorV1::Offers(_))));
    assert!(palw_gen_class_preflight_v1(&vision_class(), &fence()).unwrap().image_token_floor.is_empty());
}
