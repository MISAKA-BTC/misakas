//! **ADR-0152 §4-ter (A-held): which held classes an honest party can dissect inside a turn.**
//!
//! `PALW_HELD_ANSWERABLE_N_CTX_V1` (8,192) splits testnet-12's two genesis held rows: the 8k row is
//! answerable — its dissection is played, a fused accusation of it opens a held session — and the 2M
//! row is not: no honest move fits a 42-DAA turn on either side, so a fused accusation of it is
//! refused (the shard-court addendum) and its responder keeps the mercy (C1). The fold holds no
//! `Params`, so the split rides the V2 bundle as a `#[borsh(skip)]` mirror
//! (`PalwStateParamsV2::held_unanswerable_classes`), written by `Params::sync_palw_held_answerability`
//! and checked by `validate_palw_v2`. These tests pin: testnet-12's mirror is exactly the 2M row;
//! every other preset's is empty; a disagreeing mirror is a startup refusal; the fence taken away
//! empties it; and nothing about it moves an id (it is derived, never hashed).

use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_RCORE_CONSERVATIVE_CLASSES, Params, devnet_shipped_params, mainnet_shipped_params,
    palw_rc_shipped_params, palw_t12_genesis_held_class_ids_v1, palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{PALW_HELD_ANSWERABLE_N_CTX_V1, PalwConsensusObjectV2};

fn bundle_state(p: &Params) -> kaspa_consensus_core::palw_state_v2::PalwStateParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.clone(),
        _ => panic!("a ConsensusV2 preset"),
    }
}

/// testnet-12's genesis held rows at their widths: `(class id, n_ctx)`, read off the bundle's own
/// genesis registrations — the same rows the mirror is derived from.
fn t12_held_rows(p: &Params) -> Vec<(kaspa_consensus_core::Hash64, u32)> {
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("V2") };
    bundle
        .genesis_objects
        .iter()
        .filter_map(|object| match object {
            PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(c), .. }
                if kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&c.profile) =>
            {
                Some((*class_id, c.profile.n_ctx))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn testnet12s_unanswerable_held_classes_are_the_2m_row_alone() {
    let t12 = palw_t12_shipped_params();
    assert_eq!(PALW_HELD_ANSWERABLE_N_CTX_V1, 8_192, "the bound §4-ter derives: the 8k row's own width");
    let rows = t12_held_rows(&t12);
    let mut ids: Vec<_> = rows.iter().map(|(id, _)| *id).collect();
    ids.sort();
    let mut shipped = palw_t12_genesis_held_class_ids_v1();
    shipped.sort();
    assert_eq!(ids, shipped, "the bundle registers exactly the two genesis held rows");
    let (eight_k, _) = *rows.iter().find(|(_, n_ctx)| *n_ctx == 8_192).expect("the 8k row");
    let (two_m, n_ctx) = *rows.iter().find(|(_, n_ctx)| *n_ctx > 8_192).expect("the 2M row");
    assert_eq!(n_ctx, 2_097_152);
    let mirror = bundle_state(&t12);
    assert_eq!(mirror.held_unanswerable_classes(), &[two_m], "the mirror is the 2M row");
    assert_eq!(mirror.held_unanswerable_classes(), PALW_T12_RCORE_CONSERVATIVE_CLASSES.as_slice(), "the same class C7 holds");
    assert!(mirror.held_class_is_unanswerable_v1(&two_m));
    assert!(!mirror.held_class_is_unanswerable_v1(&eight_k), "the 8k row is answerable: its dissection is played");
    assert_eq!(t12.palw_held_unanswerable_classes_v1(), vec![two_m], "the derivation the mirror is checked against");
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
}

#[test]
fn every_other_preset_has_no_unanswerable_class() {
    for (name, p) in
        [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params()), ("mainnet", mainnet_shipped_params())]
    {
        assert!(p.palw_offence_attribution_fence().is_none(), "{name}: the fence is testnet-12's alone");
        assert!(p.palw_held_unanswerable_classes_v1().is_empty(), "{name}: nothing derives without the fence");
        if let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode {
            assert!(bundle.state.held_unanswerable_classes().is_empty(), "{name}: the mirror is empty");
        }
    }
}

/// **A missed sync is a startup refusal**, never a 2M class quietly read as answerable (whose honest
/// producers the lifted mercy would then convict for a move nobody can make).
#[test]
fn a_mirror_that_disagrees_with_its_rows_is_refused_at_startup() {
    let t12 = palw_t12_shipped_params();
    let mut blank = t12.clone();
    if let PalwConsensusMode::ConsensusV2(bundle) = &mut blank.palw_consensus_mode {
        bundle.state = bundle.state.clone().with_held_unanswerable_classes(Vec::new());
    }
    let refused = blank.validate_palw_v2().expect_err("an empty mirror under the fence reads the 2M row as answerable");
    assert!(format!("{refused:?}").contains("held_unanswerable_classes"), "{refused:?}");
    blank.sync_palw_held_answerability();
    blank.validate_palw_v2().expect("re-synced, it validates");
    assert_eq!(bundle_state(&blank), bundle_state(&t12));

    let mut widened = t12.clone();
    let rows = t12_held_rows(&t12);
    if let PalwConsensusMode::ConsensusV2(bundle) = &mut widened.palw_consensus_mode {
        bundle.state = bundle.state.clone().with_held_unanswerable_classes(rows.iter().map(|(id, _)| *id).collect());
    }
    assert!(widened.validate_palw_v2().is_err(), "a mirror naming the 8k row too is refused (it would keep the 8k mercy)");
}

/// **The fence taken away empties the mirror**, and the ids never read it: it is derived from rows
/// already inside the ruleset id, and borsh-skipped.
#[test]
fn the_mirror_follows_the_fence_and_moves_no_id() {
    let t12 = palw_t12_shipped_params();
    let mut never = t12.clone();
    never.palw_offence_attribution = Some(ForkActivation::never());
    never.palw_rcore_plus = None;
    never.palw_rcore_conservative_classes = &[];
    never.sync_palw_rcore_plus();
    assert!(bundle_state(&never).held_unanswerable_classes().is_empty(), "no fence, no unanswerable class");
    never.validate_palw_v2().expect("absence validates");

    let mut cleared = t12.clone();
    if let PalwConsensusMode::ConsensusV2(bundle) = &mut cleared.palw_consensus_mode {
        bundle.state = bundle.state.clone().with_held_unanswerable_classes(Vec::new());
    }
    assert_eq!(cleared.consensus_params_id(), t12.consensus_params_id(), "the mirror is not in the ruleset id");
    assert_eq!(cleared.consensus_identity_id(), t12.consensus_identity_id());
    assert_eq!(cleared.consensus_schedule_id(), t12.consensus_schedule_id());
    let bytes = |p: &Params| borsh::to_vec(&bundle_state(p)).expect("borsh");
    assert_eq!(bytes(&cleared), bytes(&t12), "borsh-skipped: the bundle's bytes do not carry it");
}

/// **C5: past the fence a held class is admitted only where its attention lie is attributable.** A
/// held (graph-v7) row at `n_ctx ≤ 8,192` passes; past it — 16,384, the 2M row's 2,097,152 — it is
/// refused by name; below the fence every one passes (the gate as it was); a class that does not
/// register the held map is never asked (the floor's non-held profile, at any width).
#[test]
fn c5_a_held_class_past_the_answerable_context_is_refused_past_the_fence() {
    use kaspa_consensus_core::palw_class_admission_v2::{PalwClassAdmissionError, palw_held_class_is_attributable_v1};
    let row = |n_ctx: u32| kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v7(n_ctx).expect("a held row");
    for n_ctx in [1_024u32, 4_096, 8_192] {
        assert_eq!(palw_held_class_is_attributable_v1(&row(n_ctx), true), Ok(()), "n_ctx {n_ctx} is answerable");
    }
    for n_ctx in [16_384u32, 2_097_152] {
        let refused = palw_held_class_is_attributable_v1(&row(n_ctx), true).expect_err("unattributable");
        assert_eq!(refused, PalwClassAdmissionError::HeldClassUnattributable { n_ctx, bound: PALW_HELD_ANSWERABLE_N_CTX_V1 });
        assert_eq!(refused.code(), "HELD_CLASS_UNATTRIBUTABLE");
        assert_eq!(palw_held_class_is_attributable_v1(&row(n_ctx), false), Ok(()), "below the fence the gate is as it was");
    }
    let floor =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the floor");
    assert!(!kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&floor));
    assert_eq!(palw_held_class_is_attributable_v1(&floor, true), Ok(()), "a class that is not held is never asked");
}

/// testnet-12's court turn (`PalwCourtParamsV2::turn_deadline_daa`, derived from its window and
/// ladder): the base turn every response-only move keeps.
const T12_TURN: u64 = 42;

/// **The review's F4 and F5, and the user's decision on F5: answerability is the class's compute
/// turn.** One predicate, `palw_held_class_unanswerable_v1`, a pure function of the profile, behind
/// the bundle's mirror (C1), a registration (C5) and a backend's `supports_dissection` (N4):
/// * the compute turn is `⌈2 × reference replay / 120 s⌉` DAA — testnet-12's genesis 8k row (1.5B):
///   3,423,941 ms → 58; the 3B row at 8,192: 6,951,424 ms → 116, inside the 120-DAA cap; a wider
///   model at 8,192 (the 3B geometry at twice its depth) past it — refused by C5 past the fence only;
/// * a compute move's turn is `max(42, compute)`; a class with no compute turn keeps 42;
/// * at the cap a whole held dissection fits testnet-12's court window — exactly `2 × 120 + 26 × 42 +
///   reserve + 1` DAA, one less refused — and testnet-12 passes the startup check;
/// * F4: a held hybrid row (Qwen3.6, recurrent layers) is unanswerable at any context;
/// * testnet-12's genesis 8k row stays answerable and its mirror is the 2M row alone.
#[test]
fn f4_f5_answerability_is_the_classs_compute_turn() {
    use kaspa_consensus_core::palw_class_admission_v2::{
        PALW_HELD_COMPUTE_REPLAYS_V1, PALW_HELD_COMPUTE_TURN_CAP_DAA_V1, PalwClassAdmissionError, PalwHeldUnanswerableV1,
        palw_held_class_is_attributable_v1, palw_held_class_unanswerable_v1, palw_held_compute_turn_daa_v1,
        palw_held_dissection_fits_court_v1, palw_held_move_turn_daa_v1, palw_held_reference_replay_ms_v1,
    };
    use kaspa_consensus_core::palw_qwen25_profile::{
        PalwQwen25GeometryV1, QWEN25_1_5B, QWEN25_3B, qwen25_a16_artifact_row_profile_v7,
    };
    let t12 = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("testnet-12 runs V2") };
    assert_eq!(bundle.court.turn_deadline_daa(), T12_TURN, "testnet-12's base turn");
    assert_eq!((PALW_HELD_COMPUTE_REPLAYS_V1, PALW_HELD_COMPUTE_TURN_CAP_DAA_V1), (2, 120));

    let dense = |geometry: PalwQwen25GeometryV1| qwen25_a16_artifact_row_profile_v7(geometry).expect("a held dense row");
    let small = dense(PalwQwen25GeometryV1 { n_ctx: 8_192, ..QWEN25_1_5B });
    let large = dense(PalwQwen25GeometryV1 { n_ctx: 8_192, ..QWEN25_3B });
    let wider = dense(PalwQwen25GeometryV1 { n_ctx: 8_192, layer_count: 2 * QWEN25_3B.layer_count, ..QWEN25_3B });
    assert_eq!(palw_held_reference_replay_ms_v1(&small), Some(3_423_941));
    assert_eq!(palw_held_compute_turn_daa_v1(&small), Some(58), "the launch gate's row: ⌈2 × 3,423.941 s / 120 s⌉");
    assert_eq!(palw_held_compute_turn_daa_v1(&large), Some(116));
    assert_eq!(palw_held_move_turn_daa_v1(&small, T12_TURN), 58, "a compute move's turn");
    let tiny = dense(PalwQwen25GeometryV1 { n_ctx: 64, ..QWEN25_1_5B });
    assert!(palw_held_compute_turn_daa_v1(&tiny).is_some_and(|turn| turn < T12_TURN));
    assert_eq!(palw_held_move_turn_daa_v1(&tiny, T12_TURN), T12_TURN, "never shorter than the base turn");
    for (name, profile) in [("1.5B", &small), ("3B", &large)] {
        assert_eq!(palw_held_class_unanswerable_v1(profile), None, "{name} at 8,192 is answerable");
        assert_eq!(palw_held_class_is_attributable_v1(profile, true), Ok(()));
    }
    let turn = palw_held_compute_turn_daa_v1(&wider).expect("priced");
    assert!(turn > PALW_HELD_COMPUTE_TURN_CAP_DAA_V1, "{turn}");
    assert_eq!(palw_held_class_unanswerable_v1(&wider), Some(PalwHeldUnanswerableV1::ComputeTurnPastCap { turn, cap: 120 }));
    let refused = palw_held_class_is_attributable_v1(&wider, true).expect_err("C5 refuses it past the fence");
    assert_eq!(
        refused,
        PalwClassAdmissionError::HeldClassUnanswerable { why: PalwHeldUnanswerableV1::ComputeTurnPastCap { turn, cap: 120 } }
    );
    assert_eq!(refused.code(), "HELD_CLASS_UNANSWERABLE");
    assert_eq!(palw_held_class_is_attributable_v1(&wider, false), Ok(()), "below the fence, as it was");

    // The court window holds a dissection at the cap: 2 × 120 + 26 × 42 + reserve + 1.
    let reserve = kaspa_consensus_core::palw_state_v2::palw_close_assembly_daa_v1(
        kaspa_consensus_core::palw_state_v2::PALW_COURT_CLOSE_MAX_CHUNKS,
    );
    let window = bundle.state.window_court();
    assert!(palw_held_dissection_fits_court_v1(T12_TURN, window, reserve), "testnet-12: {window} holds it");
    let need = 2 * 120 + 26 * T12_TURN + reserve + 1;
    assert!(
        palw_held_dissection_fits_court_v1(T12_TURN, need, reserve)
            && !palw_held_dissection_fits_court_v1(T12_TURN, need - 1, reserve)
    );
    t12.validate_palw_held_answerability_v1().expect("testnet-12's own ruleset passes the startup check");

    // F4: the held hybrid row, at a context the dense one answers.
    let hybrid = kaspa_consensus_core::palw_context_ladder::palw_qwen36_context_row_profile_v7(512).expect("a held hybrid row");
    assert!(matches!(palw_held_class_unanswerable_v1(&hybrid), Some(PalwHeldUnanswerableV1::Recurrent { .. })));
    assert!(matches!(
        palw_held_class_is_attributable_v1(&hybrid, true),
        Err(PalwClassAdmissionError::HeldClassUnanswerable { why: PalwHeldUnanswerableV1::Recurrent { .. } })
    ));

    // testnet-12's genesis: the 8k row answerable (turn 58), the mirror the 2M row alone.
    let sp = &bundle.state;
    let held: Vec<_> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(c), .. }
                if kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&c.profile) =>
            {
                Some((*class_id, c.profile.clone(), palw_held_class_unanswerable_v1(&c.profile)))
            }
            _ => None,
        })
        .collect();
    for (class_id, profile, why) in &held {
        assert_eq!(why.is_some(), profile.n_ctx > 8_192, "genesis row {class_id} at n_ctx {}: {why:?}", profile.n_ctx);
        assert_eq!(sp.held_class_is_unanswerable_v1(class_id), why.is_some(), "the mirror is the predicate");
        if profile.n_ctx == 8_192 {
            assert_eq!(palw_held_compute_turn_daa_v1(profile), Some(58), "the genesis 8k row's compute turn");
        }
    }
    assert_eq!(held.iter().filter(|(_, _, why)| why.is_none()).count(), 1, "the 8k row, answerable");
}
