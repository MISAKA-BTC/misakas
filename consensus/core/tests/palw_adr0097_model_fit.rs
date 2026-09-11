//! **ADR-0097 — a model's fit is a lookup.**
//!
//! Section 5's invariants, as tests. Each states the RULE and derives its numbers from the shipped
//! presets through `palw_model_fit_v1`; where a number is pinned it is pinned as a LIMITATION —
//! the widest context a wall admits today — so that the day a wall moves, the pin says so rather
//! than staying green (`pin-the-limitation-not-just-the-behaviour`). The generator is
//! `misaka-palw-base0 --bin palw-model-fit`; these are the assertions that make its findings hold.

use kaspa_consensus_core::config::params::{Params, devnet_shipped_params, palw_rc_shipped_params};
use kaspa_consensus_core::palw_class_admission_v2::{PalwKaryCourtV1, palw_admission_shape_at_v1};
use kaspa_consensus_core::palw_context_ladder::{palw_a16_context_row_profile_v5, palw_qwen36_context_row_profile_v5};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2, PalwCourtParamsV2};
use kaspa_consensus_core::palw_model_fit_v1::{
    PalwFitVerdictV1, PalwFitWallV1, PalwModelFitReportV1, palw_fewest_layers_refused_at_context_v1, palw_geometry_ceiling_fit_v1,
    palw_model_fit_v1, palw_widest_context_under_the_geometry_ceiling_v1, stand_ins,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B, QWEN25_A16_GRAPH_V5_N_CTX};
use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_artifact_row_profile_v5};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, PalwStepError};

/// The point of judgement every fit here is read at: every scheduled fence armed, every
/// `never()` fence dormant — "on this ruleset, ever".
const EVER: u64 = u64::MAX - 1;

fn bundle(params: &Params) -> &PalwConsensusParamsV2 {
    match &params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle,
        _ => panic!("the preset ships no ConsensusV2 bundle; this test's premise is wrong, not its subject"),
    }
}

/// The court and the id form a registration of `profile` is judged under — the acceptance path's
/// own spelling, `palw_admission_shape_at_v1`.
fn shape(params: &Params, profile: &PalwShapeProfileV3) -> (Option<PalwKaryCourtV1>, PalwPromptIdsFormV1) {
    let shape = palw_admission_shape_at_v1(params, bundle(params), profile, EVER).expect("the preset has an admission shape");
    (shape.court, params.palw_prompt_ids_form_at(EVER))
}

fn fit(params: &Params, profile: &PalwShapeProfileV3) -> PalwModelFitReportV1 {
    let (court, form) = shape(params, profile);
    palw_model_fit_v1(profile, bundle(params), court, form)
}

fn kimi_k3(n_ctx: u32) -> Result<PalwShapeProfileV3, PalwStepError> {
    qwen36_artifact_row_profile_v5(PalwQwen36GeometryV1 { n_ctx, ..stand_ins::KIMI_K3_AS_HYBRID_V1 })
}

fn presets() -> [(&'static str, Params); 2] {
    [("testnet-11 (RC)", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())]
}

/// The widest `n_ctx` at which `wall` admits the row a family builds, by bisection under the
/// geometry ceiling. Mirrors the generator's `widest_admitted`.
fn widest_admitted(
    params: &Params,
    build: fn(u32) -> Result<PalwShapeProfileV3, PalwStepError>,
    wall: PalwFitWallV1,
    hi: u32,
) -> Option<u32> {
    let admits = |n_ctx: u32| {
        build(n_ctx).ok().and_then(|p| fit(params, &p).row(wall).map(|r| r.verdict == PalwFitVerdictV1::Admitted)).unwrap_or(false)
    };
    if !admits(1) {
        return None;
    }
    if admits(hi) {
        return Some(hi);
    }
    let (mut lo, mut hi) = (1u32, hi);
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if admits(mid) { lo = mid } else { hi = mid }
    }
    Some(lo)
}

/// **Invariant 1 — positive control.** Every row a shipped preset's GENESIS registers with a
/// carried profile is admitted on EVERY wall of that preset. The chain admitted these rows when it
/// was cut; a report that refused one of them would be a report disagreeing with the chain it
/// describes. Read off `genesis_objects` rather than off a list this test keeps, so a preset that
/// registers a new row is covered the day it does.
#[test]
fn every_genesis_row_is_admitted_on_every_wall_of_its_own_preset() {
    for (name, params) in presets() {
        let mut carried = 0usize;
        for object in &bundle(&params).genesis_objects {
            let PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(carriage), .. } = object else { continue };
            carried += 1;
            let report = fit(&params, &carriage.profile);
            assert!(
                report.admitted(),
                "{name}: genesis row {class_id} (n_ctx {}, {} layers) is refused by {:?} (unpriced {:?}) — the report disagrees with the chain",
                report.n_ctx,
                report.layer_count,
                report.refusing_walls(),
                report.unpriced_walls()
            );
            assert_eq!(report.rows.len(), PalwFitWallV1::ALL.len(), "every wall is a row");
            if report.fused {
                assert_eq!(
                    report.arity_played,
                    report.arity_derived_for_this_row.expect("an arity derives for a registered fused row")
                );
            }
        }
        assert!(carried > 0, "{name} registers no row with a carried profile; the control has nothing to control");
    }
    // And the dense family's own row at the width testnet-11 registers it, on the RC, by name.
    let params = palw_rc_shipped_params();
    let dense = fit(&params, &palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).expect("the dense row builds"));
    assert!(dense.admitted(), "{:?}", dense.refusing_walls());
    assert!(dense.fused);
}

/// **Invariant 2 — the dense row's fit on the RC is 512, and the wall at 513 is the court window.**
/// ADR-0092 §8's finding restated as a lookup: the ladder admits the dense row past 512 (the
/// widest it admits is pinned below), and the window does not, so the row ships at the width the
/// clock allows and not one position more. The pin fails in both directions.
#[test]
fn the_dense_rows_fit_on_the_rc_is_the_court_window_at_512() {
    let params = palw_rc_shipped_params();
    let at = |n_ctx: u32| fit(&params, &palw_a16_context_row_profile_v5(n_ctx).expect("the dense family builds under the ceiling"));
    assert!(at(512).admitted());
    let over = at(513);
    assert_eq!(over.refusing_walls(), vec![PalwFitWallV1::CourtWindow], "at 513 the ONLY wall is the window: {:?}", over.rows);
    let window = over.row(PalwFitWallV1::CourtWindow).unwrap();
    assert!(window.need > window.have, "{window:?}");

    let hi = palw_widest_context_under_the_geometry_ceiling_v1(QWEN25_1_5B.layer_count);
    assert_eq!(widest_admitted(&params, palw_a16_context_row_profile_v5, PalwFitWallV1::CourtWindow, hi), Some(512));
    // The LIMITATION, pinned: the RC ladder admits the dense graph-v5 row to exactly this width and
    // no further. `palw_qwen25_profile` once said "at most 574"; this is the number the ruleset's
    // own predicate gives today, and a change to either the ladder or the graph moves it.
    assert_eq!(widest_admitted(&params, palw_a16_context_row_profile_v5, PalwFitWallV1::Ladder, hi), Some(651));
    // Past the ladder the close cannot be priced at all: the derivation's walk is capped at the
    // ladder (audit D H-5), so `unpriced` is what the close says of a row the ladder refuses.
    let deep = at(652);
    assert_eq!(deep.refusing_walls(), vec![PalwFitWallV1::Ladder, PalwFitWallV1::CourtWindow]);
    assert_eq!(
        deep.unpriced_walls(),
        vec![PalwFitWallV1::CloseBytes, PalwFitWallV1::CloseChunks, PalwFitWallV1::TerminalMacs, PalwFitWallV1::OperandCount]
    );
}

/// **Invariant 2, the hybrid half.** The hybrid family's row is at 8; at 512 it is refused by the
/// ladder ALONE — the window admits it, the cache chunks admit it — so what keeps the structurally
/// better long-context family (ADR-0081 §1.1) at 8 positions is one number inside the ruleset id.
#[test]
fn the_hybrid_row_is_refused_at_512_by_the_ladder_alone() {
    let params = palw_rc_shipped_params();
    let report = fit(&params, &palw_qwen36_context_row_profile_v5(512).expect("the hybrid family builds at 512"));
    assert_eq!(report.refusing_walls(), vec![PalwFitWallV1::Ladder], "{:?}", report.rows);
    let hi = palw_widest_context_under_the_geometry_ceiling_v1(QWEN36_35B_A3B.layer_count);
    assert_eq!(widest_admitted(&params, palw_qwen36_context_row_profile_v5, PalwFitWallV1::Ladder, hi), Some(204));
}

/// **Invariant 3 — a 2M context is refused by the geometry ceiling for every model deeper than
/// eight layers, and a 1M context for every model deeper than sixteen.** Pure arithmetic over
/// `PALW_STEP_MAX_ENUMERATION`; no family, no ruleset. This is the first wall and the one a
/// person asking about "2M context" meets before any court is consulted.
#[test]
fn a_2m_context_is_refused_by_the_geometry_ceiling_past_eight_layers() {
    assert_eq!(palw_fewest_layers_refused_at_context_v1(1 << 21), Some(9));
    assert_eq!(palw_fewest_layers_refused_at_context_v1(1 << 20), Some(17));
    for (layers, widest) in [
        (QWEN25_1_5B.layer_count, 599_186u32),
        (QWEN36_35B_A3B.layer_count, 419_430),
        (stand_ins::KIMI_K3_AS_HYBRID_V1.layer_count, 182_361),
    ] {
        assert_eq!(palw_widest_context_under_the_geometry_ceiling_v1(layers), widest);
        assert_eq!(palw_geometry_ceiling_fit_v1(1 << 21, layers).verdict, PalwFitVerdictV1::Refused);
        assert_eq!(palw_geometry_ceiling_fit_v1(1 << 20, layers).verdict, PalwFitVerdictV1::Refused);
        assert_eq!(palw_geometry_ceiling_fit_v1(widest, layers).verdict, PalwFitVerdictV1::Admitted);
        assert_eq!(palw_geometry_ceiling_fit_v1(widest + 1, layers).verdict, PalwFitVerdictV1::Refused);
    }
    // And the family builders refuse to build past it, in the sentence `validate_geometry` uses —
    // so there is no profile to price and the report's first row is the whole report.
    for (build, n_ctx) in [
        (palw_a16_context_row_profile_v5 as fn(u32) -> Result<PalwShapeProfileV3, PalwStepError>, 1u32 << 20),
        (palw_qwen36_context_row_profile_v5, 1 << 20),
        (kimi_k3, 1 << 20),
        (kimi_k3, 1 << 21),
    ] {
        match build(n_ctx) {
            Err(PalwStepError::ProfileNotCanonical(why)) => assert!(why.contains("enumeration"), "{why}"),
            other => panic!("a row at {n_ctx} past the ceiling must be refused by validate_geometry, got {other:?}"),
        }
    }
}

/// **Invariant 4 — the Kimi K3 stand-in, on the shipped ruleset.** At the model card's context it
/// has no profile (Invariant 3). Under the ceiling it is refused at the ladder at every width
/// past TEN positions, and at 131,072 positions by four walls at once — the ladder, the window,
/// the cache chunks and the PublicDa payload — with the close unpriceable past the ladder. Pinned
/// as the limitation it is: no single fix admits this row, and the ladder is inside the ruleset
/// id (ADR-0092 Decision 4).
#[test]
fn the_kimi_k3_stand_in_fits_the_rc_at_ten_positions_and_is_refused_by_four_walls_at_128k() {
    let params = palw_rc_shipped_params();
    let hi = palw_widest_context_under_the_geometry_ceiling_v1(stand_ins::KIMI_K3_AS_HYBRID_V1.layer_count);
    assert_eq!(widest_admitted(&params, kimi_k3, PalwFitWallV1::Ladder, hi), Some(10), "the ladder's widest context for the stand-in");
    let at = |n_ctx: u32| fit(&params, &kimi_k3(n_ctx).expect("the stand-in builds under the ceiling"));
    assert!(at(10).row(PalwFitWallV1::Ladder).unwrap().verdict == PalwFitVerdictV1::Admitted);
    assert_eq!(at(512).refusing_walls(), vec![PalwFitWallV1::Ladder]);
    let wide = at(131_072);
    assert_eq!(
        wide.refusing_walls(),
        vec![PalwFitWallV1::Ladder, PalwFitWallV1::CourtWindow, PalwFitWallV1::StateChunks, PalwFitWallV1::PublicDaPayload],
        "{:?}",
        wide.rows
    );
    assert_eq!(wide.unpriced_walls().len(), 4, "the four close walls cannot price a row the ladder refuses");
    // The seat footprint is arithmetic on the geometry: 23 attention layers of 6 × 128 i32 rows,
    // K and V, at 131,072 positions.
    assert_eq!(wide.seat.attention_layers, 23);
    assert_eq!(wide.seat.recurrent_layers, 69);
    assert_eq!(wide.seat.kv_row_bytes, 6 * 128 * 4);
    assert_eq!(wide.seat.kv_cache_bytes, 23 * 2 * (6 * 128 * 4) * 131_072);
    assert_eq!(wide.seat.prompt_ids_bytes, 131_072 * 4);
}

/// **Invariant 5 — a wall reads its ceiling (negative control).** The same stand-in against the
/// same ruleset with the ladder raised to 2^40: the ladder row flips to admitted and the close
/// becomes priceable. Proves the predicate reads the bundle it is handed and not a constant.
#[test]
fn raising_the_ladder_flips_the_ladder_row_and_prices_the_close() {
    let params = palw_rc_shipped_params();
    let profile = kimi_k3(512).expect("the stand-in builds at 512");
    let (court, form) = shape(&params, &profile);
    let before = palw_model_fit_v1(&profile, bundle(&params), court, form);
    assert_eq!(before.refusing_walls(), vec![PalwFitWallV1::Ladder]);
    assert!(!before.unpriced_walls().is_empty());

    let mut raised = bundle(&params).clone();
    let rc = raised.court;
    raised.court = PalwCourtParamsV2::with_cost_ceilings(
        1 << 40,
        rc.turn_deadline_daa(),
        rc.terminal_rounds(),
        rc.max_close_bytes(),
        rc.max_terminal_macs(),
        rc.max_operand_count(),
    )
    .expect("a deeper ladder is a legal court")
    .with_dissection_arity(rc.dissection_arity())
    .expect("the arity is legal");
    let after = palw_model_fit_v1(&profile, &raised, court, form);
    assert_eq!(after.row(PalwFitWallV1::Ladder).unwrap().verdict, PalwFitVerdictV1::Admitted);
    assert_eq!(after.row(PalwFitWallV1::Ladder).unwrap().have, 1 << 40);
    assert!(after.unpriced_walls().is_empty(), "under a ladder that holds the row, the close prices: {:?}", after.rows);
    // And a deeper ladder costs rounds: the window row's need rose by exactly the fourteen extra
    // ladder rounds, twice over (both parties), at the turn deadline.
    let (w0, w1) = (before.row(PalwFitWallV1::CourtWindow).unwrap().need, after.row(PalwFitWallV1::CourtWindow).unwrap().need);
    assert_eq!(w1 - w0, 2 * 14 * rc.turn_deadline_daa());
}

/// **Invariant 6 — the report agrees with the ruleset's own reading of every ceiling.** Each
/// `have` is the bundle's number, read back; a report that carried a transcribed ceiling would be
/// a document, not a lookup.
#[test]
fn every_have_is_the_rulesets_own_number() {
    for (_, params) in presets() {
        let b = bundle(&params);
        let profile = palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).unwrap();
        let report = fit(&params, &profile);
        let have = |wall| report.row(wall).unwrap().have;
        assert_eq!(have(PalwFitWallV1::Ladder), b.court.max_step_leaf_count());
        assert_eq!(have(PalwFitWallV1::CloseBytes), b.court.max_close_bytes());
        assert_eq!(have(PalwFitWallV1::CloseChunks), b.court.max_close_chunks());
        assert_eq!(have(PalwFitWallV1::TerminalMacs), b.court.max_terminal_macs());
        assert_eq!(have(PalwFitWallV1::OperandCount), b.court.max_operand_count() as u64);
        assert_eq!(have(PalwFitWallV1::CourtWindow), b.state.window_court() - 1, "strict: the backstop closes on the challenger");
        assert_eq!(have(PalwFitWallV1::StateChunks), kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_STATE_CHUNKS as u64);
        assert_eq!(have(PalwFitWallV1::PublicDaPayload), kaspa_consensus_core::palw_mode_v2::PALW_STANDARD_TX_BYTES);
        assert_eq!(have(PalwFitWallV1::GeometryCeiling), kaspa_consensus_core::palw_step::PALW_STEP_MAX_ENUMERATION);
        assert_eq!(
            report.answer_tokens_per_job,
            b.freeprompt.max_decode_tokens().min(kaspa_consensus_core::palw_v2::PALW_V2_MAX_TRACE_EVENTS as u32)
        );
    }
}
