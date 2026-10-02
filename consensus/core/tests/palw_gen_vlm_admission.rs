//! **RFC-0003 §II.2.1: real vision-language stages are admissible, their attention dissected** —
//! RFC-0002 F7 composed into the pipeline admission.
//!
//! The admission targets are tir-lower's lowered tiny HF VLMs (`rfc3/lower` 4c25416a5, the tower then
//! the language model as the text stage over `TextStream`, `max_trip` 64):
//! `consensus-vectors/tir-v2/pipelines/vlm-{llava,qwen2-vl,qwen2.5-vl}-tiny.json`. Each LM's attention
//! cones reduce over the history; under the k-ary court (armed wherever `palw_tir_v1` is) each such
//! commit point is admitted through F7's obligations, value bound, round and root-claim sizing and
//! window — the class records the points, and owes its court's terminal move there.

#[path = "rcore_common.rs"]
mod rcore;
use rcore::*;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
use kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError;
use kaspa_consensus_core::palw_gen_admission_v1::*;
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::pipeline::TirPipelineV1;
use misaka_palw_tir::program_v2::TirProgramV2;

const AT: u64 = 1_100;

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

/// A lowered VLM as a registered Text class: one image slot at the fixture's size, a prompt the
/// stream holds, layouts of `tile`-lane commit tiles, `h_tile`-position history tiles and a
/// checkpoint every `checkpoint` positions.
fn vlm_class(vector: &str, tile: u32, h_tile: u32, checkpoint: u32) -> PalwGenClassV1 {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines").join(vector);
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the vector")).expect("json");
    let program_bytes: Vec<Vec<u8>> =
        v["programs"].as_array().unwrap().iter().map(|p| unhex(p["program_borsh_hex"].as_str().unwrap())).collect();
    let programs: Vec<TirProgramV2> =
        program_bytes.iter().map(|b| TirProgramV2::decode_canonical(b).expect("a canonical program")).collect();
    let pipeline_bytes = unhex(v["pipeline_borsh_hex"].as_str().unwrap());
    let pipeline = TirPipelineV1::decode_canonical(&pipeline_bytes, &programs).expect("a canonical pipeline");
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
    let image = &v["job"]["images"][0];
    let (h, w) = (image["h"].as_u64().unwrap() as u32, image["w"].as_u64().unwrap() as u32);
    PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        pipeline: pipeline_bytes,
        programs: program_bytes,
        layouts,
        output: OutputSpecV1::tokens(pipeline.stages[pipeline.output_stage as usize].max_trip),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: 32,
            max_negative_tokens: 0,
            images: vec![PalwGenImageOfferV1 { h, w, tile_len: 64, token_equivalents: 1_000_000 }],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
            profile: PalwGenProfileOffersV1::None,
        },
        tokenizer_id: Hash64::from_bytes([0x74; 64]),
    }
}

fn register(class: PalwGenClassV1) -> kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 {
    let bond = PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(3));
    palw_gen_post_genesis_registration_v1(class, Hash64::from_bytes([0xA7; 64]), 0, 1 << 100, 1, AT, bond, vec![]).expect("counted")
}

const TARGETS: [&str; 3] = ["vlm-llava-tiny.json", "vlm-qwen2-vl-tiny.json", "vlm-qwen2.5-vl-tiny.json"];

#[test]
fn the_lowered_vlm_stages_are_admitted_with_their_attention_dissected() {
    let p = armed();
    let bundle = bundle(&p);
    let one_move = PalwGenAdmissionRulesV1::at(&p, AT).expect("the fence is in force");
    assert!(one_move.court.is_some(), "the k-ary court is armed wherever palw_tir_v1 is");
    // PALW-GEN-20: a close is priced as the builder carries it. The lowered stages' activation tables are four 32 KiB artifact pieces
    // a tile's lookups can ALL land in, so a close at the activation is past one carrier (~147 KB): such a class is convictable only
    // where closes may ride chunks (`palw_held_close_chunks_v1`, PALW-TIR-38's carried bound) and is refused by name where they may
    // not (PALW-GEN-21) — the HF frontend's `LowerOpts::table_shift` makes the table two pieces.
    let rules = PalwGenAdmissionRulesV1 { held_close_chunks: true, ..one_move };
    for vector in TARGETS {
        let object = register(vlm_class(vector, 16, 16, 8));
        match verify_gen_class_admission_v1(&bundle, &one_move, &object) {
            Ok(_) => {}
            Err(PalwClassAdmissionError::CourtCostExceedsCeiling { what, got, ceiling }) => {
                assert_eq!(what, "generative close bytes as carried", "{vector}");
                assert!(got > ceiling, "{vector}: {got} B against {ceiling} B");
            }
            Err(e) => panic!("{vector}: {e}"),
        }
        let admitted = verify_gen_class_admission_v1(&bundle, &rules, &object).unwrap_or_else(|e| panic!("{vector}: {e}"));
        let dissected = &admitted.record.dissected;
        eprintln!(
            "{vector}: admitted — {} step leaves at the widest job, pwu {}; {} dissected commit points {:?}",
            admitted.entry.max_step_leaf_count,
            admitted.entry.canonical_step_leaf_count,
            dissected.len(),
            &dissected[..dissected.len().min(6)]
        );
        assert!(!dissected.is_empty(), "{vector}: the LM's attention reduces over the history");
        assert!(dissected.iter().all(|(stage, _, _)| *stage == 1), "{vector}: only the text stage has a history");
        assert_eq!(admitted.report.profile, PalwGenProfileV1::Text);
    }
}

#[test]
fn without_the_court_a_history_cone_must_fit_whole() {
    let p = armed();
    let bundle = bundle(&p);
    let armed_rules = PalwGenAdmissionRulesV1::at(&p, AT).unwrap();
    // Closes may ride chunks here: the question is the court's, not the carrier's (see the test above).
    let rules = PalwGenAdmissionRulesV1 { court: None, held_close_chunks: true, ..armed_rules };
    for vector in TARGETS {
        let object = register(vlm_class(vector, 16, 16, 8));
        match verify_gen_class_admission_v1(&bundle, &rules, &object) {
            // A tiny model's attention may fit the court whole; then nothing needed dissecting.
            Ok(_) => eprintln!("{vector}: fits whole at tiny scale"),
            Err(PalwClassAdmissionError::GenNeedsDissection { stage, .. }) => assert_eq!(stage, 1, "{vector}"),
            Err(e) => panic!("{vector}: {e}"),
        }
    }
}

#[test]
fn a_dissected_class_owes_its_terminal_move() {
    // The fold records a class with dissected points as one whose court ends in a root claim at a
    // dissected leaf (`fused_attention`), as for an IR class.
    let mut chain = Chain::new(armed());
    chain.room = true;
    let (registrant, _, _) = floor_producer(&chain.p);
    let (floor, _, target, slash) = genesis_classes(&chain.p)[0];
    let target = chain.s.class_target(&floor).map(|t| t.target).unwrap_or(target);
    let class = vlm_class("vlm-llava-tiny.json", 16, 16, 8);
    let object =
        palw_gen_post_genesis_registration_v1(class, Hash64::from_bytes([0xA7; 64]), 0, target, slash, AT, registrant, vec![9; 16])
            .unwrap();
    let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredGenV1 { class_id, .. } = &object else {
        unreachable!()
    };
    let class_id = *class_id;
    chain.step_at(AT, &[object], PalwBlockWorkV3::None, Hash64::default(), 0);
    let row = chain.s.gen_class_v1(&class_id).expect("the row");
    assert!(!row.dissected.is_empty());
    assert!(chain.s.class(&class_id).expect("the class").fused_attention, "it owes a root claim at a dissected leaf");
}

/// ref2's H7 shape (`tests/palw_tir_h7.rs`) as a text stage: the scores `Qᵀ · K` over a 16-row window, then `TopK { axis 0, k 4 }` of
/// them, committed in tiles of 4 lanes — a dissected cone with a `TopK`. `rows` adds the vision-language class's image-rows input (the
/// golden toy VLM's: `[2, 4]` `I32`, through a `StageFinal` edge), which makes the stage the language model of a generative class.
fn topk_lm_program(rows: bool) -> misaka_palw_tir::program_v2::TirProgramV2 {
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::program_v2::{InputSource, OutputDecl};
    use misaka_palw_tir::{DType, Ref, TensorType};
    let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
    let image_rows = rows.then(|| pb.param("lm.image_rows", DType::I32, &[2, 4], false));
    let embed = pb.param("embed", DType::I8, &[8, 7], false);
    let head = pb.param("head", DType::I8, &[8, 7], false);
    let ks = pb.hist_state("k", DType::I32, &[3], 16, true);
    let qs = pb.hist_state("q", DType::I32, &[4], 16, true);
    let carry = vec![TensorType::fixed(DType::I32, &[7])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(embed, Ref::Input(0), 0, 0);
        let x = b.cast(x, DType::I32);
        // The vision-language class's stage reads the first image row (the tower's output, shifted down as the golden toy language
        // model reads its rows) into the first four lanes of the embedding.
        let x = match image_rows {
            None => x,
            Some(rows) => {
                let zero = b.c(DType::Idx, 0);
                let row = b.gather(rows, zero, 0, 0);
                let row = b.shr(row, 16, misaka_palw_tir::Rounding::HalfAwayFromZero, DType::I32);
                let row = b.clamp(row, -128, 127, DType::I32);
                let head4 = b.slice(x, 0, 0, 4);
                let tail3 = b.slice(x, 0, 4, 3);
                let mixed = b.add(head4, row, DType::I32);
                b.concat(&[mixed, tail3], 0)
            }
        };
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let rk = b.slice(Ref::CarryIn(0), 0, 0, 3);
        let rk = b.clamp(rk, -127, 127, DType::I32);
        let k = b.hist_append(ks, rk);
        let rq = b.slice(Ref::CarryIn(0), 0, 3, 4);
        let rq = b.clamp(rq, -127, 127, DType::I32);
        let q = b.hist_append(qs, rq);
        let qt = b.transpose(q, &[1, 0]);
        let s = b.matmul(qt, k, DType::I64);
        let s = b.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.topk(s, 0, 4);
        let y = b.cast(Ref::CarryIn(0), DType::I32);
        b.finish(&[y])
    };
    let (post, logits) = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[7, 1]);
        let l = b.matmul(head, x, DType::I64);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let l = b.reshape_fixed(l, &[8]);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let v1 = pb.finish(pre, vec![layer], post, logits);
    let lifted: Vec<(u16, InputSource)> =
        if rows { vec![(0, InputSource::External { lo: i32::MIN as i64, hi: i32::MAX as i64 })] } else { vec![] };
    TirProgramV2::from_v1_lifting_params(&v1, &lifted, OutputDecl::Logits { node: logits, scheme_id: v1.logits_scheme_id })
        .expect("a text program")
}

/// The offers of a text class of this file (no steps, no scalars, a prompt of 8, no source), with `images` slots.
fn text_offers(images: Vec<PalwGenImageOfferV1>) -> PalwGenOffersV1 {
    PalwGenOffersV1 {
        steps: vec![],
        scalars: vec![],
        max_prompt_tokens: 8,
        max_negative_tokens: 0,
        images,
        max_source_tokens: 0,
        forced_prompt_prefix: vec![],
        source_token_floor: 0,
        profile: PalwGenProfileOffersV1::None,
    }
}

/// One tile of 4 lanes per commit point and state of every stage, a checkpoint every 4 positions, a history tile of 16.
fn tiny_layouts(
    pipeline: &misaka_palw_tir::pipeline::TirPipelineV1,
    programs: &[misaka_palw_tir::program_v2::TirProgramV2],
) -> Vec<PalwTirLayoutV1> {
    pipeline
        .stages
        .iter()
        .map(|st| {
            let p = &programs[st.program as usize];
            let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
            PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: st.max_trip,
                checkpoint_interval: 4,
                h_tile: 16,
                commit_tiles: vec![4; commits],
                state_tiles: vec![4; p.states.len()],
            }
        })
        .collect()
}

/// **A text-only class**: the H7 language model alone, no image slot and no source — a text-only model, whose jobs are FP Job V4.
fn text_only_class() -> PalwGenClassV1 {
    use misaka_palw_tir::pipeline::{StageDecl, TIR_PIPELINE_VERSION_V1, TripRule};
    let program = topk_lm_program(false);
    let pipeline = TirPipelineV1 {
        version: TIR_PIPELINE_VERSION_V1,
        stages: vec![StageDecl { name: "lm".into(), program: 0, trip: TripRule::TextStream, max_trip: 16, tokens: None, bind: vec![] }],
        output_stage: 0,
    };
    PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        layouts: tiny_layouts(&pipeline, std::slice::from_ref(&program)),
        pipeline: pipeline.encode(),
        programs: vec![program.encode()],
        output: OutputSpecV1::tokens(16),
        offers: text_offers(vec![]),
        tokenizer_id: Hash64::from_bytes([0x74; 64]),
    }
}

/// **The H7 language model as a vision-language class**: the golden toy VLM's vision stage (a 2×3 image slot, `[2, 4]` rows out), then
/// the H7 stage over the text stream reading them through a `StageFinal` edge — a text class with an image slot, so one that takes FP Job
/// V5 and can be claimed.
fn topk_class() -> PalwGenClassV1 {
    use misaka_palw_tir::pipeline::{Binding, StageDecl, TIR_PIPELINE_VERSION_V1, TripRule};
    use misaka_palw_tir::program_v2::TirProgramV2;
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines/toy-vlm.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the golden toy VLM")).expect("json");
    let vision = TirProgramV2::decode_canonical(&unhex(v["programs"][0]["program_borsh_hex"].as_str().unwrap())).expect("the tower");
    let programs = vec![vision, topk_lm_program(true)];
    let pipeline = TirPipelineV1 {
        version: TIR_PIPELINE_VERSION_V1,
        stages: vec![
            StageDecl {
                name: "vision".into(),
                program: 0,
                trip: TripRule::Fixed { n: 1 },
                max_trip: 1,
                tokens: None,
                bind: vec![Binding::JobImage { index: 0 }],
            },
            StageDecl {
                name: "lm".into(),
                program: 1,
                trip: TripRule::TextStream,
                max_trip: 16,
                tokens: None,
                bind: vec![Binding::StageFinal { stage: 0 }],
            },
        ],
        output_stage: 1,
    };
    PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        layouts: tiny_layouts(&pipeline, &programs),
        pipeline: pipeline.encode(),
        programs: programs.iter().map(|p| p.encode()).collect(),
        output: OutputSpecV1::tokens(16),
        offers: text_offers(vec![PalwGenImageOfferV1 { h: 2, w: 3, tile_len: 4, token_equivalents: 1_000_000 }]),
        tokenizer_id: Hash64::from_bytes([0x74; 64]),
    }
}

/// **A text-only class is a dead registration: refused by name under the gen fence, and pointed at the text lane.** A generative text
/// class takes FP Job V5 (images or a source, never neither) and the generative court binds V5 jobs alone, so a text class that offers
/// neither takes FP Job V4 — the text lane's — and would be registered and never claimed, costing its registrant for nothing. Below
/// `palw_gen_v1` the registration is dropped by name like every generative registration; from it the gate's first step refuses it, whatever
/// the court and the carrier would otherwise allow; and the same model behind an image slot is admitted.
#[test]
fn a_text_only_class_is_refused_by_name_under_the_gen_fence_and_pointed_at_the_text_lane() {
    let p = armed();
    let bundle = bundle(&p);
    let object = register(text_only_class());
    // Below the fence: dropped by name, as every generative registration is.
    assert_eq!(
        palw_gen_registration_preflight_at_v1(&p, &bundle, &object, AT - 1).err(),
        Some(PalwClassAdmissionError::GenNeedsItsFence),
        "below the fence"
    );
    // From it: refused by name at the first step, as a preflight refusal, with the lane it belongs on.
    let rules = PalwGenAdmissionRulesV1::at(&p, AT).expect("the fence is in force");
    let refused = palw_gen_registration_preflight_at_v1(&p, &bundle, &object, AT).err().expect("a text-only class is refused");
    let PalwClassAdmissionError::GenClass(said) = &refused else { panic!("refused as a preflight refusal, by name: {refused:?}") };
    assert_eq!(said, &PalwGenClassErrorV1::TextOnly.to_string());
    assert_eq!(refused.code(), "GEN_CLASS_REFUSED");
    for needle in ["text-only", "FP Job V4", "text lane", "tir-registration", "FP Job V5", "image slot or a source"] {
        assert!(said.contains(needle), "the refusal should say {needle:?}: {said}");
    }
    // Nothing the court or the carrier allows lets it through: it is the class that is dead, not its closes.
    for rules in [
        PalwGenAdmissionRulesV1 { held_close_chunks: true, ..rules },
        PalwGenAdmissionRulesV1 { court: None, ..rules },
        PalwGenAdmissionRulesV1 { held_armed: false, ..rules },
    ] {
        assert_eq!(verify_gen_class_admission_v1(&bundle, &rules, &object).err(), Some(refused.clone()));
    }
    // The same model behind an image slot is a generative text class (the H7 class below, admitted past the second IR fence).
    assert!(palw_gen_class_preflight_v1(&topk_class(), &rules.fence).is_ok(), "an image slot makes it a generative text class");
    assert_eq!(palw_gen_class_preflight_v1(&text_only_class(), &rules.fence), Err(PalwGenClassErrorV1::TextOnly));
}

/// **A `TopK` in a dissected cone** (ref2's H7): below `palw_tir_fence2` the release's box-demand row
/// understates the tile, so the class is refused by name; past it the block's rules are H7's, the
/// value bound covers the tile, and the class is admitted with the TopK's cone dissected.
#[test]
fn a_topk_in_a_dissected_cone_is_refused_below_the_second_ir_fence_and_sized_by_h7_past_it() {
    use kaspa_consensus_core::palw_tir_fence2_v1::PalwTirDemandRulesV1;
    let below = armed();
    let object = register(topk_class());
    let rules = PalwGenAdmissionRulesV1::at(&below, AT).unwrap();
    assert_eq!(rules.demand, PalwTirDemandRulesV1::Release2000);
    let refused = verify_gen_class_admission_v1(&bundle(&below), &rules, &object).err().expect("refused below the fence");
    assert!(
        matches!(&refused, PalwClassAdmissionError::GenDissection { why, .. } if why.contains("TopK")),
        "the release's row: {refused:?}"
    );
    let mut past = armed();
    past.palw_tir_fence2 = Some(ForkActivation::new(AT));
    past.sync_palw_tir_fence2();
    past.validate_palw_v2().unwrap_or_else(|e| panic!("the second IR fence at {AT}: {e}"));
    let rules = PalwGenAdmissionRulesV1::at(&past, AT).unwrap();
    assert_eq!(rules.demand, PalwTirDemandRulesV1::H7);
    let admitted = verify_gen_class_admission_v1(&bundle(&past), &rules, &object).unwrap_or_else(|e| panic!("past the fence: {e}"));
    assert!(!admitted.record.dissected.is_empty(), "the TopK's cone is admitted dissected");
}
