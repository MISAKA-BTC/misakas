//! **RFC-0007 Part I: the `palw_verification_vertex_v1` fence** — dormant on every ruleset a node can run (testnet-12 included: it
//! joins no flag-day list), fingerprinted with its value where armed (Some-only, so the identity of every dormant ruleset is the
//! one it had before the field existed), invisible to the identity until it fires, named by the fork id, armed by the drill mover
//! alone, and refused by `validate_palw_v2` without each prerequisite in force at or below it — **by name** — or with its mirror
//! unsynced.
//!
//! This file adds no id pin (a quoted id here would be one `t12-repin.sh` does not know): it checks the field.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_verification_vertex_fence`

use kaspa_consensus_core::config::drill::{PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1, palw_drill_vertex_at_v1};
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_CAPACITY_RHO10_FENCES_V1,
    PALW_T12_INT11_FENCES_V1, PALW_T12_INT11_RHO100_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2,
    PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_FENCES_V1, PALW_T12_TIR_FLAG_DAY_FENCES_V1, Params, SIMNET_PARAMS,
    TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_arm_int11_flag_day_at_v1, palw_t12_drill_params_v1, palw_t12_launch_params_v1, palw_t12_release_v1_params,
    palw_t12_release_v2_params, palw_t12_release_v3_params, palw_t12_release_v5_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_vertex_v1::{PALW_DRILL_VERTEX_FENCES_V1, PALW_VERTEX_ENTRY_V1};

/// A height no testnet-12 fence uses, above every one it schedules (the int-11 flag day is 5,300).
const AT: u64 = 6_543;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x3d; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.vertex_from_daa(),
        _ => None,
    }
}

/// A salted drill ruleset with the int-11 list back to dormant (what the drill mover's own tests move from).
fn dormant_drill() -> Params {
    let mut drill = palw_t12_drill_params_v1(&salt());
    palw_t12_arm_int11_flag_day_at_v1(&mut drill, None);
    drill
}

/// Every ruleset a node can run, as it ships: the fence is dormant on all of them (testnet-12's releases and drills included).
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
        ("palw_t12_release_v5_params", palw_t12_release_v5_params()),
        ("palw_t12_launch_params_v1", palw_t12_launch_params_v1()),
        ("palw_t12_release_v1_params", palw_t12_release_v1_params()),
        ("palw_t12_release_v2_params", palw_t12_release_v2_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
        ("palw_t12_drill_params_v1", palw_t12_drill_params_v1(&salt())),
        ("palw_t12_drill_params_v1 − int-11", dormant_drill()),
    ]
}

fn armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_release_v5_params();
    (PALW_VERTEX_ENTRY_V1.set)(&mut p, Some(at));
    p
}

#[test]
fn every_ruleset_leaves_it_dormant_and_no_flag_day_list_names_it() {
    for (name, p) in rulesets() {
        assert!(p.palw_verification_vertex_v1.is_none(), "{name}: dormant");
        assert!(
            p.palw_verification_vertex_fence().is_none() && !p.palw_verification_vertex_active_at(u64::MAX),
            "{name}: the accessor agrees"
        );
        assert_eq!(mirror(&p), None, "{name}: no mirror");
        assert!(p.palw_fences_v1().contains(&("palw_verification_vertex_v1", None)), "{name}: the exhaustive fence list names it");
        assert!(p.validate_palw_verification_vertex_v1().is_ok(), "{name}: nothing to refuse");
        p.validate_palw_v2().unwrap_or_else(|e| {
            if matches!(p.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                panic!("{name}: {e}")
            }
        });
    }
    // In NO testnet-12 flag-day list: it ships dormant and arms with a later flag day the lead decides.
    for list in [
        PALW_T12_POST_LAUNCH_FENCES_V1,
        PALW_T12_POST_LAUNCH_FENCES_V2,
        PALW_T12_POST_LAUNCH_FENCES_V3,
        PALW_T12_CAPACITY_FENCES_V1,
        PALW_T12_CAPACITY_RHO10_FENCES_V1,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1,
        PALW_T12_TIR_FENCE2_FENCES_V1,
        PALW_T12_INT11_FENCES_V1,
        PALW_T12_INT11_RHO100_FENCES_V1,
    ] {
        assert!(list.iter().all(|f| f.name != "palw_verification_vertex_v1"), "no testnet-12 release arms the verification vertex");
    }
    let names: Vec<&str> = PALW_DRILL_VERTEX_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(names, ["palw_verification_vertex_v1"]);
}

#[test]
fn a_scheduled_fence_moves_the_ruleset_and_the_schedule_and_never_the_identity_and_the_fork_id_gates_it() {
    let shipped = palw_t12_release_v5_params();
    let base = ids(&shipped);
    let p = armed(ForkActivation::new(AT));
    p.validate_palw_v2().unwrap_or_else(|e| panic!("testnet-12 past its int-11 flag day can arm it: {e}"));
    assert_eq!(mirror(&p), Some(AT), "the fold's mirror");
    let moved = ids(&p);
    assert_ne!(moved.0, base.0, "the ruleset a node announces names the fence and its value");
    assert_eq!(moved.1, base.1, "two builds that differ only about a FUTURE height stay peers");
    assert_ne!(moved.2, base.2, "the schedule reports it");
    assert!(p.palw_verification_vertex_active_at(AT) && !p.palw_verification_vertex_active_at(AT - 1));
    assert!(fork_id_gate_fences_v1(&p).contains(&AT), "past its height an un-upgraded node is refused");
    let old = fork_id_v1(&shipped, AT);
    assert!(evaluate_fork_id_v1(&p, AT, old.fired.as_bytes().as_slice(), old.next).refuses(), "gated from its height");
    let below = fork_id_v1(&shipped, AT - 1);
    assert!(!evaluate_fork_id_v1(&p, AT - 1, below.fired.as_bytes().as_slice(), below.next).refuses(), "kept below it");
    // Another height is another ruleset.
    assert_ne!(ids(&armed(ForkActivation::new(AT + 1))).0, moved.0, "the height is part of the value");
    // Set back: the shipped ruleset, to the id.
    let mut back = p.clone();
    (PALW_VERTEX_ENTRY_V1.set)(&mut back, None);
    assert_eq!(ids(&back), base, "set(None) is the shipped ruleset");
    assert_eq!(mirror(&back), None);
}

#[test]
fn never_is_absence_for_the_identity() {
    let base = palw_t12_release_v5_params();
    let never = armed(ForkActivation::never());
    assert_eq!(never.consensus_identity_id(), base.consensus_identity_id(), "Some(never()) collapses whole in the normaliser");
    never.validate_palw_v2().unwrap_or_else(|e| panic!("a dormant value validates: {e}"));
    assert!(!never.palw_verification_vertex_active_at(u64::MAX - 1));
    assert_eq!(mirror(&never), None, "a dormant value has no mirror");
}

/// **Each prerequisite, by name**: the fence refuses to arm below any of the five fences the tally leans on.
#[test]
fn it_is_refused_below_each_prerequisite_by_name_and_with_its_mirror_unsynced() {
    for (name, strip) in [
        ("palw_verification_v2", (|p: &mut Params| p.palw_verification_v2 = None) as fn(&mut Params)),
        ("palw_rcore_plus", |p| p.palw_rcore_plus = None),
        ("palw_unavailable_abstains", |p| p.palw_unavailable_abstains = None),
        ("palw_panel_economy", |p| p.palw_panel_economy = None),
        ("palw_objective_offence", |p| p.palw_objective_offence = None),
    ] {
        let mut p = armed(ForkActivation::new(AT));
        strip(&mut p);
        let why = p.validate_palw_verification_vertex_v1().expect_err("a prerequisite missing");
        assert!(format!("{why:?}").contains(name), "{name}: {why:?}");
        // …and in force only AFTER the fence is as good as absent.
        let mut late = armed(ForkActivation::new(AT));
        match name {
            "palw_verification_v2" => late.palw_verification_v2 = Some(ForkActivation::new(AT + 1)),
            "palw_rcore_plus" => late.palw_rcore_plus = Some(ForkActivation::new(AT + 1)),
            "palw_unavailable_abstains" => late.palw_unavailable_abstains = Some(ForkActivation::new(AT + 1)),
            "palw_panel_economy" => late.palw_panel_economy = Some(ForkActivation::new(AT + 1)),
            _ => late.palw_objective_offence = Some(ForkActivation::new(AT + 1)),
        }
        let why = late.validate_palw_verification_vertex_v1().expect_err("a prerequisite that arms after it");
        assert!(format!("{why:?}").contains(name), "{name} late: {why:?}");
    }
    // The mirror: a fence with no mirror and a mirror with no fence.
    let mut unsynced = palw_t12_release_v5_params();
    unsynced.palw_verification_vertex_v1 = Some(ForkActivation::new(AT));
    assert!(unsynced.validate_palw_verification_vertex_v1().is_err(), "the mirror disagrees with the fence");
    let mut stale = armed(ForkActivation::new(AT));
    stale.palw_verification_vertex_v1 = None;
    assert!(stale.validate_palw_verification_vertex_v1().is_err(), "a mirror with no fence");
    stale.sync_palw_verification_vertex_v1();
    assert_eq!(ids(&stale), ids(&palw_t12_release_v5_params()), "re-synced it is the release");
    // It is a ConsensusV2 rule.
    let mut not_v2 = SIMNET_PARAMS;
    assert!(!matches!(not_v2.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)));
    not_v2.palw_verification_vertex_v1 = Some(ForkActivation::new(AT));
    assert!(not_v2.validate_palw_verification_vertex_v1().is_err(), "a ConsensusV2 rule");
    assert!(not_v2.palw_verification_vertex_fence().is_none(), "and the accessor never answers off V2");
}

#[test]
fn the_drill_entry_sets_the_field_it_names() {
    let mut p = palw_t12_release_v5_params();
    (PALW_VERTEX_ENTRY_V1.set)(&mut p, Some(ForkActivation::new(AT)));
    assert_eq!(p.palw_verification_vertex_v1, Some(ForkActivation::new(AT)));
    assert_eq!(mirror(&p), Some(AT), "the entry's own set writes the mirror");
    assert!(p.palw_fences_v1().iter().any(|(name, _)| *name == PALW_VERTEX_ENTRY_V1.name), "the entry's name is a fence's");
    (PALW_VERTEX_ENTRY_V1.set)(&mut p, None);
    assert_eq!(ids(&p), ids(&palw_t12_release_v5_params()), "setting it back is the release, byte for byte");
}

#[test]
fn the_drill_mover_arms_it_on_a_salted_drill_and_nowhere_else() {
    let drill = dormant_drill();
    // The drill's prerequisites are armed at genesis: the fence stands at a low height, the one fence, and the result validates.
    let mut moved = drill.clone();
    let moves = palw_drill_vertex_at_v1(&mut moved, 140).expect("a salted drill arms it low");
    assert_eq!(moves.len(), 1);
    assert_eq!((moves[0].name, moves[0].was, moves[0].at), ("palw_verification_vertex_v1", None, 140));
    assert!(moved.palw_verification_vertex_active_at(140) && !moved.palw_verification_vertex_active_at(139));
    moved.validate_palw_v2().expect("the result validates");
    for ((name, was), (_, now)) in drill.palw_fences_v1().iter().zip(moved.palw_fences_v1().iter()) {
        if *name != "palw_verification_vertex_v1" {
            assert_eq!(was, now, "{name} did not move");
        }
    }
    assert_eq!(ids(&moved).1, ids(&drill).1, "a future height: the identity is the drill's");
    // Refusals leave the ruleset as it came.
    for (at, why) in [(0, "genesis"), (u64::MAX, "never")] {
        let mut p = drill.clone();
        assert!(palw_drill_vertex_at_v1(&mut p, at).is_err(), "{why}");
        assert_eq!(ids(&p), ids(&drill), "{why}: untouched");
    }
    let mut public = palw_t12_shipped_params();
    assert!(palw_drill_vertex_at_v1(&mut public, AT).is_err(), "public testnet-12's genesis: never");
    let mut mainnet = mainnet_shipped_params();
    assert!(palw_drill_vertex_at_v1(&mut mainnet, AT).is_err(), "another network: never");
}
