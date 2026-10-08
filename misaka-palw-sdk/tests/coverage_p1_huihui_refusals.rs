//! **COV-P1P2 — the four past refusals of `huihui-ai/Huihui-Qwen3.5-9B-abliterated` @`05b9e7c9`, pinned** (record
//! `docs/design/palw/coverage-p1p2-record.md` §2). Each test asserts the EXACT structured refusal (`PalwRefusalV1` and its error) and
//! the commit point where it applies, and what the rules in force say instead — so a regression to the old reading fails here.
//!
//! The IR-route fixtures lower the 9B's TEXT decoder shape-only from its real `config.json` and safetensors HEADERS
//! (`misaka-palw-tir-lower/tests/fixtures/vlm-generic/huihui-qwen3.5-9b`, no weight byte). The legacy-route fixtures build the
//! `qwen36` lineage profile of the same geometry (read off that `config.json`). Neither is a registration: the artifact root is a
//! fixed placeholder (registration never reads weights; the class id is the only thing the root enters).
//!
//! | # | Past refusal | Where it stands now |
//! |---|---|---|
//! | 1 | "Court 5,102 DAA > 3,000" (manifest verifier, `verify_class_admission_v6(…, false)`, the LADDER clock) | the shared probe reads the held clock; the old call is gone from both readers |
//! | 2 | `TIR_EXCEEDS_CEILING` 67,108,865 vs 67,108,864 (IR close sizing) | `cap + 1` is the ELEMENT twin's sentinel below `palw_tir_fence2` (the height the offline gate used to judge at); the RANGE twin in force admits the same class in 41,490,156 steps |
//! | 3 | `COURT_COST_EXCEEDS_CEILING` 16,842,752 vs 16,777,216 tile MACs | the default logits tile (4,096 lanes) at the logits commit point; a narrower logits tile clears it |
//! | 4 | `HELD_CLASS_UNANSWERABLE` (ADR-0152 §4-ter C5) | the legacy held route: 24 Gated DeltaNet layers, no windowed builder for the hybrid held site |
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_class_admission_v2::{
    PalwAdmissionProbeRefusalV1, PalwClassAdmissionError, PalwHeldUnanswerableV1, palw_admission_probe_v1, palw_admission_shape_at_v1,
    palw_post_genesis_registration_capped_v1, verify_class_admission_v6,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_tir_attempt_v1::{
    PalwTirJobFactsV1, palw_tir_attempt_canonical_of_v1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1,
};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
use kaspa_hashes::Hash64;
use misaka_palw_sdk::preflight::{Options, model, source};
use misaka_palw_sdk::tir_layout::{TirLayoutChoiceV1, tir_layout_tiles_v1, tir_program_with_scheme_v1};
use misaka_palw_tir::program::TirProgramV1;
use std::path::PathBuf;

const ROOT: Hash64 = Hash64::from_bytes([0x9B; 64]);
const TOKENIZER: Hash64 = Hash64::from_bytes([0x5A; 64]);
const CAP: u64 = 1 << 26;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/vlm-generic/huihui-qwen3.5-9b")
}

fn config() -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(fixture().join("config.json")).unwrap()).unwrap()
}

fn t12() -> (Params, PalwConsensusParamsV2) {
    let params = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 is a V2 network") };
    let bundle = bundle.clone();
    (params, bundle)
}

/// The 9B text decoder, lowered shape-only from the headers at `ctx` positions, under the tiled logits scheme.
fn program(ctx: u32) -> TirProgramV1 {
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let dir = fixture();
    let src = source::open(&dir, source::detect(&dir).expect("a HF directory"), None, reg).expect("the headers");
    let analysis = model::analyze(&src, &Options { max_context: Some(ctx), ..Options::default() }, reg, None);
    let program =
        analysis.program.unwrap_or_else(|| panic!("no program: {:?}", analysis.blockers.iter().map(|b| &b.code).collect::<Vec<_>>()));
    tir_program_with_scheme_v1(&program, None).expect("the tiled scheme")
}

fn class(program: &TirProgramV1, layout: PalwTirLayoutV1) -> PalwTirClassV1 {
    PalwTirClassV1 { version: PALW_TIR_CLASS_VERSION_V1, program: program.encode(), layout, tokenizer_id: TOKENIZER }
}

/// A layout: tile 64, logits tile `logits`, history tile `h_tile`, checkpoint interval `interval`.
fn layout(params: &Params, program: &TirProgramV1, ctx: u32, logits: u32, h_tile: u32, interval: u32) -> PalwTirLayoutV1 {
    let choice = TirLayoutChoiceV1 { max_context: Some(ctx), logits_tile: Some(logits), h_chunk: h_tile, ..Default::default() };
    let mut l = tir_layout_tiles_v1(params, program, &choice).expect("tiles");
    l.checkpoint_interval = interval;
    l
}

/// The registration gate the acceptance path runs, at `daa` (weightless, the formula's canonical job).
fn gate(params: &Params, bundle: &PalwConsensusParamsV2, class: &PalwTirClassV1, daa: u64) -> Result<(), PalwClassAdmissionError> {
    use kaspa_consensus_core::palw_tir_admission_v1::{palw_tir_post_genesis_registration_v1, palw_tir_registration_preflight_at_v1};
    let program = class.decode_program().expect("decodes");
    let canonical = palw_tir_attempt_canonical_v1(class).expect("a canonical job");
    let facts = PalwTirJobFactsV1::of(class, &program, class.class_id(&ROOT));
    let bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        kaspa_consensus_core::tx::TransactionId::from_bytes([0; 64]),
        0,
    ));
    let object = palw_tir_post_genesis_registration_v1(
        class.clone(),
        palw_tir_job_context_v1(&facts, canonical),
        ROOT,
        0,
        u128::MAX,
        1,
        0,
        bond,
        Vec::new(),
        bundle.court.max_step_leaf_count(),
    )?;
    palw_tir_registration_preflight_at_v1(params, bundle, &object, daa, &[]).map(|_| ())
}

fn decided(params: &Params, daa: u64) -> String {
    kaspa_consensus_core::palw_refusal_v1::palw_refusal_decided_by_v1(
        params.palw_held_context_active_at(daa),
        Some(daa),
        params.palw_held_context.map(|f| f.daa_score()),
    )
}

// ───────────────────────────────────────────── refusal 3 ─────────────────────────────────────────────

/// **Refusal 3: `COURT_COST_EXCEEDS_CEILING`, 16,842,752 tile MACs against 16,777,216, at the DEFAULT layout** (tile 64, logits tile
/// 4,096 lanes) — the logits commit point: a 4,096-lane tile of a 4,096-wide head is 2^24 MACs before anything else in its cone.
/// The same class at a 2,048-lane logits tile passes that check (the gate names the next one), and at 256 lanes it is admitted.
#[test]
fn refusal_3_the_default_logits_tile_exceeds_the_terminal_macs_and_a_narrower_one_does_not() {
    let (params, bundle) = t12();
    let p = program(8_192);
    let default = {
        let mut l = tir_layout_tiles_v1(&params, &p, &TirLayoutChoiceV1 { max_context: Some(8_192), ..Default::default() }).unwrap();
        l.checkpoint_interval = 298;
        l
    };
    assert_eq!(default.commit_tiles.last(), Some(&4_096), "the logits commit point is the last commit tile, 4,096 lanes by default");
    for daa in [5_585u64, 9_000] {
        let err = gate(&params, &bundle, &class(&p, default.clone()), daa).expect_err("the default tile is refused");
        assert_eq!(
            err,
            PalwClassAdmissionError::CourtCostExceedsCeiling {
                what: "IR tile multiply-accumulates",
                got: 16_842_752,
                ceiling: 16_777_216
            }
        );
        let r = err.refusal_v1(&decided(&params, daa));
        assert_eq!((r.code.as_str(), r.needed, r.limit), ("COURT_COST_EXCEEDS_CEILING", Some(16_842_752), Some(16_777_216)));
        assert_eq!(16_842_752u64 - (1 << 24), 65_536, "2^24 = 4,096 lanes × a 4,096-wide head, plus the cone's 65,536");
    }
    let narrower = gate(&params, &bundle, &class(&p, layout(&params, &p, 8_192, 2_048, 64, 298)), 5_585);
    assert!(
        !matches!(narrower, Err(PalwClassAdmissionError::CourtCostExceedsCeiling { what: "IR tile multiply-accumulates", .. })),
        "a 2,048-lane logits tile is within the terminal MACs: {narrower:?}"
    );
}

// ───────────────────────────────────────────── refusal 2 ─────────────────────────────────────────────

/// **Refusal 2: `TIR_EXCEEDS_CEILING`, IR close-sizing work 67,108,865 against 67,108,864.** `cap + 1` is a SENTINEL
/// (`palw_tir_carried_closes_admit_form_v1` writes `work_cap + 1` for any sizing that stops at the cap), not a measurement. It is the
/// ELEMENT twin's, which sizes closes below `palw_tir_fence2` — the height `TirOfflineGateV1` judged at (the IR fence's own, DAA
/// 2,000) until COV-P1P2, so `declare-layout` refused every 9B layout. The rules in force since DAA 3,600 size with the RANGE twin,
/// which admits the same class: 41,490,156 steps (0.618 × the cap) at 8,192 positions, the costliest commit point `(1, 298)` at
/// 24,238,464. The offline gate now judges where every scheduled fence is in force, and admits it.
#[test]
fn refusal_2_the_cap_plus_one_sentinel_is_the_element_twin_below_fence2_and_the_range_twin_admits() {
    use kaspa_consensus_core::palw_tir_close_size_v1 as z;
    let (params, bundle) = t12();
    let p = program(8_192);
    let c = class(&p, layout(&params, &p, 8_192, 256, 32, 298));
    // Below palw_tir_fence2 (DAA 2,000: the IR fence's own height): the element twin stops at the cap.
    assert!(!params.palw_tir_fence2_active_at(2_000) && params.palw_tir_fence2_active_at(3_600));
    let err = gate(&params, &bundle, &c, 2_000).expect_err("the element twin passes its cap");
    assert_eq!(
        err,
        PalwClassAdmissionError::TirExceeds {
            limit: "IR close sizing work",
            at: "the class's terminal closes".into(),
            value: CAP + 1,
            cap: CAP
        }
    );
    let r = err.refusal_v1(&decided(&params, 2_000));
    assert_eq!((r.code.as_str(), r.needed, r.limit), ("TIR_EXCEEDS_CEILING", Some(67_108_865), Some(67_108_864)));
    // In force (int-12, DAA 5,585; int-13, DAA 9,000): admitted.
    for daa in [3_600u64, 5_585, 9_000] {
        gate(&params, &bundle, &c, daa).unwrap_or_else(|e| panic!("DAA {daa}: {} ({e})", e.code()));
    }
    // The measurement behind the verdicts: the range twin's true work, per commit point.
    let space = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(&c).unwrap();
    let inventory = kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1::new(&p).unwrap();
    let facts = PalwTirJobFactsV1::of(&c, &p, Hash64::default());
    let deepest = kaspa_consensus_core::palw_v2::PalwJobContextV2 {
        declared_prefill_tokens: 1,
        exact_decode_tokens: 8_192,
        max_context_tokens: u32::MAX,
        ..palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_of_v1(8_192).unwrap())
    };
    let sizing = z::PalwTirCloseSizingV1 { form: z::PalwTirParamFormV1::Multiproof, court: true, cap: u64::MAX / 4, stop_above: None };
    let mut trace = Vec::new();
    let (bounds, work) = kaspa_consensus_core::palw_tir_close_range_v1::palw_tir_worst_closes_range_trace_v1(
        &space, &inventory, &deepest, &sizing, &mut trace,
    )
    .expect("the range twin sizes the class");
    assert_eq!(work, 41_490_156, "the range twin's work at 8,192 positions");
    assert_eq!(bounds.len(), 43);
    let per: Vec<(u8, u16, u64)> = trace
        .iter()
        .scan(0u64, |before, (b, n, after)| {
            let w = after - *before;
            *before = *after;
            Some((*b, *n, w))
        })
        .collect();
    assert_eq!(per.iter().max_by_key(|x| x.2), Some(&(1, 298, 24_238_464)), "the costliest commit point");
    // The offline gate (declare-layout, palw-class preflight, the interval search) judges where fence2 is in force, and admits it.
    let offline = misaka_palw_sdk::tir_layout::TirOfflineGateV1::of(&params);
    assert!(offline.params.palw_tir_fence2_active_at(offline.daa), "the offline gate judges at DAA {}", offline.daa);
    misaka_palw_sdk::tir_layout::tir_class_admission_offline_v1(&params, &bundle, &c, ROOT)
        .expect("the offline gate admits the class");
}

// ───────────────────────────────────────── refusals 1 and 4 (legacy route) ─────────────────────────────────────────

/// The `qwen36` lineage geometry of the 9B, read off its `config.json` (dense FFN as one always-chosen expert, as the lineage writes
/// `QWEN35_2B` and `QWEN38_27B`), at the historical `n_ctx` 16.
fn legacy_9b() -> kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
    use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN38_27B, qwen36_geometry_artifact_eps};
    let t = &config()["text_config"];
    let u = |k: &str| t[k].as_u64().unwrap_or_else(|| panic!("{k}"));
    let head_dim = u("head_dim") as u32;
    let rope = (head_dim as f64 * t["rope_parameters"]["partial_rotary_factor"].as_f64().unwrap()) as u16;
    assert_eq!(t["rope_parameters"]["rope_theta"].as_u64(), Some(10_000_000), "the family base the lineage's 1e7 bits encode");
    qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 {
        layer_count: u("num_hidden_layers") as u16,
        full_attention_interval: u("full_attention_interval") as u16,
        hidden_dim: u("hidden_size") as u32,
        attn_heads: u("num_attention_heads") as u16,
        attn_kv_heads: u("num_key_value_heads") as u16,
        attn_head_dim: head_dim,
        rope_dims: rope,
        gdn_k_heads: u("linear_num_key_heads") as u16,
        gdn_v_heads: u("linear_num_value_heads") as u16,
        gdn_head_dim: u("linear_key_head_dim") as u32,
        gdn_conv_kernel: u("linear_conv_kernel_dim") as u16,
        n_experts: 1,
        experts_per_token: 1,
        moe_dim: u("intermediate_size") as u32,
        shared_dim: 0,
        attn_output_gate: u8::from(t["attn_output_gate"].as_bool() == Some(true)),
        vocab_size: u("vocab_size") as u32,
        n_ctx: 16,
        ..QWEN38_27B
    })
}

/// **Refusal 1: "Court 5,102 DAA > t12 limit 3,000".** The manifest verifier called `verify_class_admission_v6(…, false)` with the
/// shape's court and ladder and NO held regime, so it charged the LADDER clock: `(2·(L + H) + t + 1) · turn + reserve` =
/// 83 × 42 + 1,616 = 5,102. testnet-12 arms `palw_held_context` from genesis, so its gate reads the HELD clock (no leaf ladder; ≤ 2,919
/// up to 2^32 positions at tile 16) and admits the class's court; the wall is elsewhere (refusal 4). Fixed by the shared probe
/// `palw_admission_probe_v1` (R2, `b8044f32c`); this pins the exact old number on the 9B's own geometry and that neither reader calls
/// the old gate again.
#[test]
fn refusal_1_the_ladder_clock_charged_5102_daa_and_the_held_probe_does_not() {
    use kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v5;
    let (params, bundle) = t12();
    // The fused-attention row (graph-v5): the court that charges the leaf ladder's 40 rounds. (The graph-v1/v2 rows read 5,060 by the
    // same clock; the later rows are refused by the old call for their own fences — the held map, the token lift — before the window.)
    let profile = qwen36_profile_v5(legacy_9b()).expect("the lineage profile");
    let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, 1, 2);
    let daa = 3_752;
    let shape = palw_admission_shape_at_v1(&params, &bundle, &profile, daa).expect("a shape");
    assert!(shape.held.armed, "testnet-12 reads the held clock");
    // The old call, verbatim (model_class.rs before b8044f32c).
    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    let probe = palw_post_genesis_registration_capped_v1(
        profile.clone(),
        canonical.clone(),
        ROOT,
        0,
        1,
        1,
        0,
        kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
            kaspa_consensus_core::tx::TransactionId::default(),
            0,
        )),
        Vec::new(),
        bundle.court.max_step_leaf_count(),
    )
    .expect("the registration object");
    let old = verify_class_admission_v6(&bundle, &profile, &canonical, &probe, &certified, &[], shape.ladder, shape.court, false)
        .expect_err("the ladder clock overruns the window");
    assert_eq!(old, PalwClassAdmissionError::CourtWindowTooShort { needed: 5_102, window: 3_000 });
    let r = old.refusal_v1(&kaspa_consensus_core::palw_refusal_v1::palw_refusal_decided_by_v1(false, Some(daa), None));
    assert_eq!(
        (r.code.as_str(), r.needed, r.limit, r.unit.as_deref()),
        ("COURT_WINDOW_TOO_SHORT", Some(5_102), Some(3_000), Some("DAA"))
    );
    // The shared probe, at the same shape: the held clock; no court-window refusal.
    match palw_admission_probe_v1(&bundle, &profile, &canonical, ROOT, &[], &shape) {
        Ok(_) => {}
        Err(PalwAdmissionProbeRefusalV1::Gate(e) | PalwAdmissionProbeRefusalV1::Express(e)) => {
            panic!("the held probe refuses: {} ({e})", e.code())
        }
        Err(PalwAdmissionProbeRefusalV1::Price(m)) => panic!("price: {m}"),
    }
    // The old path cannot come back: neither reader calls the gate directly.
    for (name, src) in [
        ("misaka-palw-extension/src/kinds/model_class.rs", include_str!("../../misaka-palw-extension/src/kinds/model_class.rs")),
        ("misaka-palw-sdk/src/sdk.rs", include_str!("../src/sdk.rs")),
    ] {
        let code: String = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        assert!(!code.contains("verify_class_admission_v6("), "{name} calls verify_class_admission_v6 directly again");
        assert!(code.contains("palw_admission_probe_v1("), "{name} no longer asks the shared probe");
    }
}

/// **Refusal 4: `HELD_CLASS_UNANSWERABLE` (ADR-0152 §4-ter C5)** — the processor's attribution check beside the gate
/// (`palw_registration_attribution_v1`, the same two calls the `ClassRegistered` arm makes): the 9B's legacy held row has 24 Gated
/// DeltaNet layers (of 32; the other 8 are full attention), and no family has a windowed builder that answers a hybrid held site.
/// Below the P0a flag day (DAA 5,300) the gate admits the version-2 profile and C5 refuses it; past it the version-2 profile meets
/// `GDN_MAP_ASSUMES_EQUAL_HEADS` (16 key heads over 32 value heads) first, and the version-3 profile (explicit key heads) passes the gate
/// and meets C5 again. The general fix is a held-site builder for recurrent layers (or the IR route, which registers the same text
/// decoder at 8,192 positions — refusal 2's test).
#[test]
fn refusal_4_the_legacy_held_route_has_no_builder_for_24_recurrent_layers() {
    use kaspa_consensus_core::palw_qwen36_profile::{qwen36_profile_v7, qwen36_profile_v8};
    let (params, bundle) = t12();
    let flag_day = kaspa_consensus_core::config::params::PALW_T12_INT11_FLAG_DAY_DAA.expect("the int-11 flag day");
    let c5 = |profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3, daa: u64| {
        let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, 1, 2);
        kaspa_consensus_core::palw_attempt_rules_v1::palw_registration_attribution_v1(
            profile,
            &canonical,
            params.palw_offence_attribution_active_at(daa),
            params.palw_prompt_ids_form_at(daa),
        )
    };
    let probe = |profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3, daa: u64| {
        let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, 1, 2);
        let shape = palw_admission_shape_at_v1(&params, &bundle, profile, daa).expect("a shape");
        palw_admission_probe_v1(&bundle, profile, &canonical, ROOT, &[], &shape).map(|_| ())
    };
    let held = |e: kaspa_consensus_core::palw_attempt_rules_v1::PalwRegistrationAttributionErrorV1| match e {
        kaspa_consensus_core::palw_attempt_rules_v1::PalwRegistrationAttributionErrorV1::Held(e) => e,
        other => panic!("not C5: {other}"),
    };
    let v2 = qwen36_profile_v7(legacy_9b()).expect("version 2");
    let v3 = qwen36_profile_v8(legacy_9b()).expect("version 3");
    let below = 3_752;
    assert!(probe(&v2, below).is_ok(), "below the flag day the gate admits the version-2 row");
    let e = held(c5(&v2, below).expect_err("C5 refuses it"));
    assert_eq!(e, PalwClassAdmissionError::HeldClassUnanswerable { why: PalwHeldUnanswerableV1::Recurrent { layers: 24 } });
    assert_eq!(e.refusal_v1(&decided(&params, below)).code, "HELD_CLASS_UNANSWERABLE");
    for daa in [flag_day, 5_585, 9_000] {
        match probe(&v2, daa) {
            Err(PalwAdmissionProbeRefusalV1::Gate(e)) => {
                assert_eq!(e, PalwClassAdmissionError::GdnMapAssumesEqualHeads { key_heads: 16, value_heads: 32 }, "DAA {daa}")
            }
            other => panic!("DAA {daa}: {:?}", other.map_err(|e| format!("{e:?}"))),
        }
        assert!(probe(&v3, daa).is_ok(), "DAA {daa}: the version-3 row passes the gate");
        let e = held(c5(&v3, daa).expect_err("C5 refuses it"));
        assert_eq!(
            e,
            PalwClassAdmissionError::HeldClassUnanswerable { why: PalwHeldUnanswerableV1::Recurrent { layers: 24 } },
            "DAA {daa}"
        );
    }
}

// ───────────────────────────── every registration-time rule, at today's height ─────────────────────────────

/// **The 9B text decoder at 8,192 positions against every registration-time rule of testnet-12 at DAA 7,000** (the fleet's height
/// on 2026-10-08; `palw_held_context` armed from genesis), in the processor's order (`processor.rs`, the `ClassRegisteredTirV1` arm
/// of the acceptance walk, then the fold). The stateful rules (target, signature, bond, slash value, activation window, exposure,
/// duplicate) are the registrant's object's to satisfy and the lane-D E2E carries this class through them
/// (`fixtures/g14/shipped/huihui-qwen3.5-9b-8k.json`); this test pins the class-dependent ones with their numbers.
#[test]
fn the_9b_8k_text_class_meets_every_class_rule_at_daa_7000() {
    use kaspa_consensus_core::palw_tir_admission_v1::{PalwTirAdmissionRulesV1, palw_tir_carriable_close_bytes_v1};
    let (params, bundle) = t12();
    let daa = 7_000;
    let p = program(8_192);
    let c = class(&p, layout(&params, &p, 8_192, 256, 32, 298));
    // 0. The IR fence is in force; no other fence moves between 5,585 and 9,000.
    assert!(params.palw_tir_v1_fence().is_some_and(|f| f.activation.is_active(daa)));
    assert!(params.fence_schedule_v1().iter().all(|h| *h <= 5_585 || *h >= 9_000));
    // 1. Program bytes, decoded strictly.
    assert_eq!(c.program.len(), 32_640);
    let rules = PalwTirAdmissionRulesV1::at(&params, daa).expect("in force");
    assert!(c.program.len() as u32 <= rules.fence.ceilings.max_program_bytes);
    // 2. Not the held history bound: the GDN layers are `Fixed` states replayed from checkpoints (C = 298), never a held site —
    //    ADR-0152 §4-ter C5 is the LEGACY arm's (`ClassRegistered`); the IR arm does not ask it.
    assert_eq!(p.history_bound, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
    assert_eq!(p.states.iter().filter(|s| matches!(s.kind, misaka_palw_tir::program::StateKind::Fixed { .. })).count(), 2);
    let processor = include_str!("../../consensus/src/pipeline/virtual_processor/processor.rs");
    let arm =
        processor.split("Obj::ClassRegisteredTirV1 { class_id, share_permille, admission, .. } => {").nth(1).expect("the IR arm");
    let arm = &arm[..arm.find("Obj::FamilyCertified").expect("the next arm")];
    assert!(!arm.contains("palw_held_class_is_attributable_v1"), "the IR arm asks C5 now: re-judge this class");
    assert!(arm.contains("verify_class_admission_v10("));
    // 3. Context within the program's bound and the network's (2^18).
    assert!(8_192 <= rules.fence.ceilings.max_context);
    // 5. The court window: the held clock, no model window at this height — the network's 3,000.
    let court = rules.court.expect("the k-ary court is armed");
    assert!(rules.held.armed && !rules.model_court_window_active);
    assert_eq!(court.window_court_daa, 3_000);
    // 6. J5b: the canonical prompt (1,023 ids) is within the inline bound (4,096).
    assert_eq!(palw_tir_attempt_canonical_v1(&c), Some((1_023, 2)));
    assert!(1_023 <= kaspa_consensus_core::palw_attempt_rules_v1::PALW_J5_INLINE_PROMPT_IDS_V1);
    // 1–9. Admission v10 at this height admits it.
    gate(&params, &bundle, &c, daa).unwrap_or_else(|e| panic!("{} ({e})", e.code()));
    // 9. The carried closes: within the 3,200,000 bytes a close is carried in, the root claims within one carrier.
    assert_eq!(palw_tir_carriable_close_bytes_v1(&bundle.court), 3_200_000);
    // Share rule: no certified family covers the IR primitives on testnet-12, so the class registers at 0‰ (weightless, Dormant
    // until certified) — required 0, which the object carries.
    let reachable = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_reachable_prims_v1(&p);
    let covered = kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_weight_v2(
        bundle.court_e2e_root,
        &kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1(),
        &[],
        &reachable,
    )
    .expect("priced");
    assert!(covered.is_none(), "a certified family covers the IR primitives: the share rule now requires a nonzero share");
    // Tooling, not consensus: the calibration-length rule (declare-layout and the runtime pack refuse a recurrent artifact that does
    // not record a calibration as long as the context — the 9B must be calibrated on one sequence of at least 8,192 tokens).
    let meta = |m: serde_json::Value| m;
    assert!(misaka_palw_sdk::tir_layout::tir_calibration_covers_context_v1(&p, &meta(serde_json::json!({})), 8_192).is_err());
    assert!(
        misaka_palw_sdk::tir_layout::tir_calibration_covers_context_v1(
            &p,
            &meta(serde_json::json!({ "calibrated_context": 4_096 })),
            8_192
        )
        .is_err()
    );
    misaka_palw_sdk::tir_layout::tir_calibration_covers_context_v1(
        &p,
        &meta(serde_json::json!({ "calibrated_context": 8_192 })),
        8_192,
    )
    .expect("an 8,192-token calibration covers it");
}
