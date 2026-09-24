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
        assert_eq!(palw_held_class_is_attributable_v1(&row(n_ctx), true, T12_TURN), Ok(()), "n_ctx {n_ctx} is answerable");
    }
    for n_ctx in [16_384u32, 2_097_152] {
        let refused = palw_held_class_is_attributable_v1(&row(n_ctx), true, T12_TURN).expect_err("unattributable");
        assert_eq!(refused, PalwClassAdmissionError::HeldClassUnattributable { n_ctx, bound: PALW_HELD_ANSWERABLE_N_CTX_V1 });
        assert_eq!(refused.code(), "HELD_CLASS_UNATTRIBUTABLE");
        assert_eq!(palw_held_class_is_attributable_v1(&row(n_ctx), false, T12_TURN), Ok(()), "below the fence the gate is as it was");
    }
    let floor =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the floor");
    assert!(!kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&floor));
    assert_eq!(palw_held_class_is_attributable_v1(&floor, true, T12_TURN), Ok(()), "a class that is not held is never asked");
}

/// testnet-12's court turn (`PalwCourtParamsV2::turn_deadline_daa`, derived from its window and
/// ladder — `the_answerability_budget_is_testnet_12s_turn` pins it).
const T12_TURN: u64 = 42;

/// **The review's F4 and F5: answerability is a class's replay, not its context alone.** One
/// predicate, `palw_held_class_unanswerable_v1`, behind the bundle's mirror (C1), a registration (C5)
/// and a backend's `supports_dissection` (N4):
/// * F5 — a held dense row whose whole-context reference replay does not fit one 42-DAA turn is
///   unanswerable inside the context bound: Qwen2.5-3B at 8,192 (the 1.5B at 8,192 fits);
/// * F4 — a held hybrid row (Qwen3.6, recurrent layers) is unanswerable at any context: no family has
///   a windowed builder for it;
/// * testnet-12's genesis 8k row stays answerable and its mirror is the 2M row alone; the budget is
///   its own turn.
#[test]
fn f4_f5_answerability_is_the_classs_replay_inside_a_turn() {
    use kaspa_consensus_core::palw_class_admission_v2::{
        PALW_HELD_ANSWER_REPLAYS_PER_TURN_V1, PalwClassAdmissionError, PalwHeldUnanswerableV1, palw_held_class_is_attributable_v1,
        palw_held_class_unanswerable_v1,
    };
    let t12 = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("testnet-12 runs V2") };
    assert_eq!(bundle.court.turn_deadline_daa(), T12_TURN, "the budget is testnet-12's own turn");
    assert_eq!(PALW_HELD_ANSWER_REPLAYS_PER_TURN_V1, 1);

    // The 1.5B row at 8,192 fits; the 3B row at the same context does not (F5).
    let dense = |geometry: kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1| {
        kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(geometry).expect("a held dense row")
    };
    let small = dense(kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        n_ctx: 8_192,
        ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B
    });
    let large = dense(kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        n_ctx: 8_192,
        ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_3B
    });
    assert_eq!(palw_held_class_unanswerable_v1(&small, T12_TURN), None, "the 8k 1.5B row is answerable");
    let Some(PalwHeldUnanswerableV1::ReplayPastTurn { replay_ms, budget_ms }) = palw_held_class_unanswerable_v1(&large, T12_TURN)
    else {
        panic!("the 8k 3B row's replay does not fit the turn")
    };
    assert_eq!(budget_ms, T12_TURN * 120_000, "one reference replay a turn: 42 × 120 s");
    assert!(replay_ms > budget_ms, "{replay_ms} ms > {budget_ms} ms");
    let refused = palw_held_class_is_attributable_v1(&large, true, T12_TURN).expect_err("C5 refuses it past the fence");
    assert_eq!(
        refused,
        PalwClassAdmissionError::HeldClassUnanswerable { why: PalwHeldUnanswerableV1::ReplayPastTurn { replay_ms, budget_ms } }
    );
    assert_eq!(refused.code(), "HELD_CLASS_UNANSWERABLE");
    assert_eq!(palw_held_class_is_attributable_v1(&large, false, T12_TURN), Ok(()), "below the fence, as it was");
    // A shorter turn makes the same 1.5B row unanswerable: the budget is the network's own.
    assert!(matches!(palw_held_class_unanswerable_v1(&small, 20), Some(PalwHeldUnanswerableV1::ReplayPastTurn { .. })));

    // F4: the held hybrid row, at a context the dense one answers.
    let hybrid = kaspa_consensus_core::palw_context_ladder::palw_qwen36_context_row_profile_v7(512).expect("a held hybrid row");
    assert!(kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&hybrid));
    assert!(matches!(palw_held_class_unanswerable_v1(&hybrid, T12_TURN), Some(PalwHeldUnanswerableV1::Recurrent { .. })));
    assert!(matches!(
        palw_held_class_is_attributable_v1(&hybrid, true, T12_TURN),
        Err(PalwClassAdmissionError::HeldClassUnanswerable { why: PalwHeldUnanswerableV1::Recurrent { .. } })
    ));

    // testnet-12's genesis: the 8k row answerable, the mirror the 2M row alone.
    let sp = &bundle.state;
    let held: Vec<_> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(c), .. }
                if kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&c.profile) =>
            {
                Some((*class_id, c.profile.n_ctx, palw_held_class_unanswerable_v1(&c.profile, T12_TURN)))
            }
            _ => None,
        })
        .collect();
    for (class_id, n_ctx, why) in &held {
        assert_eq!(why.is_some(), *n_ctx > 8_192, "genesis row {class_id} at n_ctx {n_ctx}: {why:?}");
        assert_eq!(sp.held_class_is_unanswerable_v1(class_id), why.is_some(), "the mirror is the predicate");
    }
    assert_eq!(held.iter().filter(|(_, _, why)| why.is_none()).count(), 1, "the 8k row, answerable");
}
