//! **RFC-0004: the `palw_improvement_v1` fence and the protocol's skeleton** — dormant on every
//! network, fingerprinted with its value where armed, invisible to the identity until it fires,
//! named by the fork id, armed by the drill mover alone, refused by `validate_palw_v2` when it names
//! another build's scoring library, sign table or protocol version, ceilings past the format's caps,
//! or prerequisites the ruleset lacks — and the thirteen appended objects at tags 70–82, each named by
//! the one predicate the acceptance walk and the fold share.
//!
//! **Byte identity.** The field is hashed Some-only and collapsed whole from `Some(never())`, so a
//! `None` writes nothing: every shipped ruleset's three ids are pinned — measured before this field
//! existed — in `palw_tir_fences_are_dormant.rs` and `t12_repin_values.rs`, and this change leaves
//! those pins green. The two state tables enter `state_root` and the carriage only once written
//! (`improvement/v1`, tails `0xC3`/`0xC4`): the state module's own suites pin the empty state's root
//! and carriage, and its root-sensitivity suite shows a row in either table moves both. This file
//! adds no pin; it checks the field and the objects.
//!
//! The armed cases run over testnet-12 as shipped (`palw_t12_shipped_params`: `palw_tir_v1` at DAA
//! 2,000 and `palw_kary_court` below it) with the second IR fence, the generative fence and the decode
//! rules armed at heights no fence uses — the improvement fence's prerequisites.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::drill::{
    PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1, palw_drill_decode_rules_at_v1, palw_drill_gen_fence_at_v1, palw_drill_improve_fence_at_v1,
};
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_CAPACITY_RHO10_FENCES_V1,
    PALW_T12_DECODE_RULES_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_FENCES_V1,
    PALW_T12_TIR_FLAG_DAY_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_drill_params_v1, palw_t12_launch_params_v1, palw_t12_release_v1_params,
    palw_t12_release_v2_params, palw_t12_release_v3_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::fork_id_gate_fences_v1;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_gen_v1::PalwGenFenceV1;
use kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1;
use kaspa_consensus_core::palw_improve_candidate_v1::{PalwCandidateDeclarationsV1, PalwCandidateSubmissionV1};
use kaspa_consensus_core::palw_improve_material_v1::{
    PalwCaseReferenceV1, PalwCaseSourceV1, PalwDataUseOptInV1, PalwDatasetV1, PalwFpJobFactsV1, PalwHardCaseV1,
    PalwSetterKeysRevealV1, PalwSetterSetCommitmentV1, PalwSetterSetRevealV1, PalwTeacherLicenceV1, PalwTeachingArtifactCommitV1,
    PalwTeachingArtifactV1,
};
use kaspa_consensus_core::palw_improve_state_v1::{
    PalwImprovementPolicySetV1, PalwImprovementPolicyV1, PalwImprovementPoolFundingV1, PalwLineageRollbackV1, PalwRollbackCauseV1,
    PalwTeacherClassV1, PalwTeachingArtifactKindV1, PalwVerificationTypeV1,
};
use kaspa_consensus_core::palw_improve_v1::{
    PALW_DRILL_IMPROVE_CEILINGS_V1, PALW_DRILL_IMPROVE_FENCES_V1, PALW_DRILL_IMPROVE_V1_ENTRY, PALW_IMPROVE_COURT_VERSION_V1,
    PalwImprovementCeilingsV1, PalwImprovementFenceV1, palw_improve_scoring_set_id_v1, palw_improve_sign_table_id_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBondKeyV2, PalwConsensusObjectV2, palw_improvement_object_name_v1, palw_object_is_gen_v1, palw_object_is_improvement_v1,
    palw_object_is_tir_fence2_v1, palw_object_is_tir_v1,
};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::tx::TransactionOutpoint;

/// The second IR fence's height in these cases (a prerequisite since the coordinator's decision of
/// 2026-09-29): above testnet-12's IR flag day, used by no fence.
const FENCE2_AT: u64 = 9_999_985;
/// The generative fence's height in these cases: above testnet-12's IR flag day, used by no fence.
const GEN_AT: u64 = 9_999_990;
/// The decode rules' height in these cases (a prerequisite since the integration's decision of 2026-10-01):
/// above the generative fence's, below the improvement fence's, used by no fence.
const DECODE_AT: u64 = 9_999_992;
/// The improvement fence's: above the generative fence's, used by no fence.
const AT: u64 = 9_999_995;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x4d; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

/// Every ruleset a node can run, as it ships.
fn rulesets() -> Vec<(&'static str, Params)> {
    vec![
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET_PARAMS", TESTNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("DEVNET_PARAMS", DEVNET_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("from(mainnet)", Params::from(NetworkId::new(NetworkType::Mainnet))),
        ("from(testnet-10)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 10))),
        ("from(testnet-11)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))),
        ("from(testnet-12)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))),
        ("from(devnet)", Params::from(NetworkId::new(NetworkType::Devnet))),
        ("from(simnet)", Params::from(NetworkId::new(NetworkType::Simnet))),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
        ("palw_t12_launch_params_v1", palw_t12_launch_params_v1()),
        ("palw_t12_release_v1_params", palw_t12_release_v1_params()),
        ("palw_t12_release_v2_params", palw_t12_release_v2_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
        ("palw_t12_drill_params_v1", palw_t12_drill_params_v1(&salt())),
    ]
}

/// Testnet-12 as shipped with the second IR fence at [`FENCE2_AT`], the generative fence at
/// [`GEN_AT`] and the decode rules at [`DECODE_AT`]: every prerequisite in force.
fn t12_with_gen() -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_fence2 = Some(ForkActivation::new(FENCE2_AT));
    p.sync_palw_tir_fence2();
    p.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(GEN_AT)));
    p.sync_palw_gen_v1();
    p.palw_fp_decode_rules = Some(ForkActivation::new(DECODE_AT));
    p.sync_palw_fp_decode_rules();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("testnet-12 past its IR flag day can arm the generative fence and the decode rules: {e}"));
    p
}

fn t12_armed(at: ForkActivation) -> Params {
    let mut p = t12_with_gen();
    p.palw_improvement_v1 = Some(PalwImprovementFenceV1::drill_v1(at));
    p.sync_palw_improvement_v1();
    p
}

#[test]
fn every_shipped_ruleset_leaves_it_dormant_and_no_release_lists_it() {
    for (name, p) in rulesets() {
        assert!(p.palw_improvement_v1.is_none(), "{name}: dormant");
        assert!(p.palw_improvement_v1_fence().is_none() && !p.palw_improvement_v1_active_at(u64::MAX), "{name}: the accessor agrees");
        assert!(p.palw_fences_v1().contains(&("palw_improvement_v1", None)), "{name}: the exhaustive fence list names it");
        assert!(p.validate_palw_improvement_v1().is_ok(), "{name}: nothing to refuse");
        if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode {
            assert_eq!(bundle.state.improve_from_daa(), None, "{name}: the fold's mirror is empty");
            assert!(!bundle.state.improve_active_at(u64::MAX), "{name}: and answers no");
        }
    }
    // No testnet-12 flag day carries it: the only list naming it is the drill's.
    for list in [
        PALW_T12_POST_LAUNCH_FENCES_V1,
        PALW_T12_POST_LAUNCH_FENCES_V2,
        PALW_T12_POST_LAUNCH_FENCES_V3,
        PALW_T12_CAPACITY_FENCES_V1,
        PALW_T12_CAPACITY_RHO10_FENCES_V1,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1,
        PALW_T12_TIR_FENCE2_FENCES_V1,
        PALW_T12_DECODE_RULES_FENCES_V1,
    ] {
        assert!(list.iter().all(|f| f.name != "palw_improvement_v1"), "no testnet-12 release arms the improvement fence");
    }
    // The decode rules have a flag-day list of their own, dormant: testnet-12 as shipped does not arm them.
    let decode_names: Vec<&str> = PALW_T12_DECODE_RULES_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(decode_names, ["palw_fp_decode_rules"]);
    assert!(palw_t12_shipped_params().palw_fp_decode_rules.is_none(), "dormant while PALW_T12_DECODE_RULES_DAA is None");
    let names: Vec<&str> = PALW_DRILL_IMPROVE_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(names, ["palw_improvement_v1"]);
}

#[test]
fn a_scheduled_fence_moves_the_ruleset_and_the_schedule_and_never_the_identity() {
    let base = ids(&t12_with_gen());
    let armed = t12_armed(ForkActivation::new(AT));
    armed.validate_palw_v2().unwrap_or_else(|e| panic!("testnet-12 past its IR and generative fences can arm it: {e}"));
    let moved = ids(&armed);
    assert_ne!(moved.0, base.0, "the ruleset a node announces names the fence and its value");
    assert_eq!(moved.1, base.1, "two builds that differ only about a FUTURE height stay peers");
    assert_ne!(moved.2, base.2, "the schedule reports it");
    assert!(armed.palw_improvement_v1_fence().is_some());
    assert!(!armed.palw_improvement_v1_active_at(AT - 1) && armed.palw_improvement_v1_active_at(AT));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &armed.palw_consensus_mode else { panic!("V2") };
    assert_eq!(bundle.state.improve_from_daa(), Some(AT), "the fold's copy of the height");
    assert!(!bundle.state.improve_active_at(AT - 1) && bundle.state.improve_active_at(AT));
}

/// **`Λ` is the fast honest path** (spec 17 §17.4.3 rows 19-20, corrected 2026-10-01): the anchor slot, a licence allowance of
/// 30 DAA (capped by the receipt window) and the challenge window in force — never the receipt window (a deadline) nor the court
/// window. On testnet-12 and its drills (the RC windows: anchor 20, receipt 600, challenge 1,200, the 120-DAA short challenge
/// window in force from genesis) it is 170 DAA, not the 1,820 the deadlines made it; where the short window is not in force the
/// 1,200-DAA window is, and a window that arms later shortens it from then on and never before. The challenge floor holds a
/// shorter window (devnet's 100) to 120.
#[test]
fn the_claim_lifecycle_bound_is_the_fast_honest_path_and_not_the_deadlines() {
    use kaspa_consensus_core::palw_improve_v1::{
        PALW_IMPROVE_LICENCE_ALLOWANCE_DAA_V1, palw_improvement_claim_lifecycle_base_v1, palw_improvement_claim_lifecycle_v1,
    };
    let armed = t12_armed(ForkActivation::new(AT));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &armed.palw_consensus_mode else { panic!("V2") };
    let state = &bundle.state;
    assert_eq!((bundle.panel.anchor_delay(), state.window_receipt(), state.window_challenge()), (20, 600, 1_200), "the RC windows");
    assert_eq!(PALW_IMPROVE_LICENCE_ALLOWANCE_DAA_V1, 30);
    assert_eq!(palw_improvement_claim_lifecycle_base_v1(bundle), 20 + 30, "the anchor slot and the licence allowance");
    assert_eq!(state.improve_lifecycle_base_daa(), Some(50), "the mirror");
    assert_eq!(state.window_challenge_at(0), 120, "testnet-12 runs the short challenge window from genesis");
    assert_eq!(state.improve_lifecycle_at(AT), Some(170), "Λ = 20 + 30 + 120 on testnet-12 and its drills");
    // Where the short window is not in force the 1,200-DAA window is.
    let long = state.clone().with_short_challenge_window_from_daa(None);
    assert_eq!(long.improve_lifecycle_at(AT), Some(20 + 30 + 1_200));
    // The window in force at the DAA: a fence that arms later shortens Λ then and never before.
    let later = state.clone().with_short_challenge_window_from_daa(Some(500));
    assert_eq!((later.improve_lifecycle_at(499), later.improve_lifecycle_at(500)), (Some(1_250), Some(170)));
    // The challenge floor: devnet's 100-DAA window counts as 120 (anchor 4, receipt 40 gives the allowance 30).
    assert_eq!(palw_improvement_claim_lifecycle_v1(4 + 30, 100), 154);
    // Not armed, not asked.
    let dormant = t12_with_gen();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(dormant) = &dormant.palw_consensus_mode else {
        panic!("V2")
    };
    assert_eq!(dormant.state.improve_lifecycle_at(AT), None);
}

#[test]
fn never_is_absence_for_the_identity_and_genesis_is_a_rule() {
    let base = t12_with_gen();
    let never = t12_armed(ForkActivation::never());
    assert_eq!(never.consensus_identity_id(), base.consensus_identity_id(), "Some(never()) collapses whole in the normaliser");
    never.validate_palw_v2().unwrap_or_else(|e| panic!("a dormant value validates: {e}"));
    assert!(!never.palw_improvement_v1_active_at(u64::MAX - 1));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &never.palw_consensus_mode else { panic!("V2") };
    assert_eq!(bundle.state.improve_from_daa(), None, "never() mirrors as absence");
    let genesis = t12_armed(ForkActivation::always());
    assert_ne!(genesis.consensus_identity_id(), base.consensus_identity_id(), "a fence in force at genesis separates identities");
}

#[test]
fn the_value_is_fingerprinted_field_by_field() {
    let a = t12_armed(ForkActivation::new(AT));
    let with = |edit: &dyn Fn(&mut PalwImprovementFenceV1)| {
        let mut p = a.clone();
        let mut f = p.palw_improvement_v1.unwrap();
        edit(&mut f);
        p.palw_improvement_v1 = Some(f);
        p
    };
    let other = Hash64::from_bytes([9u8; 64]);
    let edits: Vec<(&str, Box<dyn Fn(&mut PalwImprovementFenceV1)>)> = vec![
        ("scoring_set_id", Box::new(move |f| f.scoring_set_id = other)),
        ("sign_table_id", Box::new(move |f| f.sign_table_id = other)),
        ("court_version", Box::new(|f| f.court_version += 1)),
        ("max_candidates_per_epoch", Box::new(|f| f.ceilings.max_candidates_per_epoch -= 1)),
        ("max_items_per_epoch", Box::new(|f| f.ceilings.max_items_per_epoch -= 1)),
        ("max_eval_positions_per_epoch", Box::new(|f| f.ceilings.max_eval_positions_per_epoch -= 1)),
        ("max_eval_budget_permille", Box::new(|f| f.ceilings.max_eval_budget_permille -= 1)),
        ("max_governed_lines", Box::new(|f| f.ceilings.max_governed_lines -= 1)),
        ("max_policy_bytes", Box::new(|f| f.ceilings.max_policy_bytes -= 1)),
        ("max_open_epochs", Box::new(|f| f.ceilings.max_open_epochs -= 1)),
        ("max_live_results", Box::new(|f| f.ceilings.max_live_results -= 1)),
        ("max_eval_seat_permille", Box::new(|f| f.ceilings.max_eval_seat_permille -= 1)),
    ];
    let mut seen = std::collections::BTreeSet::new();
    seen.insert(a.consensus_params_id().to_string());
    for (name, edit) in &edits {
        let b = with(edit.as_ref());
        assert_ne!(a.consensus_params_id(), b.consensus_params_id(), "{name} is part of the value");
        assert_ne!(a.consensus_schedule_id(), b.consensus_schedule_id(), "{name}: and the schedule report says so");
        assert_eq!(a.consensus_identity_id(), b.consensus_identity_id(), "{name}: while the height is in the future they peer");
        assert!(seen.insert(b.consensus_params_id().to_string()), "{name}: no two fields share a place in the preimage");
    }
}

#[test]
fn the_fork_id_gate_names_it_at_its_height() {
    let armed = t12_armed(ForkActivation::new(AT));
    assert!(fork_id_gate_fences_v1(&armed).contains(&AT), "past its height an un-upgraded node is refused");
    assert!(!fork_id_gate_fences_v1(&t12_with_gen()).contains(&AT));
    assert!(armed.palw_fences_v1().contains(&("palw_improvement_v1", Some(ForkActivation::new(AT)))));
}

#[test]
fn validate_refuses_every_value_this_build_cannot_run() {
    let ok = t12_armed(ForkActivation::new(AT));
    assert!(ok.validate_palw_improvement_v1().is_ok());
    let with = |edit: &dyn Fn(&mut PalwImprovementFenceV1)| {
        let mut p = ok.clone();
        let mut fence = p.palw_improvement_v1.unwrap();
        edit(&mut fence);
        p.palw_improvement_v1 = Some(fence);
        p.sync_palw_improvement_v1();
        p.validate_palw_improvement_v1()
    };
    let other = Hash64::from_bytes([7u8; 64]);
    assert!(with(&|f| f.scoring_set_id = other).is_err(), "another scoring library");
    assert!(with(&|f| f.sign_table_id = other).is_err(), "another sign-test table");
    assert!(with(&|f| f.court_version = PALW_IMPROVE_COURT_VERSION_V1 + 1).is_err(), "a protocol version this build lacks");
    let caps = PalwImprovementCeilingsV1::FORMAT_CAPS_V1;
    assert!(with(&|f| f.ceilings.max_candidates_per_epoch = 0).is_err(), "a zero ceiling admits no epoch");
    assert!(with(&|f| f.ceilings.max_items_per_epoch = 0).is_err(), "no item admits no epoch");
    assert!(with(&|f| f.ceilings.max_candidates_per_epoch = caps.max_candidates_per_epoch + 1).is_err(), "past 64 candidates");
    assert!(with(&|f| f.ceilings.max_items_per_epoch = caps.max_items_per_epoch + 1).is_err(), "past 2^16 items");
    assert!(
        with(&|f| f.ceilings.max_eval_positions_per_epoch = caps.max_eval_positions_per_epoch + 1).is_err(),
        "past 2^40 positions"
    );
    assert!(with(&|f| f.ceilings.max_eval_budget_permille = 1_001).is_err(), "past the whole capacity");
    assert!(with(&|f| f.ceilings.max_eval_seat_permille = 1_001).is_err(), "a panel's share past the whole fee");
    assert!(
        with(&|f| f.ceilings.max_eval_seat_permille = 0).is_ok(),
        "no seat share is a legal ceiling: the policy then pays its seats nothing"
    );
    assert!(with(&|f| f.ceilings.max_governed_lines = caps.max_governed_lines + 1).is_err(), "past 2^16 lines");
    assert!(with(&|f| f.ceilings.max_policy_bytes = caps.max_policy_bytes + 1).is_err(), "past 64 KiB policies");
    assert!(with(&|f| f.ceilings = caps).is_ok(), "the format's caps themselves are legal");
    assert!(with(&|f| f.activation = ForkActivation::never()).is_ok(), "a dormant value is never refused");

    let mut no_audit = ok.clone();
    no_audit.palw_audit_2026_09_11 = None;
    assert!(no_audit.validate_palw_improvement_v1().is_err(), "without A-2 an older build fails the block that carries one");
    // The IR fence: absent, above it, or dormant.
    let mut no_tir = ok.clone();
    no_tir.palw_tir_v1 = None;
    no_tir.sync_palw_tir_v1();
    assert!(no_tir.validate_palw_improvement_v1().is_err(), "a candidate is an IR class");
    let mut late_tir = ok.clone();
    late_tir.palw_tir_v1 = Some(kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT + 1)));
    late_tir.sync_palw_tir_v1();
    assert!(late_tir.validate_palw_improvement_v1().is_err(), "the IR fence must be in force at or below it");
    let mut never_tir = ok.clone();
    never_tir.palw_tir_v1 = Some(kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1::testnet12_v1(ForkActivation::never()));
    never_tir.sync_palw_tir_v1();
    assert!(never_tir.validate_palw_improvement_v1().is_err(), "a dormant IR fence is no IR fence");
    // The generative fence, likewise.
    let mut no_gen = ok.clone();
    no_gen.palw_gen_v1 = None;
    no_gen.sync_palw_gen_v1();
    assert!(no_gen.validate_palw_improvement_v1().is_err(), "an evaluation job is an RFC-0003 pipeline");
    let mut late_gen = ok.clone();
    late_gen.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(AT + 1)));
    late_gen.sync_palw_gen_v1();
    assert!(late_gen.validate_palw_improvement_v1().is_err(), "the generative fence must be in force at or below it");
    let mut same_height = ok.clone();
    same_height.palw_improvement_v1 = Some(PalwImprovementFenceV1::drill_v1(ForkActivation::new(GEN_AT)));
    same_height.sync_palw_improvement_v1();
    // (the decode rules, a prerequisite since ADR-0082 D10/D11, must be in force at or below it too)
    same_height.palw_fp_decode_rules = Some(ForkActivation::new(GEN_AT));
    same_height.sync_palw_fp_decode_rules();
    let at_gen = same_height.validate_palw_improvement_v1();
    assert!(at_gen.is_ok(), "at the generative fence's own height: {at_gen:?}");
    // The second IR fence (every evaluation pipeline sized under H7; no verdict flips mid-epoch).
    let mut no_fence2 = ok.clone();
    no_fence2.palw_tir_fence2 = None;
    no_fence2.sync_palw_tir_fence2();
    assert!(no_fence2.validate_palw_improvement_v1().is_err(), "palw_tir_fence2 is a prerequisite");
    let mut late_fence2 = ok.clone();
    late_fence2.palw_tir_fence2 = Some(ForkActivation::new(AT + 1));
    late_fence2.sync_palw_tir_fence2();
    assert!(late_fence2.validate_palw_improvement_v1().is_err(), "the second IR fence must be in force at or below it");
    // The k-ary court.
    let mut no_kary = ok.clone();
    no_kary.palw_kary_court = None;
    assert!(no_kary.validate_palw_improvement_v1().is_err(), "evaluation disputes include history dissections");
    let mut late_kary = ok.clone();
    late_kary.palw_kary_court = Some(ForkActivation::new(AT + 1));
    assert!(late_kary.validate_palw_improvement_v1().is_err(), "the k-ary court must be in force at or below it");
    // The decode rules (ADR-0082 D10/D11): an evaluation's text is a decode under the sampler's rules, and a
    // verdict must not flip when they arm mid-epoch.
    let mut no_decode = ok.clone();
    no_decode.palw_fp_decode_rules = None;
    no_decode.sync_palw_fp_decode_rules();
    let why = no_decode.validate_palw_improvement_v1().unwrap_err().to_string();
    assert!(why.contains("palw_fp_decode_rules"), "an evaluation's text is a decode under the sampler's rules: {why}");
    assert!(no_decode.validate_palw_v2().is_err(), "and validate_palw_v2 asks it");
    let mut late_decode = ok.clone();
    late_decode.palw_fp_decode_rules = Some(ForkActivation::new(AT + 1));
    late_decode.sync_palw_fp_decode_rules();
    assert!(late_decode.validate_palw_improvement_v1().is_err(), "the decode rules must be in force at or below it");
    let mut never_decode = ok.clone();
    never_decode.palw_fp_decode_rules = Some(ForkActivation::never());
    never_decode.sync_palw_fp_decode_rules();
    assert!(never_decode.validate_palw_improvement_v1().is_err(), "a dormant decode-rules fence is no decode rules");
    let mut same_decode = ok.clone();
    same_decode.palw_fp_decode_rules = Some(ForkActivation::new(AT));
    same_decode.sync_palw_fp_decode_rules();
    assert!(same_decode.validate_palw_improvement_v1().is_ok(), "at the improvement fence's own height");
    // The fold reads the fence through the bundle's mirror: a ruleset whose copy disagrees is refused.
    let mut stale = ok.clone();
    stale.palw_improvement_v1 = Some(PalwImprovementFenceV1::drill_v1(ForkActivation::new(AT + 5)));
    assert!(stale.validate_palw_improvement_v1().is_err(), "a stale mirror");
    assert!(stale.validate_palw_v2().is_err(), "and validate_palw_v2 asks it");
    stale.sync_palw_improvement_v1();
    assert!(stale.validate_palw_improvement_v1().is_ok(), "re-mirrored");

    let mut not_v2 = SIMNET_PARAMS;
    assert!(!matches!(not_v2.palw_consensus_mode, kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)));
    not_v2.palw_improvement_v1 = Some(PalwImprovementFenceV1::drill_v1(ForkActivation::new(AT)));
    assert!(not_v2.validate_palw_improvement_v1().is_err(), "a ConsensusV2 rule");
    assert!(not_v2.palw_improvement_v1_fence().is_none(), "and the accessor never answers off V2");
}

#[test]
fn the_ids_are_keyed_hashes_of_their_descriptors() {
    let keyed = |key: &[u8], bytes: &[u8]| blake2b_simd::Params::new().hash_length(64).key(key).hash(bytes).as_bytes().to_vec();
    use kaspa_consensus_core::palw_improve_v1::PALW_IMPROVE_SCORING_SET_DOMAIN_V1;
    // The scoring set: the library's descriptor (its version and reference programs), pinned by its
    // vector (consensus-vectors/tir-v2/scoring/set.json).
    assert_eq!(
        palw_improve_scoring_set_id_v1().as_byte_slice(),
        &keyed(PALW_IMPROVE_SCORING_SET_DOMAIN_V1, &misaka_palw_tir::scoring::scoring_set_descriptor_v1())[..]
    );
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/scoring/set.json");
    let set: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the scoring set's vector")).expect("json");
    assert_eq!(set["scoring_set_id_hex"].as_str().unwrap(), palw_improve_scoring_set_id_v1().to_string());
    // The sign table's id is the pinned digest of the whole table (spec 17 §17.9.2); the promotion
    // module's `the_pinned_sign_table_is_the_definition` recomputes it.
    let pinned = kaspa_consensus_core::palw_improve_promotion_v1::PALW_IMPROVE_SIGN_TABLE_ID_HEX_V1;
    assert_eq!(palw_improve_sign_table_id_v1().to_string(), pinned, "the fence reads the pin");
    assert_ne!(palw_improve_scoring_set_id_v1(), palw_improve_sign_table_id_v1());
    let fence = PalwImprovementFenceV1::drill_v1(ForkActivation::new(AT));
    assert_eq!(fence.court_version, PALW_IMPROVE_COURT_VERSION_V1);
    assert_eq!(fence.ceilings, PALW_DRILL_IMPROVE_CEILINGS_V1);
    assert!(PALW_DRILL_IMPROVE_CEILINGS_V1.within_format_caps().is_ok());
    assert!(PalwImprovementCeilingsV1::FORMAT_CAPS_V1.within_format_caps().is_ok());
}

#[test]
fn the_drill_entry_sets_the_field_it_names() {
    let mut p = t12_with_gen();
    (PALW_DRILL_IMPROVE_V1_ENTRY.set)(&mut p, Some(ForkActivation::new(AT)));
    assert_eq!(
        p.palw_improvement_v1,
        Some(PalwImprovementFenceV1::this_build_v1(ForkActivation::new(AT), PALW_DRILL_IMPROVE_CEILINGS_V1))
    );
    assert!(p.palw_fences_v1().iter().any(|(name, _)| *name == PALW_DRILL_IMPROVE_V1_ENTRY.name), "the entry's name is a fence's");
    assert!(p.validate_palw_improvement_v1().is_ok(), "the entry mirrors what it sets");
    (PALW_DRILL_IMPROVE_V1_ENTRY.set)(&mut p, None);
    assert_eq!(ids(&p), ids(&t12_with_gen()), "setting it back is the release, byte for byte");
}

#[test]
fn the_drill_mover_arms_it_on_a_salted_drill_and_nowhere_else() {
    let mut drill = palw_t12_drill_params_v1(&salt());
    kaspa_consensus_core::config::drill::palw_drill_tir_fence2_at_v1(&mut drill, 2_300).expect("the second IR fence first");
    palw_drill_gen_fence_at_v1(&mut drill, 2_345).expect("the generative fence, above the drill's IR flag day");
    // Without the decode rules the improvement fence is refused, naming them; the rules arm below it.
    let mut without_decode = drill.clone();
    let why = palw_drill_improve_fence_at_v1(&mut without_decode, 2_400).unwrap_err();
    assert!(why.contains("does not validate") && why.contains("palw_fp_decode_rules"), "{why}");
    assert_eq!(ids(&without_decode), ids(&drill), "a refusal leaves the ruleset as it came");
    let decode_moves = palw_drill_decode_rules_at_v1(&mut drill, 2_360).expect("the decode rules, above the generative fence");
    assert_eq!(decode_moves.len(), 1);
    assert_eq!((decode_moves[0].name, decode_moves[0].was, decode_moves[0].at), ("palw_fp_decode_rules", None, 2_360));
    assert!(decode_moves[0].to_string().contains("ARMED"), "{}", decode_moves[0]);
    assert!(drill.palw_fp_decode_rules_active_at(2_360) && !drill.palw_fp_decode_rules_active_at(2_359));
    // Above the generative fence: armed, the one fence, and the result validates.
    let mut moved = drill.clone();
    let moves = palw_drill_improve_fence_at_v1(&mut moved, 2_400).expect("a salted drill arms it above palw_gen_v1");
    assert_eq!(moves.len(), 1);
    assert_eq!((moves[0].name, moves[0].was, moves[0].at), ("palw_improvement_v1", None, 2_400));
    assert!(moves[0].to_string().contains("ARMED"), "{}", moves[0]);
    assert!(moved.palw_improvement_v1_active_at(2_400) && !moved.palw_improvement_v1_active_at(2_399));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &moved.palw_consensus_mode else { panic!("V2") };
    assert_eq!(bundle.state.improve_from_daa(), Some(2_400), "the fold's mirror");
    for ((name, was), (_, now)) in drill.palw_fences_v1().iter().zip(moved.palw_fences_v1().iter()) {
        if *name != "palw_improvement_v1" {
            assert_eq!(was, now, "{name} did not move");
        }
    }
    assert_ne!(ids(&moved).0, ids(&drill).0, "the drill chain's params id moves");
    assert_eq!(ids(&moved).1, ids(&drill).1, "a future height: the identity is the drill's");
    // Refusals leave the ruleset as it came.
    for (at, why) in [
        (2_330, "below the generative fence"),
        (2_355, "below the decode rules"),
        (2_300, "palw_tir_fence2's own height"),
        (2_345, "palw_gen_v1's own height"),
        (2_360, "palw_fp_decode_rules's own height"),
        (2_000, "palw_tir_v1's own height"),
        (0, "genesis"),
        (u64::MAX, "never"),
    ] {
        let mut p = drill.clone();
        assert!(palw_drill_improve_fence_at_v1(&mut p, at).is_err(), "{why}");
        assert_eq!(ids(&p), ids(&drill), "{why}: untouched");
    }
    let mut without_gen = palw_t12_drill_params_v1(&salt());
    assert!(
        palw_drill_improve_fence_at_v1(&mut without_gen, 2_400).unwrap_err().contains("does not validate"),
        "without the generative fence the result does not validate"
    );
    let mut public = palw_t12_shipped_params();
    assert!(palw_drill_improve_fence_at_v1(&mut public, 2_400).is_err(), "public testnet-12's genesis: never");
    let mut mainnet = mainnet_shipped_params();
    assert!(palw_drill_improve_fence_at_v1(&mut mainnet, 2_400).is_err(), "another network: never");
}

/// **The decode rules' own flag-day entry and drill mover**: dormant on every network, armed alone on a salted
/// drill chain (the prerequisite RFC-0003's FP Job V5 and this fence name), refused where the other movers are.
#[test]
fn the_decode_rules_have_their_own_dormant_list_and_drill_mover() {
    use kaspa_consensus_core::config::params::{PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1, PALW_T12_DECODE_RULES_DAA};
    assert_eq!(PALW_T12_DECODE_RULES_DAA, None, "dormant until the user sets a height");
    assert_eq!(PALW_T12_DECODE_RULES_FENCES_V1.len(), 1);
    assert_eq!(PALW_T12_DECODE_RULES_FENCES_V1[0].name, PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1.name);
    // Every shipped ruleset leaves the decode rules where it had them: dormant (the list's height is `None`).
    for (name, p) in rulesets() {
        assert!(p.palw_fp_decode_rules.is_none(), "{name}: dormant");
    }
    // A salted drill arms them alone.
    let drill = palw_t12_drill_params_v1(&salt());
    let mut moved = drill.clone();
    let moves = palw_drill_decode_rules_at_v1(&mut moved, 2_360).expect("a salted drill arms the decode rules");
    assert_eq!(moves.len(), 1);
    assert_eq!((moves[0].name, moves[0].was, moves[0].at), ("palw_fp_decode_rules", None, 2_360));
    assert!(moved.palw_fp_decode_rules_active_at(2_360) && !moved.palw_fp_decode_rules_active_at(2_359));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &moved.palw_consensus_mode else { panic!("V2") };
    assert_eq!(bundle.state.fp_decode_rules_from_daa(), Some(2_360), "the bundle's mirror follows");
    for ((name, was), (_, now)) in drill.palw_fences_v1().iter().zip(moved.palw_fences_v1().iter()) {
        if *name != "palw_fp_decode_rules" {
            assert_eq!(was, now, "{name} did not move");
        }
    }
    assert_ne!(ids(&moved).0, ids(&drill).0, "the drill chain's params id moves");
    assert_eq!(ids(&moved).1, ids(&drill).1, "a future height: the identity is the drill's");
    // Refusals leave the ruleset as it came.
    for (at, why) in [(2_000, "palw_tir_v1's own height"), (0, "genesis"), (u64::MAX, "never")] {
        let mut p = drill.clone();
        assert!(palw_drill_decode_rules_at_v1(&mut p, at).is_err(), "{why}");
        assert_eq!(ids(&p), ids(&drill), "{why}: untouched");
    }
    let mut public = palw_t12_shipped_params();
    assert!(palw_drill_decode_rules_at_v1(&mut public, 2_360).is_err(), "public testnet-12's genesis: never");
    let mut mainnet = mainnet_shipped_params();
    assert!(palw_drill_decode_rules_at_v1(&mut mainnet, 2_360).is_err(), "another network: never");
}

fn h(byte: u8) -> Hash64 {
    Hash64::from_bytes([byte; 64])
}

fn bond(byte: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: h(byte), index: 0 })
}

fn policy() -> PalwImprovementPolicyV1 {
    kaspa_consensus_core::palw_improve_policy_v1::palw_improvement_policy_example_v1()
}

/// One of each improvement object, in tag order.
fn improvement_objects() -> Vec<PalwConsensusObjectV2> {
    let sig = vec![0xAA; 4];
    vec![
        PalwConsensusObjectV2::ModelLineImprovementPolicySet {
            payload: Box::new(PalwImprovementPolicySetV1 { line_id: h(10), sequence: 1, policy: Some(policy()) }),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::HardCaseSubmitted {
            payload: Box::new(PalwHardCaseV1 {
                line_id: h(10),
                case_id: h(11),
                domain: 1,
                prompt_ids: vec![1, 2, 3],
                reference: PalwCaseReferenceV1::ExactKey { commitment: h(12) },
                source: PalwCaseSourceV1::Setter,
                head_evidence: None,
            }),
            submitter: bond(1),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::DataUseOptIn {
            payload: Box::new(PalwDataUseOptInV1 {
                job_pin: h(13),
                claim: h(34),
                job: PalwFpJobFactsV1 {
                    job_id: h(35),
                    execution_seed: [7; 32],
                    tokenizer_id: h(36),
                    prompt_token_ids_hash: h(37),
                    prompt_tokens: 3,
                    decode_tokens_executed: 5,
                    max_context_tokens: 64,
                },
            }),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::SetterSetCommitted {
            payload: Box::new(PalwSetterSetCommitmentV1 {
                line_id: h(10),
                epoch: 1,
                set_id: h(14),
                items: 8,
                prompts_commitment: h(15),
                keys_commitment: h(16),
            }),
            setter: bond(2),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::SetterSetRevealed {
            payload: Box::new(PalwSetterSetRevealV1 { line_id: h(10), epoch: 1, set_id: h(14), prompts: vec![vec![5]], salt: h(17) }),
        },
        PalwConsensusObjectV2::SetterKeysRevealed {
            payload: Box::new(PalwSetterKeysRevealV1 { line_id: h(10), epoch: 1, set_id: h(14), keys: vec![vec![6]], salt: h(18) }),
        },
        PalwConsensusObjectV2::DatasetRegistered {
            payload: Box::new(PalwDatasetV1 {
                line_id: h(10),
                dataset_id: h(19),
                content_root: h(20),
                items: 100,
                license_classes: vec![h(21)],
                teacher_classes: PalwTeacherClassV1::PublicData.bit(),
                provenance_commitment: h(22),
            }),
            registrant: bond(3),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::TeachingArtifactCommitted {
            payload: Box::new(PalwTeachingArtifactCommitV1 { line_id: h(10), commit: h(23) }),
            teacher: bond(4),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::TeachingArtifactRevealed {
            payload: Box::new(PalwTeachingArtifactV1 {
                line_id: h(10),
                kind: PalwTeachingArtifactKindV1::Answer,
                task_id: h(11),
                teacher_type: PalwTeacherClassV1::OpenDistill,
                teacher_id: h(24),
                license_class: h(21),
                provenance_commitment: h(25),
                output_hash: h(26),
                verification_type: PalwVerificationTypeV1::Exact,
                answer_span: vec![7, 8],
                salt: h(27),
            }),
        },
        PalwConsensusObjectV2::TeacherLicenceRegistered {
            payload: Box::new(PalwTeacherLicenceV1 {
                licence_id: h(28),
                rights_holder_key: vec![1, 2, 3],
                model_family: h(29),
                domains: vec![1],
                uses: 1,
                per_use_fee: 10,
                expiry_daa: 1_000_000,
            }),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::CandidateSubmitted {
            payload: Box::new(PalwCandidateSubmissionV1 {
                line_id: h(10),
                epoch: 1,
                class_id: h(30),
                artifact: PalwTirArtifactRefV1::Composite { parent_class: h(31), parent_root: h(32), adapter_root: h(33), p: 290 },
                layout: PalwTirLayoutV1 {
                    version: PALW_TIR_LAYOUT_VERSION_V1,
                    max_context: 64,
                    checkpoint_interval: 8,
                    h_tile: 8,
                    commit_tiles: vec![16],
                    state_tiles: vec![16],
                },
                declarations: PalwCandidateDeclarationsV1 {
                    datasets: vec![(h(19), 1_000)],
                    licences: Vec::new(),
                    teacher_classes: PalwTeacherClassV1::PublicData.bit(),
                },
            }),
            submitter: bond(5),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::LineageHeadRolledBack {
            payload: Box::new(PalwLineageRollbackV1 { line_id: h(10), epoch: 1, to_class: h(31), cause: PalwRollbackCauseV1::Owner }),
            filer: bond(6),
            signature: sig,
        },
        PalwConsensusObjectV2::ImprovementPoolFunded {
            payload: Box::new(PalwImprovementPoolFundingV1 { line_id: h(10), amount: 1_000, sink_index: 1 }),
        },
    ]
}

/// **Tags 70–82, appended**: each object's borsh discriminant is its reserved tag, the name the
/// acceptance walk and the fold print carries the same tag, every one round-trips, and the one
/// predicate names all thirteen and nothing of the IR, second-IR-fence or generative sets.
#[test]
fn the_improvement_objects_are_tags_70_to_82_and_one_predicate_names_them() {
    let objects = improvement_objects();
    assert_eq!(objects.len(), 13);
    for (i, object) in objects.iter().enumerate() {
        let tag = 70 + i as u8;
        let bytes = borsh::to_vec(object).expect("an object serializes");
        assert_eq!(bytes[0], tag, "{object:?}");
        let name = palw_improvement_object_name_v1(object).expect("named");
        assert!(name.ends_with(&format!("(tag {tag})")), "{name} names tag {tag}");
        assert!(palw_object_is_improvement_v1(object), "{name}");
        assert!(
            !palw_object_is_tir_v1(object) && !palw_object_is_tir_fence2_v1(object) && !palw_object_is_gen_v1(object),
            "{name} belongs to no other fence"
        );
        let back: PalwConsensusObjectV2 = borsh::from_slice(&bytes).expect("an object decodes");
        assert_eq!(&back, object, "{name} round-trips");
        // The stateless gate lets every one ride (A-2): a block carrying one is valid on this build
        // and on an older one that skips it undecoded.
        assert!(kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(object).is_ok(), "{name} rides");
        // None is an H-1 carrier: nothing here is a conviction or a court move a halt must let through.
        assert!(!kaspa_consensus_core::palw_heartbeat_carriers_v1::palw_h1_carrier_object_v1(object), "{name}");
    }
    let names: std::collections::BTreeSet<_> = objects.iter().filter_map(palw_improvement_object_name_v1).collect();
    assert_eq!(names.len(), 13, "thirteen distinct names");
    let other = PalwConsensusObjectV2::ModelLineRetired { line_id: h(10), signature: vec![1] };
    assert!(!palw_object_is_improvement_v1(&other) && palw_improvement_object_name_v1(&other).is_none());
    // Tag 83 is free: nothing decodes it.
    let mut bytes = borsh::to_vec(objects.last().unwrap()).unwrap();
    bytes[0] = 83;
    assert!(borsh::from_slice::<PalwConsensusObjectV2>(&bytes).is_err(), "tag 83 is unassigned");
}
