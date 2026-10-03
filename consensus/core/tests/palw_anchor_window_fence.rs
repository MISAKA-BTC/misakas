//! **ADR-0170, the seed anchor window** — `palw_anchor_window_v1` — dormant on every ruleset a node can run, fingerprinted where
//! armed (its companion values with it), invisible to the identity until it fires, named by the fork id, refused without its
//! prerequisites or with the bundle's mirror unsynced, and armed by its entry alone; on the DAA-5,300 list right after
//! `palw_real_clock_tick_v1`.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_anchor_window_fence`

use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_INT11_FENCES_V1, PALW_T12_INT11_FLAG_DAY_DAA, Params, SIMNET_PARAMS, TESTNET_PARAMS,
    TESTNET11_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_release_v5_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_anchor_window_v1::{PALW_T12_ANCHOR_WINDOW_FENCES_V1, palw_anchor_window_value_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// A height no testnet-12 fence uses.
const AT: u64 = 7_777;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_release_v5_params();
    for f in PALW_T12_ANCHOR_WINDOW_FENCES_V1 {
        (f.set)(&mut p, Some(at));
    }
    p
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.anchor_window_from_daa(),
        _ => None,
    }
}

#[test]
fn it_is_on_the_5300_list_after_the_clock_tick_and_dormant_on_every_other_ruleset() {
    assert_eq!(PALW_T12_ANCHOR_WINDOW_FENCES_V1.iter().map(|f| f.name).collect::<Vec<_>>(), ["palw_anchor_window_v1"]);
    let names: Vec<&str> = PALW_T12_INT11_FENCES_V1.iter().map(|f| f.name).collect();
    let at = names.iter().position(|n| *n == "palw_anchor_window_v1").expect("on the int-11 list");
    assert_eq!(names[at - 1], "palw_real_clock_tick_v1", "right after the clock tick");
    assert_eq!(names[at + 1], "palw_capacity_network_verify", "and before the capacity entries, which stay the tail");
    let shipped = palw_t12_shipped_params();
    assert_eq!(shipped.palw_anchor_window_v1.map(|a| a.daa_score()), PALW_T12_INT11_FLAG_DAY_DAA, "testnet-12 as shipped arms it at the flag day");
    assert_eq!(mirror(&shipped), PALW_T12_INT11_FLAG_DAY_DAA, "and the fold's mirror agrees");
    shipped.validate_palw_v2().expect("the shipped ruleset validates with it armed");
    for (name, p) in [
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET_PARAMS", TESTNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("DEVNET_PARAMS", DEVNET_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        // The dormant baseline is the int-10 release: the list is the DAA-5,300 flag day's alone.
        ("palw_t12_release_v5_params", palw_t12_release_v5_params()),
    ] {
        assert_eq!(p.palw_anchor_window_v1, None, "{name}");
        assert!(!p.palw_anchor_window_active_at(u64::MAX - 1), "{name}");
        assert_eq!(mirror(&p), None, "{name}: no mirror");
        assert!(p.palw_fences_v1().contains(&("palw_anchor_window_v1", None)), "{name}: the exhaustive list names it");
        p.validate_palw_anchor_window_v1().unwrap_or_else(|e| panic!("{name}: dormant validates: {e:?}"));
    }
}

#[test]
fn armed_it_moves_the_ruleset_and_the_schedule_never_the_identity_and_the_fork_id_gates_it() {
    let shipped = palw_t12_release_v5_params();
    let p = armed(ForkActivation::new(AT));
    p.validate_palw_v2().expect("testnet-12 with the window armed validates (every prerequisite is a genesis rule)");
    assert_eq!(mirror(&p), Some(AT), "the fold's mirror");
    let (a, b) = (ids(&p), ids(&shipped));
    assert_ne!(a.0, b.0, "the params id names it");
    assert_ne!(a.2, b.2, "and the schedule");
    assert_eq!(a.1, b.1, "two builds that differ only about a future height share an identity");
    assert!(p.palw_anchor_window_active_at(AT) && !p.palw_anchor_window_active_at(AT - 1));
    let state = match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.clone(),
        _ => panic!("testnet-12 is ConsensusV2"),
    };
    assert_eq!(state.anchor_window_spans_at(AT), Some(24), "W = 24 past the fence");
    assert_eq!(state.anchor_window_spans_at(AT - 1), None, "and no window below it");
    assert!(state.anchor_window_records_merged_at(AT) && !state.anchor_window_records_merged_at(AT - 1), "M1 rides the fence");
    assert!(fork_id_gate_fences_v1(&p).contains(&AT));
    let old = fork_id_v1(&shipped, AT);
    assert!(evaluate_fork_id_v1(&p, AT, old.fired.as_bytes().as_slice(), old.next).refuses(), "gated from its height");
    let below = fork_id_v1(&shipped, AT - 1);
    assert!(!evaluate_fork_id_v1(&p, AT - 1, below.fired.as_bytes().as_slice(), below.next).refuses(), "kept below it");
    // Set back: the shipped ruleset, to the id; `never()` collapses out of the identity.
    let mut back = p.clone();
    for f in PALW_T12_ANCHOR_WINDOW_FENCES_V1 {
        (f.set)(&mut back, None);
    }
    assert_eq!(ids(&back), b, "set(None) is the shipped ruleset");
    let never = armed(ForkActivation::never());
    never.validate_palw_v2().expect("dormant");
    assert_eq!(ids(&never).1, b.1, "a never() value collapses out of the identity (the params and schedule ids name the field as they do for every Some-only fence)");
    assert_eq!(mirror(&never), None);
    // The companion values are hashed with the height: W = 24 with M1 in is the recommended variant, [64, 0] the conservative one.
    assert_eq!(palw_anchor_window_value_v1(), [24, 1]);
}

#[test]
fn it_is_refused_without_its_prerequisites_and_with_the_mirror_unsynced() {
    let mut unsynced = palw_t12_release_v5_params();
    unsynced.palw_anchor_window_v1 = Some(ForkActivation::new(AT));
    assert!(unsynced.validate_palw_anchor_window_v1().is_err(), "the mirror disagrees with the fence");
    let mut stale = armed(ForkActivation::new(AT));
    stale.palw_anchor_window_v1 = None;
    assert!(stale.validate_palw_anchor_window_v1().is_err(), "a mirror with no fence");
    stale.sync_palw_anchor_window_v1();
    assert_eq!(ids(&stale), ids(&palw_t12_release_v5_params()), "synced to None it is the dormant ruleset again");
    for (what, strip) in [
        ("palw_execution_lane", (|p: &mut Params| p.palw_execution_lane = None) as fn(&mut Params)),
        ("palw_economic_safety", |p| p.palw_economic_safety = None),
        ("palw_admission_independence", |p| p.palw_admission_independence = None),
        ("palw_model_registry", |p| p.palw_model_registry = None),
        ("palw_model_registry later than the fence", |p| p.palw_model_registry = Some(ForkActivation::new(AT + 1))),
    ] {
        let mut p = armed(ForkActivation::new(AT));
        strip(&mut p);
        let e = p.validate_palw_anchor_window_v1().expect_err(what);
        assert!(format!("{e:?}").contains("palw_anchor_window_v1"), "{what}: {e:?}");
    }
    // And it does not stand on a ruleset that is not ConsensusV2.
    let mut v1 = TESTNET11_PARAMS;
    v1.palw_anchor_window_v1 = Some(ForkActivation::new(AT));
    assert!(v1.validate_palw_anchor_window_v1().is_err(), "a ruleset with no V2 bundle");
}
