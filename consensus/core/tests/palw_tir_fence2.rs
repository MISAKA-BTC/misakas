//! **RFC-0002 Phase F: the second IR fence, `palw_tir_fence2`** — dormant on every ruleset a node can
//! run (every id pinned in `palw_tir_fences_are_dormant.rs` holds), fingerprinted where armed,
//! invisible to the identity until it fires, named by the fork id, refused without `palw_tir_v1` in
//! force at or below it or with its mirror unsynced, and armed by its flag-day entry alone.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_tir_fence2`

use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_TIR_FENCE2_DAA, PALW_T12_TIR_FENCE2_FENCES_V1, PALW_T12_TIR_FLAG_DAY_DAA,
    Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// A height no testnet-12 fence uses, above the IR flag day.
const AT: u64 = 2_345;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_shipped_params();
    for f in PALW_T12_TIR_FENCE2_FENCES_V1 {
        (f.set)(&mut p, Some(at));
    }
    p
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.tir_fence2_from_daa(),
        _ => None,
    }
}

#[test]
fn dormant_on_every_ruleset_and_listed_alone() {
    assert_eq!(PALW_T12_TIR_FENCE2_DAA, None, "the height is the user's to set");
    let names: Vec<&str> = PALW_T12_TIR_FENCE2_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(names, ["palw_tir_fence2"]);
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
        assert_eq!(p.palw_tir_fence2, None, "{name}");
        assert!(!p.palw_tir_fence2_active_at(u64::MAX - 1), "{name}");
        assert_eq!(mirror(&p), None, "{name}: no mirror");
        assert!(p.palw_fences_v1().contains(&("palw_tir_fence2", None)), "{name}: the exhaustive list names it");
    }
}

#[test]
fn armed_it_moves_the_ruleset_and_the_schedule_never_the_identity_and_the_fork_id_gates_it() {
    let shipped = palw_t12_shipped_params();
    let p = armed(ForkActivation::new(AT));
    p.validate_palw_v2().expect("testnet-12 with the second IR fence past the IR flag day validates");
    assert_eq!(mirror(&p), Some(AT), "the fold's mirror");
    let (a, b) = (ids(&p), ids(&shipped));
    assert_ne!(a.0, b.0, "the params id names it");
    assert_ne!(a.2, b.2, "and the schedule");
    assert_eq!(a.1, b.1, "two builds that differ only about a future height share an identity");
    assert!(p.palw_tir_fence2_active_at(AT) && !p.palw_tir_fence2_active_at(AT - 1));
    assert!(fork_id_gate_fences_v1(&p).contains(&AT));
    let old = fork_id_v1(&shipped, AT);
    assert!(evaluate_fork_id_v1(&p, AT, old.fired.as_bytes().as_slice(), old.next).refuses(), "gated from its height");
    let below = fork_id_v1(&shipped, AT - 1);
    assert!(!evaluate_fork_id_v1(&p, AT - 1, below.fired.as_bytes().as_slice(), below.next).refuses(), "kept below it");
    // Set back: the shipped ruleset, to the id; `never()` collapses out of the identity.
    let mut back = p.clone();
    for f in PALW_T12_TIR_FENCE2_FENCES_V1 {
        (f.set)(&mut back, None);
    }
    assert_eq!(ids(&back), b, "set(None) is the shipped ruleset");
    let never = armed(ForkActivation::never());
    never.validate_palw_v2().expect("dormant");
    assert_eq!(ids(&never).1, b.1);
    assert_eq!(mirror(&never), None);
}

#[test]
fn it_is_refused_below_palw_tir_v1_and_with_its_mirror_unsynced() {
    let ir = PALW_T12_TIR_FLAG_DAY_DAA.expect("the IR flag day's height");
    let e = armed(ForkActivation::new(ir - 1)).validate_palw_v2().expect_err("below palw_tir_v1");
    assert!(format!("{e:?}").contains("palw_tir_v1"), "{e:?}");
    armed(ForkActivation::new(ir)).validate_palw_v2().expect("at the IR flag day's own height");
    let mut unsynced = palw_t12_shipped_params();
    unsynced.palw_tir_fence2 = Some(ForkActivation::new(AT));
    assert!(unsynced.validate_palw_tir_fence2().is_err(), "the mirror disagrees with the fence");
    let mut stale = armed(ForkActivation::new(AT));
    stale.palw_tir_fence2 = None;
    assert!(stale.validate_palw_tir_fence2().is_err(), "a mirror with no fence");
    stale.sync_palw_tir_fence2();
    assert_eq!(ids(&stale), ids(&palw_t12_shipped_params()), "re-synced it is the shipped ruleset");
}
