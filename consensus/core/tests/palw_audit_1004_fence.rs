//! **Lane PA, the 2026-10-04 audit's consensus fixes** — `palw_audit_1004_v1` — dormant on every ruleset a node can run (no height
//! anywhere yet), fingerprinted where armed, invisible to the identity until it fires, named by the fork id, refused without its
//! prerequisites or with the bundle's mirror unsynced.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_audit_1004_fence`

use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_release_v5_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_audit_1004_v1::{PALW_T12_AUDIT_1004_ENTRY, palw_audit_1004_value_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// A height no testnet-12 fence uses.
const AT: u64 = 8_888;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_shipped_params();
    (PALW_T12_AUDIT_1004_ENTRY.set)(&mut p, Some(at));
    p
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.audit_1004_from_daa(),
        _ => None,
    }
}

#[test]
fn it_is_dormant_on_every_ruleset_with_no_height_anywhere() {
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
        assert_eq!(p.palw_audit_1004_v1, None, "{name}");
        assert!(!p.palw_audit_1004_active_at(u64::MAX - 1), "{name}");
        assert_eq!(mirror(&p), None, "{name}: no mirror");
        assert!(p.palw_fences_v1().contains(&("palw_audit_1004_v1", None)), "{name}: the exhaustive list names it");
        p.validate_palw_audit_1004_v1().unwrap_or_else(|e| panic!("{name}: dormant validates: {e:?}"));
    }
    palw_t12_shipped_params().validate_palw_v2().expect("the shipped ruleset validates");
}

#[test]
fn armed_it_moves_the_ruleset_and_the_schedule_never_the_identity_and_the_fork_id_gates_it() {
    let shipped = palw_t12_shipped_params();
    let p = armed(ForkActivation::new(AT));
    p.validate_palw_v2().expect("testnet-12 with the fence armed validates (its prerequisites are genesis rules)");
    assert_eq!(mirror(&p), Some(AT), "the fold's mirror");
    let (a, b) = (ids(&p), ids(&shipped));
    assert_ne!(a.0, b.0, "the params id names it");
    assert_ne!(a.2, b.2, "and the schedule");
    assert_eq!(a.1, b.1, "two builds that differ only about a future height share an identity");
    assert!(p.palw_audit_1004_active_at(AT) && !p.palw_audit_1004_active_at(AT - 1));
    assert!(fork_id_gate_fences_v1(&p).contains(&AT));
    let old = fork_id_v1(&shipped, AT);
    assert!(evaluate_fork_id_v1(&p, AT, old.fired.as_bytes().as_slice(), old.next).refuses(), "gated from its height");
    let below = fork_id_v1(&shipped, AT - 1);
    assert!(!evaluate_fork_id_v1(&p, AT - 1, below.fired.as_bytes().as_slice(), below.next).refuses(), "kept below it");
    let mut back = p.clone();
    (PALW_T12_AUDIT_1004_ENTRY.set)(&mut back, None);
    assert_eq!(ids(&back), b, "set(None) is the shipped ruleset");
    let never = armed(ForkActivation::never());
    never.validate_palw_v2().expect("dormant");
    assert_eq!(ids(&never).1, b.1, "a never() value collapses out of the identity");
    assert_eq!(mirror(&never), None);
    assert_eq!(palw_audit_1004_value_v1()[0], 4, "the beacon's depth leads the companion values");
}

#[test]
fn it_is_refused_without_its_prerequisites_and_with_the_mirror_unsynced() {
    let mut unsynced = palw_t12_shipped_params();
    unsynced.palw_audit_1004_v1 = Some(ForkActivation::new(AT));
    assert!(unsynced.validate_palw_audit_1004_v1().is_err(), "the mirror disagrees with the fence");
    let mut stale = armed(ForkActivation::new(AT));
    stale.palw_audit_1004_v1 = None;
    assert!(stale.validate_palw_audit_1004_v1().is_err(), "a mirror with no fence");
    stale.sync_palw_audit_1004_v1();
    assert_eq!(ids(&stale), ids(&palw_t12_shipped_params()), "synced to None it is the dormant ruleset again");
    for (what, strip) in [
        ("palw_economic_safety", (|p: &mut Params| p.palw_economic_safety = None) as fn(&mut Params)),
        ("palw_model_registry", |p| p.palw_model_registry = None),
        ("palw_model_registry later than the fence", |p| p.palw_model_registry = Some(ForkActivation::new(AT + 1))),
    ] {
        let mut p = armed(ForkActivation::new(AT));
        strip(&mut p);
        let e = p.validate_palw_audit_1004_v1().expect_err(what);
        assert!(format!("{e:?}").contains("palw_audit_1004_v1"), "{what}: {e:?}");
    }
    let mut v1 = TESTNET11_PARAMS;
    v1.palw_audit_1004_v1 = Some(ForkActivation::new(AT));
    assert!(v1.validate_palw_audit_1004_v1().is_err(), "a ruleset with no V2 bundle");
}
