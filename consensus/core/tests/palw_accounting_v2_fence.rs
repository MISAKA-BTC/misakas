//! **ADR-0172, consensus accounting v2** — `palw_accounting_v2` — dormant on every ruleset a node can run, in no flag-day list, fingerprinted
//! where armed (its companion values with it), invisible to the identity until it fires, named by the fork id, and refused without its
//! prerequisites. Nothing but the validators reads it yet (the pure rules are `palw_accounting_v2`'s own unit tests).
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_accounting_v2_fence`

use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_INT11_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS,
    devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_release_v5_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_accounting_v2::palw_accounting_v2_value_v1;

/// A height no testnet-12 fence uses, above the DAA-5,300 flag day every prerequisite is at or below.
const AT: u64 = 9_999;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_accounting_v2 = Some(at);
    p
}

#[test]
fn it_is_in_no_flag_day_list_and_dormant_on_every_ruleset() {
    assert!(PALW_T12_INT11_FENCES_V1.iter().all(|f| f.name != "palw_accounting_v2"), "not in the DAA-5,300 release");
    for (name, p) in [
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET_PARAMS", TESTNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("DEVNET_PARAMS", DEVNET_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        ("palw_t12_release_v5_params", palw_t12_release_v5_params()),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
    ] {
        assert_eq!(p.palw_accounting_v2, None, "{name}");
        assert!(!p.palw_accounting_v2_active_at(u64::MAX - 1), "{name}");
        assert!(p.palw_fences_v1().contains(&("palw_accounting_v2", None)), "{name}: the exhaustive list names it");
        p.validate_palw_accounting_v2().unwrap_or_else(|e| panic!("{name}: dormant validates: {e:?}"));
    }
    palw_t12_shipped_params().validate_palw_v2().expect("the shipped ruleset validates, dormant");
}

#[test]
fn armed_it_moves_the_ruleset_and_the_schedule_never_the_identity_and_the_fork_id_gates_it() {
    let shipped = palw_t12_shipped_params();
    let p = armed(ForkActivation::new(AT));
    p.validate_palw_v2().expect("testnet-12 with accounting v2 armed above the 5,300 flag day validates");
    assert!(p.palw_accounting_v2_active_at(AT) && !p.palw_accounting_v2_active_at(AT - 1));
    let (a, b) = (ids(&p), ids(&shipped));
    assert_ne!(a.0, b.0, "the params id names it");
    assert_ne!(a.2, b.2, "and the schedule");
    assert_eq!(a.1, b.1, "two builds that differ only about a future height share an identity");
    assert!(fork_id_gate_fences_v1(&p).contains(&AT));
    let old = fork_id_v1(&shipped, AT);
    assert!(evaluate_fork_id_v1(&p, AT, old.fired.as_bytes().as_slice(), old.next).refuses(), "gated from its height");
    let below = fork_id_v1(&shipped, AT - 1);
    assert!(!evaluate_fork_id_v1(&p, AT - 1, below.fired.as_bytes().as_slice(), below.next).refuses(), "kept below it");
    // Set back: the shipped ruleset, to the id; `never()` collapses out of the identity.
    let mut back = p.clone();
    back.palw_accounting_v2 = None;
    assert_eq!(ids(&back), b, "None is the shipped ruleset");
    let never = armed(ForkActivation::never());
    never.validate_palw_v2().expect("dormant");
    assert_eq!(ids(&never).1, b.1, "a never() value collapses out of the identity");
    assert_eq!(palw_accounting_v2_value_v1(), [20_000, 0, 1]);
}

#[test]
fn it_is_refused_without_its_prerequisites() {
    for (what, strip) in [
        ("palw_floor_reserve_v1", (|p: &mut Params| p.palw_floor_reserve_v1 = None) as fn(&mut Params)),
        ("palw_real_clock_tick_v1", |p| p.palw_real_clock_tick_v1 = None),
        ("palw_clock_cursor", |p| p.palw_clock_cursor = None),
        ("palw_clock_floor", |p| p.palw_clock_floor = None),
        ("palw_clock_lead_cap", |p| p.palw_clock_lead_cap = None),
        ("palw_anchor_clock", |p| p.palw_anchor_clock = None),
        ("palw_single_lottery", |p| p.palw_single_lottery = None),
        ("palw_heartbeat_transparent_same_chain", |p| p.palw_heartbeat_transparent_same_chain = None),
        ("palw_execution_lane", |p| p.palw_execution_lane = None),
        ("palw_anchor_window_v1", |p| p.palw_anchor_window_v1 = None),
        ("palw_model_registry", |p| p.palw_model_registry = None),
        ("palw_floor_reserve_v1 later than the fence", |p| p.palw_floor_reserve_v1 = Some(ForkActivation::new(AT + 1))),
    ] {
        let mut p = armed(ForkActivation::new(AT));
        strip(&mut p);
        let e = p.validate_palw_accounting_v2().expect_err(what);
        assert!(format!("{e:?}").contains("palw_accounting_v2"), "{what}: {e:?}");
    }
    // And it does not stand on a ruleset that is not ConsensusV2.
    let mut v1 = TESTNET11_PARAMS;
    v1.palw_accounting_v2 = Some(ForkActivation::new(AT));
    assert!(v1.validate_palw_accounting_v2().is_err(), "a ruleset with no V2 bundle");
}
