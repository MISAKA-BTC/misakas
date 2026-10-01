//! **RFC-0003 decision 22: the `palw_held_close_chunks_v1` fence** — dormant on every ruleset a node can run
//! (every id pinned in `palw_tir_fences_are_dormant.rs` and `palw_tir_flag_day_t12.rs` holds, measured before the
//! field existed), fingerprinted with its value where armed, invisible to the identity until it fires, named by
//! the fork id, armed by the drill mover alone, and refused by `validate_palw_v2` without `palw_tir_v1` and
//! `palw_held_context` in force at or below it or with its mirror unsynced.
//!
//! This file adds no id pin (a quoted id here would be one `t12-repin.sh` does not know): it checks the field.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_held_close_chunks_fence`

use kaspa_consensus_core::config::drill::{
    PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1, palw_drill_held_close_chunks_at_v1, palw_drill_tir_fence_at_v1,
};
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_CAPACITY_RHO10_FENCES_V1,
    PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_FENCES_V1,
    PALW_T12_TIR_FLAG_DAY_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_drill_params_v1, palw_t12_launch_params_v1, palw_t12_release_v1_params,
    palw_t12_release_v2_params, palw_t12_release_v3_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_held_close_v1::{PALW_DRILL_HELD_CLOSE_CHUNKS_FENCES_V1, PALW_HELD_CLOSE_CHUNKS_ENTRY_V1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// A height no testnet-12 fence uses, above its IR flag day (2,000).
const AT: u64 = 2_345;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x3c; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.held_close_chunks_from_daa(),
        _ => None,
    }
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

fn armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_shipped_params();
    (PALW_HELD_CLOSE_CHUNKS_ENTRY_V1.set)(&mut p, Some(at));
    p
}

#[test]
fn every_shipped_ruleset_leaves_it_dormant_and_no_release_lists_it() {
    for (name, p) in rulesets() {
        assert!(p.palw_held_close_chunks_v1.is_none(), "{name}: dormant");
        assert!(
            p.palw_held_close_chunks_fence().is_none() && !p.palw_held_close_chunks_active_at(u64::MAX),
            "{name}: the accessor agrees"
        );
        assert_eq!(mirror(&p), None, "{name}: no mirror");
        assert!(p.palw_fences_v1().contains(&("palw_held_close_chunks_v1", None)), "{name}: the exhaustive fence list names it");
        assert!(p.validate_palw_held_close_chunks_v1().is_ok(), "{name}: nothing to refuse");
    }
    // In NO testnet-12 flag-day list today: the only list naming it is the drill's.
    for list in [
        PALW_T12_POST_LAUNCH_FENCES_V1,
        PALW_T12_POST_LAUNCH_FENCES_V2,
        PALW_T12_POST_LAUNCH_FENCES_V3,
        PALW_T12_CAPACITY_FENCES_V1,
        PALW_T12_CAPACITY_RHO10_FENCES_V1,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1,
        PALW_T12_TIR_FENCE2_FENCES_V1,
    ] {
        assert!(list.iter().all(|f| f.name != "palw_held_close_chunks_v1"), "no testnet-12 release arms the held leaf challenge");
    }
    let names: Vec<&str> = PALW_DRILL_HELD_CLOSE_CHUNKS_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(names, ["palw_held_close_chunks_v1"]);
}

#[test]
fn a_scheduled_fence_moves_the_ruleset_and_the_schedule_and_never_the_identity_and_the_fork_id_gates_it() {
    let shipped = palw_t12_shipped_params();
    let base = ids(&shipped);
    let p = armed(ForkActivation::new(AT));
    p.validate_palw_v2().unwrap_or_else(|e| panic!("testnet-12 past its IR flag day can arm it: {e}"));
    assert_eq!(mirror(&p), Some(AT), "the fold's mirror");
    let moved = ids(&p);
    assert_ne!(moved.0, base.0, "the ruleset a node announces names the fence and its value");
    assert_eq!(moved.1, base.1, "two builds that differ only about a FUTURE height stay peers");
    assert_ne!(moved.2, base.2, "the schedule reports it");
    assert!(p.palw_held_close_chunks_active_at(AT) && !p.palw_held_close_chunks_active_at(AT - 1));
    assert!(fork_id_gate_fences_v1(&p).contains(&AT), "past its height an un-upgraded node is refused");
    let old = fork_id_v1(&shipped, AT);
    assert!(evaluate_fork_id_v1(&p, AT, old.fired.as_bytes().as_slice(), old.next).refuses(), "gated from its height");
    let below = fork_id_v1(&shipped, AT - 1);
    assert!(!evaluate_fork_id_v1(&p, AT - 1, below.fired.as_bytes().as_slice(), below.next).refuses(), "kept below it");
    // Another height is another ruleset.
    assert_ne!(ids(&armed(ForkActivation::new(AT + 1))).0, moved.0, "the height is part of the value");
    // Set back: the shipped ruleset, to the id.
    let mut back = p.clone();
    (PALW_HELD_CLOSE_CHUNKS_ENTRY_V1.set)(&mut back, None);
    assert_eq!(ids(&back), base, "set(None) is the shipped ruleset");
    assert_eq!(mirror(&back), None);
}

#[test]
fn never_is_absence_for_the_identity_and_genesis_is_a_rule() {
    let base = palw_t12_shipped_params();
    let never = armed(ForkActivation::never());
    assert_eq!(never.consensus_identity_id(), base.consensus_identity_id(), "Some(never()) collapses whole in the normaliser");
    never.validate_palw_v2().unwrap_or_else(|e| panic!("a dormant value validates: {e}"));
    assert!(!never.palw_held_close_chunks_active_at(u64::MAX - 1));
    assert_eq!(mirror(&never), None, "a dormant value has no mirror");
}

#[test]
fn it_is_refused_below_its_prerequisites_and_with_its_mirror_unsynced() {
    // Below palw_tir_v1 (the shipped IR flag day is 2,000): the objects it serves are dropped by name there.
    let e = armed(ForkActivation::new(1_999)).validate_palw_v2().expect_err("below palw_tir_v1");
    assert!(format!("{e:?}").contains("palw_tir_v1"), "{e:?}");
    armed(ForkActivation::new(2_000)).validate_palw_v2().expect("at palw_tir_v1's own height");
    // Without palw_held_context in force at or below it (the held regime is the object's whole place).
    let mut no_held = armed(ForkActivation::new(AT));
    no_held.palw_held_context = None;
    assert!(no_held.validate_palw_held_close_chunks_v1().is_err(), "no held regime");
    let mut late_held = armed(ForkActivation::new(AT));
    late_held.palw_held_context = Some(ForkActivation::new(AT + 1));
    assert!(late_held.validate_palw_held_close_chunks_v1().is_err(), "the held regime arms after it");
    // Without palw_tir_v1 at all, or armed later than it.
    let mut no_tir = armed(ForkActivation::new(AT));
    no_tir.palw_tir_v1 = None;
    no_tir.sync_palw_tir_v1();
    assert!(no_tir.validate_palw_held_close_chunks_v1().is_err(), "no IR fence");
    // The mirror: a fence with no mirror and a mirror with no fence.
    let mut unsynced = palw_t12_shipped_params();
    unsynced.palw_held_close_chunks_v1 = Some(ForkActivation::new(AT));
    assert!(unsynced.validate_palw_held_close_chunks_v1().is_err(), "the mirror disagrees with the fence");
    let mut stale = armed(ForkActivation::new(AT));
    stale.palw_held_close_chunks_v1 = None;
    assert!(stale.validate_palw_held_close_chunks_v1().is_err(), "a mirror with no fence");
    stale.sync_palw_held_close_chunks_v1();
    assert_eq!(ids(&stale), ids(&palw_t12_shipped_params()), "re-synced it is the shipped ruleset");
    // It is a ConsensusV2 rule.
    let mut not_v2 = SIMNET_PARAMS;
    assert!(!matches!(not_v2.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)));
    not_v2.palw_held_close_chunks_v1 = Some(ForkActivation::new(AT));
    assert!(not_v2.validate_palw_held_close_chunks_v1().is_err(), "a ConsensusV2 rule");
    assert!(not_v2.palw_held_close_chunks_fence().is_none(), "and the accessor never answers off V2");
}

#[test]
fn the_drill_entry_sets_the_field_it_names() {
    let mut p = palw_t12_shipped_params();
    (PALW_HELD_CLOSE_CHUNKS_ENTRY_V1.set)(&mut p, Some(ForkActivation::new(AT)));
    assert_eq!(p.palw_held_close_chunks_v1, Some(ForkActivation::new(AT)));
    assert_eq!(mirror(&p), Some(AT), "the entry's own set writes the mirror");
    assert!(p.palw_fences_v1().iter().any(|(name, _)| *name == PALW_HELD_CLOSE_CHUNKS_ENTRY_V1.name), "the entry's name is a fence's");
    (PALW_HELD_CLOSE_CHUNKS_ENTRY_V1.set)(&mut p, None);
    assert_eq!(ids(&p), ids(&palw_t12_shipped_params()), "setting it back is the release, byte for byte");
}

#[test]
fn the_drill_mover_arms_it_on_a_salted_drill_and_nowhere_else() {
    let drill = palw_t12_drill_params_v1(&salt());
    // Above the drill's IR flag day (2,000): armed, the one fence, and the result validates.
    let mut moved = drill.clone();
    let moves = palw_drill_held_close_chunks_at_v1(&mut moved, AT).expect("a salted drill arms it above palw_tir_v1");
    assert_eq!(moves.len(), 1);
    assert_eq!((moves[0].name, moves[0].was, moves[0].at), ("palw_held_close_chunks_v1", None, AT));
    assert!(moved.palw_held_close_chunks_active_at(AT) && !moved.palw_held_close_chunks_active_at(AT - 1));
    moved.validate_palw_v2().expect("the result validates");
    for ((name, was), (_, now)) in drill.palw_fences_v1().iter().zip(moved.palw_fences_v1().iter()) {
        if *name != "palw_held_close_chunks_v1" {
            assert_eq!(was, now, "{name} did not move");
        }
    }
    assert_eq!(ids(&moved).1, ids(&drill).1, "a future height: the identity is the drill's");
    // The prerequisite lowered by the drill's own flag, the fence may stand beside it.
    let mut low = drill.clone();
    palw_drill_tir_fence_at_v1(&mut low, 300).expect("a salted drill crosses the IR flag day low");
    palw_drill_held_close_chunks_at_v1(&mut low, 301).expect("and arms the held leaf challenge just above it");
    low.validate_palw_v2().expect("both validate");
    let mut same = drill.clone();
    palw_drill_tir_fence_at_v1(&mut same, 300).expect("the IR flag day low");
    assert!(
        palw_drill_held_close_chunks_at_v1(&mut same, 300).is_err(),
        "the fork id names heights, not fences: another fence's height is refused"
    );
    // Refusals leave the ruleset as it came.
    for (at, why) in [(0, "genesis"), (u64::MAX, "never")] {
        let mut p = drill.clone();
        assert!(palw_drill_held_close_chunks_at_v1(&mut p, at).is_err(), "{why}");
        assert_eq!(ids(&p), ids(&drill), "{why}: untouched");
    }
    let mut public = palw_t12_shipped_params();
    assert!(palw_drill_held_close_chunks_at_v1(&mut public, AT).is_err(), "public testnet-12's genesis: never");
    let mut mainnet = mainnet_shipped_params();
    assert!(palw_drill_held_close_chunks_at_v1(&mut mainnet, AT).is_err(), "another network: never");
}
