//! **Lane V02 — a resolved claim's lock off the work ceiling (`Params::palw_final_lock_full_collateral`,
//! post-launch, 2026-09-26) is dormant on every shipped preset, and arming it is a scheduled fence like
//! any other.**
//!
//! testnet-12 launched from `0e8ec984e` counting an honest `Valid` seat's post-`Final` lock inside the
//! 500‰ work ceiling for `window_court`, which closes every seat's bind room once enough Finals land
//! (the 2026-09-25 sweep's V02, HIGH). The fix ships after launch behind this fence, which the operator
//! arms at the common post-launch height with every other fix of the release. So:
//!
//! * as shipped the field is `None` everywhere, testnet-12 included, the bundle's mirror is `None`, and
//!   testnet-12's three ids are the release's to the byte (`b8564b88…` / `5de80e64…` / `93da24cc…`,
//!   the shipping re-pin `9c717c16d`);
//! * armed at a future height it moves `consensus_params_id` and `consensus_schedule_id` but NOT
//!   `consensus_identity_id`, so an armed node and a shipped node stay peers until the height (and a
//!   `Some(never())` collapses to absence — the fourth of the four places a Some-only fence needs);
//! * the fork id names the height: below it an armed build and the shipped build keep each other, and
//!   from it the armed build refuses the shipped one — which is why the height must not be one
//!   testnet-12 already schedules (1,000);
//! * `validate_palw_v2` refuses it off ConsensusV2, below `palw_rcore_plus`, and with the bundle's mirror
//!   unsynced; `sync_palw_rcore_plus` re-mirrors it.
//!
//! The rule itself is `rcore_v02_final_lock_budget` (the fold) and the processor's
//! `t12_final_lock_full_collateral_fence` (a real chain crossing the height).

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params,
    palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// testnet-12 as the release ships it (`0e8ec984e`; pinned by the shipping re-pin `9c717c16d` as
/// `palw_clock_lead_cap_is_t12_only::T12_WITH_THE_CAP`): params, identity, schedule.
const T12_RELEASE: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// Heights an operator might pick after launch: a low one a drill crosses, the common post-launch
/// height (500), and a later one. Never 1,000 — `palw_bond_maturity`'s height on testnet-12.
const HEIGHTS: [u64; 3] = [60, 500, 5_000];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.final_lock_full_collateral_from_daa(),
        _ => None,
    }
}

fn armed_at(height: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_final_lock_full_collateral = Some(ForkActivation::new(height));
    p.sync_palw_final_lock_full_collateral();
    p
}

fn presets() -> Vec<(&'static str, Params)> {
    vec![
        ("testnet-12", palw_t12_shipped_params()),
        ("testnet-11", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("mainnet", mainnet_shipped_params()),
        ("testnet-10", Params::from(TESTNET_PARAMS.net)),
        ("simnet", Params::from(SIMNET_PARAMS.net)),
    ]
}

/// **Dormant everywhere as shipped**, and testnet-12's ids are the release's: the field, its mirror,
/// its Some-only writers and its collapse cost the live chain nothing until an operator arms it.
#[test]
fn the_split_is_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_final_lock_full_collateral, None, "{name}: lane V02's fence ships dormant");
        assert_eq!(p.palw_final_lock_full_collateral_fence(), None, "{name}");
        assert_eq!(mirror(&p), None, "{name}: the bundle's mirror is dormant");
        assert!(!p.palw_final_lock_full_collateral_active_at(0) && !p.palw_final_lock_full_collateral_active_at(u64::MAX), "{name}");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == "palw_final_lock_full_collateral" && fence.is_none()),
            "{name}: the fence is on the list fork_id_v1 and the schedule walk read"
        );
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    let now = ids(&t12);
    println!("testnet-12 on this build: {now:?}");
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
}

/// **Armed at a future height: the ruleset and the schedule name it, the identity does not** — so an
/// armed node and a shipped node stay peers until the height. Armed at genesis it is a rule in force
/// from block one, and the identity separates the two. A `Some(never())` is absence.
#[test]
fn arming_the_split_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_shipped_params();
    let (params_id, identity_id, schedule_id) = ids(&shipped);
    let mut seen = std::collections::BTreeSet::new();
    for height in HEIGHTS {
        let armed = armed_at(height);
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("armed at {height}: a runnable ruleset: {e:?}"));
        let (p, i, s) = ids(&armed);
        println!("testnet-12 with the split at {height}: params {p} identity {i} schedule {s}");
        assert_ne!(p, params_id, "armed at {height}: the ruleset a node announces names the fence");
        assert_ne!(s, schedule_id, "armed at {height}: the schedule the operator log names it");
        assert_eq!(i, identity_id, "armed at {height}: a height not yet reached is not yet a rule — the two builds peer");
        assert!(seen.insert(p.clone()), "armed at {height}: the height is in the params id");
        assert_eq!(armed.palw_final_lock_full_collateral_fence(), Some(ForkActivation::new(height)));
        assert_eq!(mirror(&armed), Some(height), "the bundle's mirror carries the height");
        assert!(armed.palw_final_lock_full_collateral_active_at(height) && !armed.palw_final_lock_full_collateral_active_at(height - 1));
    }
    let mut never = shipped.clone();
    never.palw_final_lock_full_collateral = Some(ForkActivation::never());
    never.sync_palw_final_lock_full_collateral();
    never.validate_palw_v2().expect("a never-armed fence is no fence");
    assert_eq!(never.palw_final_lock_full_collateral_fence(), None, "a never-armed fence arms nothing");
    assert_eq!(mirror(&never), None, "and mirrors nothing");
    assert_eq!(never.consensus_identity_id().to_string(), identity_id, "Some(never()) is absence in the identity");
    let genesis = armed_at(0);
    genesis.validate_palw_v2().expect("a network may be born with the rule");
    assert_ne!(genesis.consensus_identity_id().to_string(), identity_id, "in force at genesis separates identities");
}

/// **The fork id sees the height**: the fence is on the gate, at its own height, and not at a height
/// testnet-12 already schedules. Below it an armed build and the shipped build keep each other in BOTH
/// directions; from it the armed build refuses the shipped one.
#[test]
fn an_armed_build_below_the_split_handshakes_with_the_shipped_build() {
    let shipped = palw_t12_shipped_params();
    let shipped_gate = fork_id_gate_fences_v1(&shipped);
    for height in HEIGHTS {
        let armed = armed_at(height);
        let gate = fork_id_gate_fences_v1(&armed);
        assert!(gate.contains(&height), "armed at {height}: the height is on the fork-id gate ({gate:?})");
        assert!(!shipped_gate.contains(&height), "armed at {height}: an INDEPENDENT height, not one the release schedules");
        for daa in [0, 1, height / 2, height - 1] {
            let (a, s) = (fork_id_v1(&armed, daa), fork_id_v1(&shipped, daa));
            let armed_sees = evaluate_fork_id_v1(&armed, daa, s.fired.as_bytes().as_slice(), s.next);
            let shipped_sees = evaluate_fork_id_v1(&shipped, daa, a.fired.as_bytes().as_slice(), a.next);
            assert!(!armed_sees.refuses(), "armed at {height}, both at DAA {daa}: the armed node keeps the shipped one ({armed_sees:?})");
            assert!(!shipped_sees.refuses(), "armed at {height}, both at DAA {daa}: the shipped node keeps the armed one ({shipped_sees:?})");
        }
        let s = fork_id_v1(&shipped, height);
        let past = evaluate_fork_id_v1(&armed, height, s.fired.as_bytes().as_slice(), s.next);
        assert!(past.refuses(), "armed at {height}: from the height the armed node refuses a node that did not upgrade");
    }
}

/// **What `validate_palw_v2` refuses**: the split off ConsensusV2, below `palw_rcore_plus`, and with
/// the mirror unsynced either way; `sync_palw_rcore_plus` re-mirrors it.
#[test]
fn the_split_needs_rcore_plus_and_a_synced_mirror() {
    let named = |p: &Params, needle: &str| {
        let why = p.validate_palw_final_lock_full_collateral_v1().expect_err(needle);
        assert!(format!("{why:?}").contains(needle), "expected a refusal naming {needle:?}, got {why:?}");
        assert!(p.validate_palw_v2().is_err(), "{needle}: validate_palw_v2 asks it");
    };
    // Unsynced: the field without the mirror, and the mirror without the field.
    let mut unsynced = palw_t12_shipped_params();
    unsynced.palw_final_lock_full_collateral = Some(ForkActivation::new(500));
    named(&unsynced, "disagrees with the V2 bundle's mirror");
    let mut stale = armed_at(500);
    stale.palw_final_lock_full_collateral = None;
    named(&stale, "without palw_final_lock_full_collateral armed");
    // `sync_palw_rcore_plus` re-mirrors it (every site that assembles or re-fences a bundle calls it).
    let mut via_rcore = palw_t12_shipped_params();
    via_rcore.palw_final_lock_full_collateral = Some(ForkActivation::new(500));
    via_rcore.sync_palw_rcore_plus();
    assert_eq!(mirror(&via_rcore), Some(500));
    via_rcore.validate_palw_v2().expect("re-mirrored through R-core+'s setter");
    // Below R-core+ (a hypothetical network arming R-core+ later than the split) — refused by name.
    let mut early = armed_at(500);
    early.palw_rcore_plus = Some(ForkActivation::new(501));
    early.sync_palw_rcore_plus();
    assert!(
        early.validate_palw_final_lock_full_collateral_v1().is_err_and(|e| format!("{e:?}").contains("without palw_rcore_plus at or below it"))
    );
    let mut none = armed_at(500);
    none.palw_rcore_plus = None;
    assert!(none.validate_palw_final_lock_full_collateral_v1().is_err_and(|e| format!("{e:?}").contains("without palw_rcore_plus")));
    // Off ConsensusV2 (mainnet's bundle-free const), refused by name.
    let mut v1 = MAINNET_PARAMS;
    v1.palw_final_lock_full_collateral = Some(ForkActivation::new(500));
    assert!(v1.validate_palw_final_lock_full_collateral_v1().is_err_and(|e| format!("{e:?}").contains("not ConsensusV2")));
    // Dormant (None or never) is legal anywhere.
    for (name, mut p) in presets() {
        p.palw_final_lock_full_collateral = Some(ForkActivation::never());
        assert_eq!(p.validate_palw_final_lock_full_collateral_v1(), Ok(()), "{name}: never() is dormant");
    }
    for height in HEIGHTS {
        armed_at(height).validate_palw_v2().expect("over testnet-12's genesis R-core+, any height is legal");
    }
}
