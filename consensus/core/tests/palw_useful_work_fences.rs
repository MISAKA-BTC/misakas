//! **ADR-0165, the Useful Work Transition's two fences** — `palw_floor_reserve_v1` (the base floor is a
//! reserve) and `palw_real_clock_tick_v1` (an attempt-lane block carries the slot's clock tick) — dormant on every
//! ruleset a node can run, fingerprinted where armed, invisible to the identity until they fire, named by the
//! fork id, refused without their prerequisites or with the mirror unsynced, and armed by their entries alone.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_useful_work_fences`

use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_real_share_v1::{PALW_T12_USEFUL_WORK_FENCES_V1, PalwClockMergesetFactsV1};

/// A height no testnet-12 fence uses.
const AT: u64 = 7_777;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_shipped_params();
    for f in PALW_T12_USEFUL_WORK_FENCES_V1 {
        (f.set)(&mut p, Some(at));
    }
    p
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.floor_reserve_from_daa(),
        _ => None,
    }
}

#[test]
fn dormant_on_every_ruleset_and_listed_as_a_pair() {
    let names: Vec<&str> = PALW_T12_USEFUL_WORK_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(names, ["palw_floor_reserve_v1", "palw_real_clock_tick_v1"]);
    for (name, p) in [
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET_PARAMS", TESTNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("DEVNET_PARAMS", DEVNET_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
    ] {
        assert_eq!(p.palw_floor_reserve_v1, None, "{name}");
        assert_eq!(p.palw_real_clock_tick_v1, None, "{name}");
        assert!(!p.palw_floor_reserve_active_at(u64::MAX - 1) && !p.palw_real_clock_tick_active_at(u64::MAX - 1), "{name}");
        assert_eq!(mirror(&p), None, "{name}: no mirror");
        let fences = p.palw_fences_v1();
        assert!(fences.contains(&("palw_floor_reserve_v1", None)), "{name}: the exhaustive list names the reserve");
        assert!(fences.contains(&("palw_real_clock_tick_v1", None)), "{name}: and the tick");
    }
}

#[test]
fn armed_they_move_the_ruleset_and_the_schedule_never_the_identity_and_the_fork_id_gates_them() {
    let shipped = palw_t12_shipped_params();
    let p = armed(ForkActivation::new(AT));
    p.validate_palw_v2().expect("testnet-12 with the Useful Work Transition armed validates (every prerequisite is a genesis rule)");
    assert_eq!(mirror(&p), Some(AT), "the fold's mirror");
    let (a, b) = (ids(&p), ids(&shipped));
    assert_ne!(a.0, b.0, "the params id names them");
    assert_ne!(a.2, b.2, "and the schedule");
    assert_eq!(a.1, b.1, "two builds that differ only about a future height share an identity");
    assert!(p.palw_floor_reserve_active_at(AT) && !p.palw_floor_reserve_active_at(AT - 1));
    assert!(p.palw_real_clock_tick_active_at(AT) && !p.palw_real_clock_tick_active_at(AT - 1));
    assert!(fork_id_gate_fences_v1(&p).contains(&AT));
    let old = fork_id_v1(&shipped, AT);
    assert!(evaluate_fork_id_v1(&p, AT, old.fired.as_bytes().as_slice(), old.next).refuses(), "gated from its height");
    let below = fork_id_v1(&shipped, AT - 1);
    assert!(!evaluate_fork_id_v1(&p, AT - 1, below.fired.as_bytes().as_slice(), below.next).refuses(), "kept below it");
    // Each fence is its own identity input: arming one alone differs from arming the other alone and from both.
    let mut only_tick = palw_t12_shipped_params();
    (PALW_T12_USEFUL_WORK_FENCES_V1[1].set)(&mut only_tick, Some(ForkActivation::new(AT)));
    let mut only_reserve = palw_t12_shipped_params();
    (PALW_T12_USEFUL_WORK_FENCES_V1[0].set)(&mut only_reserve, Some(ForkActivation::new(AT)));
    only_tick.validate_palw_v2().expect("the tick alone");
    only_reserve.validate_palw_v2().expect("the reserve alone");
    assert!(ids(&only_tick).0 != ids(&only_reserve).0 && ids(&only_tick).0 != a.0 && ids(&only_reserve).0 != a.0);
    // Set back: the shipped ruleset, to the id; `never()` collapses out of the identity.
    let mut back = p.clone();
    for f in PALW_T12_USEFUL_WORK_FENCES_V1 {
        (f.set)(&mut back, None);
    }
    assert_eq!(ids(&back), b, "set(None) is the shipped ruleset");
    let never = armed(ForkActivation::never());
    never.validate_palw_v2().expect("dormant");
    assert_eq!(ids(&never), b);
    assert_eq!(mirror(&never), None);
}

#[test]
fn they_are_refused_without_their_prerequisites_and_with_the_mirror_unsynced() {
    let mut unsynced = palw_t12_shipped_params();
    unsynced.palw_floor_reserve_v1 = Some(ForkActivation::new(AT));
    assert!(unsynced.validate_palw_useful_work_v1().is_err(), "the mirror disagrees with the fence");
    let mut stale = armed(ForkActivation::new(AT));
    stale.palw_floor_reserve_v1 = None;
    assert!(stale.validate_palw_useful_work_v1().is_err(), "a mirror with no fence");
    stale.sync_palw_floor_reserve_v1();
    assert_eq!(ids(&stale).0, {
        let mut only_tick = palw_t12_shipped_params();
        (PALW_T12_USEFUL_WORK_FENCES_V1[1].set)(&mut only_tick, Some(ForkActivation::new(AT)));
        ids(&only_tick).0
    });
    // The tick needs the cursor, the floor, the lead cap, the single lottery and the anchor clock below it.
    for (what, strip) in [
        ("palw_clock_cursor", (|p: &mut Params| p.palw_clock_cursor = None) as fn(&mut Params)),
        ("palw_clock_floor", |p| p.palw_clock_floor = None),
        ("palw_clock_lead_cap", |p| p.palw_clock_lead_cap = None),
        ("palw_single_lottery", |p| p.palw_single_lottery = None),
        ("palw_anchor_clock", |p| p.palw_anchor_clock = None),
    ] {
        let mut p = palw_t12_shipped_params();
        (PALW_T12_USEFUL_WORK_FENCES_V1[1].set)(&mut p, Some(ForkActivation::new(AT)));
        strip(&mut p);
        let e = p.validate_palw_useful_work_v1().expect_err(what);
        assert!(format!("{e:?}").contains("palw_real_clock_tick_v1"), "{what}: {e:?}");
    }
    // The reserve needs the registry at or below it.
    let mut p = palw_t12_shipped_params();
    (PALW_T12_USEFUL_WORK_FENCES_V1[0].set)(&mut p, Some(ForkActivation::new(AT)));
    p.palw_model_registry = None;
    assert!(p.validate_palw_useful_work_v1().is_err(), "no registry, no real class");
    // And neither stands on a ruleset that is not ConsensusV2.
    let mut v1 = TESTNET11_PARAMS;
    v1.palw_real_clock_tick_v1 = Some(ForkActivation::new(AT));
    assert!(v1.validate_palw_useful_work_v1().is_err() || !matches!(v1.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)));
}

#[test]
fn the_tick_rule_is_the_heartbeat_rule_when_the_fence_is_off() {
    use kaspa_consensus_core::palw_real_share_v1::palw_clock_tick_source_v1;
    // Every mergeset shape: off, the answer is the heartbeat-only one, whatever attempts it holds.
    for heartbeats in [0u64, 1, 4] {
        for attempts in [0u64, 1, 100] {
            for priced in [0u64, 2] {
                let f = PalwClockMergesetFactsV1 {
                    priced,
                    heartbeats,
                    attempts,
                    newest_beat_ms: (heartbeats > 0).then_some(50),
                    newest_attempt_ms: (attempts > 0).then_some(70),
                };
                let (stand_in, ms) = palw_clock_tick_source_v1(&f, false);
                assert_eq!(stand_in, priced == 0 && heartbeats > 0);
                assert_eq!(ms, if heartbeats > 0 { 50 } else { 0 });
            }
        }
    }
}
