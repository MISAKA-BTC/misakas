//! **On testnet-12 the model court window (`palw_model_court_window`) changes no admission and no window** — the
//! release owner's check of 2026-10-01, and the reason the fence is armed NOWHERE on testnet-12 (the coordinator's
//! decision of the same day: the DAA-3,600 flag day carries `palw_tir_fence2` alone; the window code, its fence list
//! and `--palw-drill-model-court-at` stay for a ruleset that charges the ladder clock).
//!
//! The claim: testnet-12 arms the held fence from genesis, and admission charges the NETWORK's regime
//! (`verify_class_admission_*`: `if held.armed { palw_attn_court_admits_row_held_v1 } else { palw_attn_court_admits_row_v1 }`),
//! so no class's window ever carries the leaf ladder's rounds. At testnet-12's own court the held clock needs at
//! most 2,919 of the network's 3,000 DAA for a history of 2^32 positions (tile 16), so the window the fence derives,
//! `max(network window, W(shape))`, IS the network window for every class — and every admission verdict is the same
//! with the fence armed and unarmed.
//!
//! * **where 5,102 comes from** — `palw_attn_court_admits_row_v1`, the LADDER clock: `(2·(L + H) + t + root claim) ·
//!   turn_deadline` moves plus the assembly reserve, reported as `CourtWindowTooShort { needed: moves·deadline +
//!   reserve }` (`palw_class_admission_v2`): at testnet-12's court (L = 40 leaf-ladder rounds, turn 42, terminal 2,
//!   reserve 1,616) and a history of at most one tile (H = 0, root claim 1) that is 83 × 42 + 1,616 = **5,102** —
//!   more than 3,000 for EVERY class under that clock, which is why a fence relaxing it would matter on a ruleset
//!   that charges it (a network without the held fence; the fit tool's `Shipped` regime);
//! * **why testnet-12 never charges it** — `held.armed` is the network's `palw_held_context` at the registering
//!   block, `true` from genesis here, and the held clock (`palw_attn_court_admits_row_held_v1`) has no leaf ladder:
//!   the fold refuses `CourtOpened` for every claim under the held regime (`BisectionRefusedUnderHeldContext`), so no
//!   dispute plays one;
//! * **the proof** — legacy classes (the shipped hybrid and dense rows, a Qwen3.5-9B-shaped hybrid at 262,144
//!   positions, a 2^32-position hybrid and dense) through the acceptance path's own gate, and IR classes (dissected
//!   cones at several contexts, up to the widest the registration builder takes), each with the fence armed and
//!   unarmed: the same shape, the same window, the same verdict.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_t12_court_window_changes_no_admission -- --nocapture`

#[path = "palw_tir_fixture_common.rs"]
#[allow(unused)]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_MODEL_COURT_WINDOW_FENCES_V1, Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1};
use kaspa_consensus_core::palw_attn_court_v1::{
    PalwAttnCourtError, palw_attn_court_admits_row_held_v1, palw_attn_court_admits_row_v1,
};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_class_admission_v2::{
    PalwClassAdmissionError, palw_admission_shape_at_v1, palw_class_court_window_at_v1, palw_class_court_window_for_shape_v1,
    verify_class_admission_v9,
};
use kaspa_consensus_core::palw_context_ladder::palw_close_assembly_daa_v1;
use kaspa_consensus_core::palw_court_v2::palw_court_params_held_at_v2;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_qwen25_profile::{
    PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1,
};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwPwuRuleV2};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;
use kaspa_consensus_core::palw_tir_admission_v1::{
    PalwTirAdmissionRulesV1, palw_tir_post_genesis_registration_v1, verify_class_admission_v10,
};
use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_operands_v1;
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use misaka_palw_tir::{MapParams, TirProgramV1};

/// A height past every fence the rulesets below schedule: the armed variant's window is in force here.
const DAA: u64 = 5_000;

/// testnet-12 with the model court window ARMED at `at` — or UNARMED (`None`) — through the entry's own `set`,
/// whatever the shipped ruleset itself carries.
fn t12_with_window(at: Option<u64>) -> Params {
    let mut p = palw_t12_shipped_params();
    for f in PALW_T12_MODEL_COURT_WINDOW_FENCES_V1 {
        (f.set)(&mut p, at.map(ForkActivation::new));
    }
    p
}

fn bundle_of(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("testnet-12 is a ConsensusV2 network"),
    }
}

fn hybrid(n_ctx: u32) -> PalwShapeProfileV3 {
    qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx, ..QWEN36_35B_A3B })).expect("the held hybrid row")
}

fn dense(n_ctx: u32) -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B }).expect("the held dense row")
}

/// The acceptance path's gate for a class registration at `daa`, as the processor asks it (the shape from
/// `palw_admission_shape_at_v1`, the fences read at the block).
fn admit(
    params: &Params,
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    daa: u64,
) -> Result<u64, PalwClassAdmissionError> {
    let bundle = bundle_of(params);
    let shape = palw_admission_shape_at_v1(params, &bundle, profile, daa).map_err(PalwClassAdmissionError::Profile)?;
    let ladder_cap = match shape.ladder {
        Some(r) => r.ladder,
        None => kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(bundle.court.max_step_leaf_count(), profile),
    };
    let counted = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, canonical, ladder_cap).unwrap_or(u64::MAX);
    let reg = PalwConsensusObjectV2::ClassRegistered {
        class_id: profile.shape_profile_id(),
        artifact_root: Hash64::from_u64_word(0xDEADBEEF),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target: u128::MAX,
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    };
    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    verify_class_admission_v9(
        &bundle,
        profile,
        canonical,
        &reg,
        &certified,
        &[],
        shape.ladder,
        shape.court,
        false,
        shape.token_lift,
        shape.fused_dissectable,
        params.palw_canonical_work_at(daa),
        shape.held,
        shape.kimi_family,
        params.palw_audit_2026_09_23_active_at(daa),
        params.palw_gdn_key_heads_active_at(daa),
    )
    .map(|e| e.canonical_step_leaf_count)
}

fn canonical_of(profile: &PalwShapeProfileV3, held_canonical: (u32, u32)) -> PalwJobContextV2 {
    rc_job_context(profile, held_canonical.0, held_canonical.1)
}

/// **Where 5,102 comes from, and why testnet-12 never charges it.**
#[test]
fn the_5102_figure_is_the_ladder_clock_and_testnet12_charges_the_held_clock() {
    let p = t12_with_window(None);
    let bundle = bundle_of(&p);
    assert!(p.palw_held_context_active_at(0) && p.palw_held_context_active_at(DAA), "testnet-12 arms the held fence from genesis");
    let court = palw_court_params_held_at_v2(&bundle, true, true).expect("testnet-12's held court derives an arity");
    let (ladder, turn, terminal) = (court.bisection_rounds(), court.turn_deadline_daa(), court.terminal_rounds());
    let reserve = palw_close_assembly_daa_v1(court.max_close_chunks());
    println!(
        "testnet-12's court: ladder {ladder} rounds, turn {turn}, terminal {terminal}, arity {}, reserve {reserve}, window {}",
        court.dissection_arity(),
        bundle.state.window_court()
    );
    assert_eq!((ladder, turn, terminal, reserve, bundle.state.window_court()), (40, 42, 2, 1_616, 3_000));
    // THE LADDER CLOCK (`palw_attn_court_admits_row_v1`): every fused class is charged the leaf ladder's rounds; a history
    // of at most one tile (H = 0, root claim 1) takes 2·40 + 2 + 1 = 83 moves of 42 DAA, and the gate reports
    // `CourtWindowTooShort { needed: moves · deadline + reserve }`.
    let ladder_clock = palw_attn_court_admits_row_v1(&court, 16, 16, 3_000);
    let Err(PalwAttnCourtError::OverrunsWindow { moves, deadline, reserve: r, window_court }) = ladder_clock else {
        panic!("the ladder clock refuses a one-tile history under a 3,000-DAA window: {ladder_clock:?}")
    };
    assert_eq!((moves, deadline, r, window_court), (83, 42, 1_616, 3_000));
    assert_eq!(moves * deadline + r, 5_102, "the figure: 83 × 42 + 1,616");
    // …and under that clock EVERY fused class is refused at a 3,000-DAA window, whatever its history.
    for history in [16u64, 512, 32_768, 1 << 20] {
        assert!(
            palw_attn_court_admits_row_v1(&court, history, 16, 3_000).is_err(),
            "{history}: the ladder clock needs more than 3,000"
        );
    }
    // THE HELD CLOCK (`palw_attn_court_admits_row_held_v1`), the one testnet-12 charges: no leaf ladder; the same one-tile
    // history needs 3 moves, and every history up to 2^32 positions fits the 3,000-DAA window.
    for history in [16u64, 512, 32_768, 262_144, 1 << 20, 2_000_000, 1 << 32] {
        let worst = palw_attn_court_admits_row_held_v1(&court, history, 16, 3_000).unwrap_or_else(|e| panic!("{history}: {e:?}"));
        assert!(worst + reserve + 1 <= 3_000, "{history}: {worst} + {reserve} + 1");
        println!("held clock, {history:>10} positions: the exchange takes {} of 3,000 DAA", worst + reserve + 1);
    }
    // The window the fence would derive is therefore the network's, for every such history.
    for history in [16u64, 512, 262_144, 1 << 20, 1 << 32] {
        let w =
            kaspa_consensus_core::palw_class_admission_v2::palw_court_window_for_history_v1(3_000, true, true, &court, history, 16)
                .expect("finite");
        assert_eq!(w, 3_000, "{history}");
    }
}

/// **Legacy (graph) classes: the same shape, the same window, the same verdict, fence armed and unarmed.**
#[test]
fn legacy_classes_get_the_same_window_and_verdict_with_the_fence_armed_and_unarmed() {
    let unarmed = t12_with_window(None);
    let armed = t12_with_window(Some(3_600));
    assert!(!unarmed.palw_model_court_window_active_at(DAA) && armed.palw_model_court_window_active_at(DAA));
    let bundle = bundle_of(&unarmed);
    let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor");
    let floor_job = rc_job_context(&floor, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    // (label, profile, canonical job): the floor (no attention), the shipped hybrid and dense held rows, a Qwen3.5-9B-shaped
    // hybrid (the held hybrid row at its 262,144-position native context), a 2^32-position hybrid and a 2^32-position dense —
    // the last two are the class's `n_ctx` pushed to the field's end (`u32::MAX`); another check may refuse them first, and
    // the claim is that the verdict, whichever it is, is the same.
    let mut classes: Vec<(String, PalwShapeProfileV3, PalwJobContextV2)> = vec![("floor".into(), floor, floor_job)];
    for n_ctx in [512u32, 4_096, 32_768, 131_072, 262_144] {
        let profile = hybrid(n_ctx);
        let job = canonical_of(&profile, qwen36_held_canonical_v1(n_ctx));
        let label =
            if n_ctx == 262_144 { "Qwen3.5-9B-shaped hybrid, 262,144 positions".to_string() } else { format!("hybrid {n_ctx}") };
        classes.push((label, profile, job));
    }
    for n_ctx in [512u32, 32_768, 131_072, 2_097_152] {
        let profile = dense(n_ctx);
        let job = canonical_of(&profile, qwen25_a16_held_canonical_v1(n_ctx));
        classes.push((format!("dense {n_ctx}"), profile, job));
    }
    for (label, base, held_canonical) in [
        ("hybrid at 2^32 positions", hybrid(512), qwen36_held_canonical_v1(512)),
        ("dense at 2^32 positions", dense(2_097_152), qwen25_a16_held_canonical_v1(2_097_152)),
    ] {
        let mut profile = base;
        profile.n_ctx = u32::MAX;
        let job = canonical_of(&profile, held_canonical);
        classes.push((label.into(), profile, job));
    }
    for (label, profile, canonical) in &classes {
        let (su, sa) = (
            palw_admission_shape_at_v1(&unarmed, &bundle, profile, DAA).expect("the unarmed shape"),
            palw_admission_shape_at_v1(&armed, &bundle, profile, DAA).expect("the armed shape"),
        );
        // The WINDOW: the network's, with the fence armed and unarmed.
        let window = |s: &kaspa_consensus_core::palw_class_admission_v2::PalwAdmissionShapeV1| s.court.map(|c| c.window_court_daa);
        assert_eq!(window(&su), Some(3_000), "{label}: the unarmed window is the network's");
        assert_eq!(window(&sa), window(&su), "{label}: the armed window is the unarmed window");
        assert_eq!(format!("{su:?}"), format!("{sa:?}"), "{label}: the whole admission shape is the same");
        assert_eq!(
            palw_class_court_window_at_v1(&armed, &bundle, profile, DAA).expect("armed derivation"),
            palw_class_court_window_at_v1(&unarmed, &bundle, profile, DAA).expect("unarmed derivation"),
            "{label}: the derivation itself"
        );
        // The VERDICT of the acceptance path's gate.
        let (vu, va) = (admit(&unarmed, profile, canonical, DAA), admit(&armed, profile, canonical, DAA));
        assert_eq!(va, vu, "{label}: the same admission verdict");
        println!("{label:<44} n_ctx {:>10}: window {:?} both ways; verdict {vu:?}", profile.n_ctx, window(&su));
        // The window the model rule would commit, derived from the class's own shape at the court it plays.
        let court = palw_court_params_held_at_v2(&bundle, true, true).expect("court");
        let fused = kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1(profile);
        let w = palw_class_court_window_for_shape_v1(3_000, true, true, &court, profile).expect("finite");
        assert_eq!(w, 3_000, "{label}: the model window is the network's (fused: {fused})");
    }
    // The two classes the claim is about are in the set and admit or refuse the same way.
    assert!(classes.iter().any(|(l, ..)| l.contains("Qwen3.5-9B-shaped")) && classes.iter().any(|(l, ..)| l.contains("2^32")));
}

// ------------------------------------------------------------------------------------------------------------------
// IR classes: dissected cones at several contexts, through admission v10.
// ------------------------------------------------------------------------------------------------------------------

const AT: u64 = 1_000;

/// testnet-12 with `palw_tir_v1` armed at `AT` and the model court window armed at `window` (or not).
fn ir_params(window: Option<u64>) -> Params {
    let mut p = t12_with_window(window);
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    p.sync_palw_tir_v1();
    p
}

fn root_of(program: &TirProgramV1, params: &MapParams) -> Hash64 {
    let ops = palw_tir_inventory_operands_v1(program, &TensorSrc(params)).expect("the inventory");
    artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("a root")
}

fn ir_registration(bundle: &PalwConsensusParamsV2, program: &TirProgramV1, root: Hash64, context: u32) -> PalwConsensusObjectV2 {
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: layout(program, context),
        tokenizer_id: Hash64::from_bytes([0x70; 64]),
    };
    let class_id = class.class_id(&root);
    let facts = PalwTirJobFactsV1::of_class(&class, class_id).expect("decodes");
    let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&class).expect("wide enough"));
    palw_tir_post_genesis_registration_v1(
        class,
        canonical,
        root,
        0,
        1 << 100,
        1,
        AT + 10,
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
            transaction_id: kaspa_consensus_core::tx::TransactionId::from_bytes([7; 64]),
            index: 0,
        }),
        vec![9; 16],
        bundle.court.max_step_leaf_count(),
    )
    .expect("the builder counts the canonical job")
}

#[test]
fn ir_classes_get_the_same_verdict_with_the_fence_armed_and_unarmed() {
    let (unarmed, armed) = (ir_params(None), ir_params(Some(AT)));
    assert!(!unarmed.palw_model_court_window_active_at(AT) && armed.palw_model_court_window_active_at(AT));
    let bundle = bundle_of(&unarmed);
    let (ru, ra) = (
        PalwTirAdmissionRulesV1::at(&unarmed, AT).expect("the IR fence"),
        PalwTirAdmissionRulesV1::at(&armed, AT).expect("the IR fence"),
    );
    assert!(!ru.model_court_window_active && ra.model_court_window_active, "the only difference between the two rule sets");
    assert_eq!(ru.court, ra.court, "…and the court they resolve is the same (the network window)");
    let (_, mut program, params, _) = programs().into_iter().find(|(n, ..)| n == "dense-gqa-2layer").expect("the dense model");
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let root = root_of(&program, &params);
    let mut judged = 0;
    // A dense GQA model (its attention output reduces over the history: a dissected cone) at contexts up to the
    // field's end; a context the registration builder cannot count is reported and skipped.
    for context in [64u32, 4_096, 32_768, 1_048_576, u32::MAX] {
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ir_registration(&bundle, &program, root, context)));
        let Ok(object) = built else {
            println!("IR context {context:>10}: the registration builder does not take it (not judged)");
            continue;
        };
        let admit = |rules: &PalwTirAdmissionRulesV1| {
            verify_class_admission_v10(&bundle, rules, &object, &[], &[])
                .map(|(_, record)| format!("{record:?}"))
                .map_err(|e: PalwClassAdmissionError| e)
        };
        let (vu, va) = (admit(&ru), admit(&ra));
        assert_eq!(va, vu, "IR context {context}: the same admission verdict");
        println!(
            "IR context {context:>10}: window {} both ways; verdict {}",
            ru.court.map(|c| c.window_court_daa).unwrap_or(0),
            match &vu {
                Ok(_) => "admitted".to_string(),
                Err(e) => format!("{e}"),
            }
        );
        judged += 1;
    }
    assert!(judged >= 3, "at least three IR contexts were judged ({judged})");
}

/// **The measured model reads the held regime testnet-12 charges** (`palw-class measure`, `palw-shard-plan`): a Qwen3.5-9B-shaped hybrid at
/// 8,192 positions fits the court under the network's own reading (`shape.held`: `palw_held_context` armed from genesis → the held clock,
/// at most 2,919 of the 3,000-DAA window), where the old reading (`Shipped`, the ladder clock's 5,102) refused it — the figure a registrant
/// was shown by mistake.
#[test]
fn the_measured_model_row_of_a_9b_shaped_hybrid_at_8k_fits_testnet12_under_the_held_regime() {
    use kaspa_consensus_core::palw_class_admission_v2::PalwHeldAdmissionV1;
    use kaspa_consensus_core::palw_measured_model_v1::{PalwMeasureInputsV1, PalwModelManifestV1, palw_measure_model_v1};
    let params = t12_with_window(None);
    let bundle = bundle_of(&params);
    let fingerprint = format!("{}", params.consensus_params_id());
    let geometry = PalwQwen36GeometryV1 { n_ctx: 8_192, ..QWEN36_35B_A3B };
    let manifest = PalwModelManifestV1::from_hybrid("Qwen3.5-9B-shaped hybrid", &geometry, None);
    let inputs = PalwMeasureInputsV1 {
        ruleset: "testnet-12",
        ruleset_fingerprint_hex: &fingerprint,
        bundle: &bundle,
        contexts: &[8_192],
        seat_budgets: &[24 << 30],
        max_shards: 64,
        held: None,
    };
    let network = |profile: &PalwShapeProfileV3| {
        let shape = palw_admission_shape_at_v1(&params, &bundle, profile, DAA).expect("an admission shape");
        assert!(shape.held.armed, "testnet-12 arms the held fence");
        (shape.court, params.palw_prompt_ids_form_at(DAA), shape.held)
    };
    let doc = palw_measure_model_v1(&manifest, inputs, &network, None, "the test");
    let row = &doc.deterministic.rows[0];
    assert!(row.fit_admitted, "the held regime fits the 9B-shaped hybrid at 8k: refused by {:?}", row.refusing_walls);
    // The reading that charged the ladder clock (a network without the fence): the same profile is refused by a court wall.
    let shipped = |profile: &PalwShapeProfileV3| {
        let shape = palw_admission_shape_at_v1(&params, &bundle, profile, DAA).expect("an admission shape");
        (shape.court, params.palw_prompt_ids_form_at(DAA), PalwHeldAdmissionV1::default())
    };
    let old = palw_measure_model_v1(&manifest, inputs, &shipped, None, "the test");
    assert!(!old.deterministic.rows[0].fit_admitted, "the ladder clock refuses it (the 5,102 figure)");
}
