//! **Every reader of the court window agrees for a Qwen3.5-9B-shaped hybrid on testnet-12** (the 2026-10-03 refusal of Huihui-Qwen3.5-9B:
//! "adjudication needs 5,102 DAA, window 3,000" — the ladder clock, which testnet-12 never charges because `palw_held_context` is armed
//! from genesis). The class: the Qwen3.5 hybrid profile at `n_ctx` 16 and 8,192, canonical job (1, 2), asked of testnet-12 at DAA 0,
//! 3,599, 3,600, 3,752, 5,299 and 5,300 by
//!
//! * the processor's acceptance gate (`verify_class_admission_v9` under `palw_admission_shape_at_v1`'s court, ladder and held reading),
//! * the node's preflight (`palw_model_preflight_v1`, what `getPalwModelPreflight` runs),
//! * the fit report under `palw_fit_regime_for_v1` (`palw_model_fit_v2`),
//! * the measured model (`palw_measure_model_v1` — `palw-class measure`, `palw-shard-plan`),
//! * the SDK's pre-signing gate (`PalwClassSdk::preflight_admission`, what `misaka model add|inspect|preflight` build through).
//!
//! All must admit, or refuse with the same code and the same numbers. The reader of the ladder clock (`PalwFitRegimeV1::Shipped`,
//! the measured model before this change) is shown to differ, so the test would catch its return.

use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_base0_profile::rc_job_context;
use kaspa_consensus_core::palw_class_admission_v2::{
    PalwClassAdmissionError, PalwHeldAdmissionV1, palw_admission_shape_at_v1, palw_post_genesis_registration_capped_v1, verify_class_admission_v9,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_measured_model_v1::{PalwMeasureInputsV1, PalwModelManifestV1, palw_measure_model_v1};
use kaspa_consensus_core::palw_model_fit_v1::{palw_fit_regime_for_v1, palw_model_fit_v2};
use kaspa_consensus_core::palw_model_registration_v1::palw_model_preflight_v1;
use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_profile_v7};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::Hash64;
use misaka_palw_sdk::{PalwClassEntryV1, PalwClassSdk};

const DAAS: [u64; 6] = [0, 3_599, 3_600, 3_752, 5_299, 5_300];

fn profile(n_ctx: u32) -> PalwShapeProfileV3 {
    qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx, ..QWEN36_35B_A3B })).expect("the hybrid row")
}

/// What a reader says: `Ok(())` or `(code, needed, limit)`.
type Verdict = Result<(), (String, Option<u64>, Option<u64>)>;

fn refusal(e: &PalwClassAdmissionError) -> (String, Option<u64>, Option<u64>) {
    match e {
        PalwClassAdmissionError::CourtWindowTooShort { needed, window } => (e.code().to_string(), Some(*needed), Some(*window)),
        other => (other.code().to_string(), None, None),
    }
}

#[test]
fn every_reader_of_the_court_agrees_for_a_9b_shaped_hybrid_on_testnet12() {
    let params = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 ships a bundle") };
    let fingerprint = format!("{}", params.consensus_params_id());
    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    let sdk = PalwClassSdk::builtin_v1(bundle.court, params.palw_prompt_ids_form_v1(), params.net.to_string().into_bytes());
    let dummy = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::default(), 0));
    let root = Hash64::from_u64_word(0x9B);
    let mut disagreements = Vec::new();
    for n_ctx in [16u32, 8_192] {
        let profile = profile(n_ctx);
        let canonical = rc_job_context(&profile, 1, 2);
        let object = palw_post_genesis_registration_capped_v1(
            profile.clone(),
            canonical.clone(),
            root,
            0,
            1,
            1,
            0,
            dummy,
            Vec::new(),
            bundle.court.max_step_leaf_count(),
        )
        .expect("the object expresses");
        let entry = PalwClassEntryV1 {
            model_id: "Huihui-Qwen3.5-9B",
            lineage_id: "test",
            profile: profile.clone(),
            canonical_job: (1, 2),
            needs_artifact_file: true,
        };
        let manifest = PalwModelManifestV1::from_hybrid("Qwen3.5-9B-shaped", &PalwQwen36GeometryV1 { n_ctx, ..QWEN36_35B_A3B }, None);
        for daa in DAAS {
            let shape = palw_admission_shape_at_v1(&params, bundle, &profile, daa).expect("a shape");
            // 1. the processor's gate
            let gate: Verdict = verify_class_admission_v9(
                bundle,
                &profile,
                &canonical,
                &object_registration(&object),
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
            .map(|_| ())
            .map_err(|e| refusal(&e));
            // 2. the node's preflight: admissible or the same reject code
            let node = palw_model_preflight_v1(&params, bundle, &object, &certified, &[], daa, false, None).expect("a report");
            let node_v: Verdict = if node.admissible { Ok(()) } else { Err((node.reject_code.clone(), None, None)) };
            // 3. the fit report under the network's regime
            let fit = palw_model_fit_v2(
                &profile,
                bundle,
                shape.court,
                params.palw_prompt_ids_form_at(daa),
                palw_fit_regime_for_v1(shape.held, &profile),
            );
            // 4. the measured model, under the network's reading
            let inputs = PalwMeasureInputsV1 {
                ruleset: "testnet-12",
                ruleset_fingerprint_hex: &fingerprint,
                bundle,
                contexts: &[n_ctx],
                seat_budgets: &[24 << 30],
                max_shards: 64,
                held: None,
            };
            let court_for = |p: &PalwShapeProfileV3| {
                let s = palw_admission_shape_at_v1(&params, bundle, p, daa).expect("a shape");
                (s.court, params.palw_prompt_ids_form_at(daa), s.held)
            };
            let measured = palw_measure_model_v1(&manifest, inputs, &court_for, None, "the test");
            let measured_ok = measured.deterministic.rows[0].fit_admitted;
            // 5. the SDK's pre-signing gate
            let sdk_v: Verdict = sdk.preflight_admission(bundle, &entry, root, &shape).map(|_| ()).map_err(|e| (e, None, None));
            // the ladder clock, for the record: what the measured model charged before it read `shape.held`
            let ladder = palw_model_fit_v2(
                &profile,
                bundle,
                shape.court,
                params.palw_prompt_ids_form_at(daa),
                palw_fit_regime_for_v1(PalwHeldAdmissionV1::default(), &profile),
            );
            println!(
                "n_ctx {n_ctx:>5} daa {daa:>5}: gate {gate:?} | node {node_v:?} | fit admitted {} | measured {measured_ok} | sdk {} | ladder-clock fit admitted {}",
                fit.admitted(),
                if sdk_v.is_ok() { "ok".to_string() } else { format!("{sdk_v:?}") },
                ladder.admitted()
            );
            // THE COURT WINDOW is the question, and the readers of it must agree: the gate's own window bound (no `COURT_WINDOW_TOO_SHORT`),
            // the fit report and the measured model all admit the window, on every height — none charges the ladder clock (5,102).
            let gate_court_ok = !matches!(&gate, Err((code, ..)) if code == "COURT_WINDOW_TOO_SHORT");
            for (who, said) in [("fit", fit.admitted()), ("measured", measured_ok)] {
                if said != gate_court_ok {
                    disagreements.push(format!("n_ctx {n_ctx} daa {daa}: the gate's window bound says {gate_court_ok}, {who} says {said}"));
                }
            }
            assert!(!ladder.admitted(), "the ladder clock would refuse this class (the 5,102 figure) — the reader this test exists to catch");
            // The processor-same readers (the node's preflight and the SDK's pre-signing gate) refuse by the SAME code, and never by the window.
            if let Err((code, ..)) = &node_v {
                assert_ne!(code, "COURT_WINDOW_TOO_SHORT", "n_ctx {n_ctx} daa {daa}: the node charged the window");
                match &sdk_v {
                    Err((msg, ..)) if msg.contains(code.as_str()) => {}
                    other => disagreements.push(format!("n_ctx {n_ctx} daa {daa}: the node refuses {code}, the SDK says {other:?}")),
                }
            } else if sdk_v.is_err() {
                disagreements.push(format!("n_ctx {n_ctx} daa {daa}: the node admits, the SDK refuses: {sdk_v:?}"));
            }
            // The class of the report (Huihui-Qwen3.5-9B, n_ctx 16): refused on testnet-12, but by the held-class rule (30 recurrent layers
            // answer no windowed builder, ADR-0152 §4-ter C5), never by a 5,102-DAA window.
            // int-12: from the 5,300 flag day `palw_gdn_key_heads` is armed (P0a), and the gate itself refuses this class's version-2 profile on the
            // v5 map (16 key heads over 32 value heads) — every reader above names that one code (a Qwen3.5 profile must be rebuilt with explicit key
            // heads to register past 5,300).
            if n_ctx == 16 {
                let flag_day = kaspa_consensus_core::config::params::PALW_T12_INT11_FLAG_DAY_DAA.expect("the int-12 flag day");
                let want = if daa >= flag_day { "GDN_MAP_ASSUMES_EQUAL_HEADS" } else { "HELD_CLASS_UNANSWERABLE" };
                assert_eq!(node.reject_code, want, "daa {daa}");
            }
        }
    }
    assert!(disagreements.is_empty(), "readers disagree:\n{}", disagreements.join("\n"));
}

fn object_registration(o: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2) -> kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 {
    o.clone()
}
