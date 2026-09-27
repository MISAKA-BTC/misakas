//! **Lane F2-lock — the `F + 1,000` seat-lock life applied retroactively
//! (`Params::palw_final_lock_life_retro`, post-launch, 2026-09-27) is dormant on every shipped preset,
//! and arming it is a scheduled fence like any other.**
//!
//! The DAA-750 release's lane V02 fence (`palw_final_lock_life`) shortened only the locks it DATES;
//! every lock a `Final` below DAA 750 stamped `F + window_court` kept that life, and by the auditor's
//! count at DAA 816 each genesis seat carried 959–1,583 of them. This fence re-dates them once, at its
//! crossing block, and dates every `Final` past it exactly `F + 1,000`. It ships dormant and joins
//! testnet-12's NEXT flag day (the list that follows `PALW_T12_POST_LAUNCH_FENCES_V1`), so:
//!
//! * as this build ships, the field is `None` everywhere, testnet-12 included, the bundle's mirror is
//!   `None`, and testnet-12's three ids are the DAA-750 release's to the byte (`dbbc9104…` /
//!   `5de80e64…` / `7c652212…`, the fleet's `c3dbaee3c`) — every other preset's too;
//! * armed at a future height over the DAA-750 release it moves `consensus_params_id` and
//!   `consensus_schedule_id` but NOT `consensus_identity_id`, so an armed node and a fleet node stay
//!   peers until the height (and a `Some(never())` collapses to absence);
//! * the fork id names the height: below it an armed build and the fleet's build keep each other in
//!   both directions, from it the armed build refuses the fleet's;
//! * `validate_palw_v2` refuses it off ConsensusV2, without `palw_final_lock_life` at or below it, and
//!   with the bundle's mirror unsynced; `sync_palw_rcore_plus` re-mirrors it.
//!
//! The rule itself is exercised on testnet-12's fold by `rcore_f2_lock_redate`.

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PALW_T12_POST_LAUNCH_FENCE_DAA, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_launch_params_v1, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// testnet-12 as the fleet runs it — the DAA-750 post-launch release (`c3dbaee3c`): params, identity,
/// schedule. `palw_final_lock_life_is_t12_only`'s `T12_RELEASE`; this fence, dormant, must not move it.
const T12_RELEASE: (&str, &str, &str) = (
    "dbbc9104a2ee754f0f053a6e1614118979fd2c3dc87cbe6bffcf6dcaf4bd59c9",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "7c652212ab5337bda9508deeee2d2e119331856fce0bd27897f19dd66e552397",
);

/// Heights the next flag day might take: above the DAA-750 release (its lane V02 fence must be at or
/// below), and never 1,000 (`palw_bond_maturity`'s height on testnet-12, which the fork id cannot tell
/// apart).
const HEIGHTS: [u64; 3] = [760, 1_300, 5_000];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.final_lock_life_retro_from_daa(),
        _ => None,
    }
}

/// The fleet's ruleset (the DAA-750 release) with lane F2-lock armed at `height`, re-mirrored.
fn armed_at(height: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_final_lock_life_retro = Some(ForkActivation::new(height));
    p.sync_palw_final_lock_life_retro();
    p
}

fn presets() -> Vec<(&'static str, Params)> {
    vec![
        ("testnet-12 as the fleet runs it (DAA-750 release)", palw_t12_shipped_params()),
        ("testnet-12 as launched", palw_t12_launch_params_v1()),
        ("testnet-11", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("mainnet", mainnet_shipped_params()),
        ("testnet-10", Params::from(TESTNET_PARAMS.net)),
        ("simnet", Params::from(SIMNET_PARAMS.net)),
    ]
}

/// **Dormant everywhere as shipped**, and testnet-12's ids are the DAA-750 release's: the field, its
/// mirror, its Some-only writers and its collapse cost the live chain nothing until a flag day arms it.
#[test]
fn the_retro_life_is_dormant_on_every_preset_and_testnet12_is_the_daa750_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_final_lock_life_retro, None, "{name}: lane F2-lock's fence ships dormant");
        assert_eq!(p.palw_final_lock_life_retro_fence(), None, "{name}");
        assert_eq!(mirror(&p), None, "{name}: the bundle's mirror is dormant");
        assert!(!p.palw_final_lock_life_retro_active_at(0) && !p.palw_final_lock_life_retro_active_at(u64::MAX), "{name}");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == "palw_final_lock_life_retro" && fence.is_none()),
            "{name}: the fence is on the list fork_id_v1 and the schedule walk read"
        );
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    assert_eq!(
        t12.palw_final_lock_life,
        Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA)),
        "the prerequisite, lane V02's lock life, is the DAA-750 release's"
    );
    let now = ids(&t12);
    println!("testnet-12 on this build: {now:?}");
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the DAA-750 release, to the id");
    // A `Some(never())` is absence, in all three ids.
    let mut never = palw_t12_shipped_params();
    never.palw_final_lock_life_retro = Some(ForkActivation::never());
    never.sync_palw_final_lock_life_retro();
    never.validate_palw_v2().expect("a never-armed fence is no fence");
    assert_eq!(never.palw_final_lock_life_retro_fence(), None);
    assert_eq!(mirror(&never), None, "never() mirrors nothing");
    assert_eq!(never.consensus_identity_id().to_string(), T12_RELEASE.1, "Some(never()) is absence in the identity");
}

/// **Armed at a future height: the ruleset and the schedule name it, the identity does not** — so an
/// armed node and a fleet node stay peers until the height. Armed at genesis (over a network born with
/// lane V02 there) it is a rule in force from block one, and the identity separates the two.
#[test]
fn arming_the_retro_life_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_shipped_params();
    let (params_id, identity_id, schedule_id) = ids(&shipped);
    let mut seen = std::collections::BTreeSet::new();
    for height in HEIGHTS {
        let armed = armed_at(height);
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("armed at {height}: a runnable ruleset: {e:?}"));
        let (p, i, s) = ids(&armed);
        println!("testnet-12 with the retro life at {height}: params {p} identity {i} schedule {s}");
        assert_ne!(p, params_id, "armed at {height}: the ruleset a node announces names the fence");
        assert_ne!(s, schedule_id, "armed at {height}: the schedule the operator log names it");
        assert_eq!(i, identity_id, "armed at {height}: a height not yet reached is not yet a rule — the two builds peer");
        assert!(seen.insert(p.clone()), "armed at {height}: the height is in the params id");
        assert_eq!(armed.palw_final_lock_life_retro_fence(), Some(ForkActivation::new(height)));
        assert_eq!(mirror(&armed), Some(height), "the bundle's mirror carries the height");
        assert!(armed.palw_final_lock_life_retro_active_at(height) && !armed.palw_final_lock_life_retro_active_at(height - 1));
    }
    let mut genesis = palw_t12_launch_params_v1();
    genesis.palw_final_lock_life = Some(ForkActivation::new(0));
    genesis.palw_final_lock_life_retro = Some(ForkActivation::new(0));
    genesis.sync_palw_rcore_plus();
    genesis.validate_palw_v2().expect("a network may be born with the rule");
    assert_ne!(genesis.consensus_identity_id().to_string(), palw_t12_launch_params_v1().consensus_identity_id().to_string());
}

/// **The fork id sees the height**: the fence is on the gate, at its own height, and not at a height
/// the DAA-750 release already schedules. Below it an armed build and the fleet's build keep each other
/// in BOTH directions; from it the armed build refuses the fleet's.
#[test]
fn an_armed_build_below_the_retro_life_handshakes_with_the_fleet_build() {
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
            assert!(!armed_sees.refuses(), "armed at {height}, both at DAA {daa}: the armed node keeps the fleet's ({armed_sees:?})");
            assert!(
                !shipped_sees.refuses(),
                "armed at {height}, both at DAA {daa}: the fleet node keeps the armed one ({shipped_sees:?})"
            );
        }
        let s = fork_id_v1(&shipped, height);
        let past = evaluate_fork_id_v1(&armed, height, s.fired.as_bytes().as_slice(), s.next);
        assert!(past.refuses(), "armed at {height}: from the height the armed node refuses a node that did not upgrade");
    }
}

/// **What `validate_palw_v2` refuses**: the retro life off ConsensusV2, without lane V02's lock life at
/// or below it, and with the mirror unsynced either way; `sync_palw_rcore_plus` re-mirrors it.
#[test]
fn the_retro_life_needs_the_v02_lock_life_at_or_below_it_and_a_synced_mirror() {
    let named = |p: &Params, needle: &str| {
        let why = p.validate_palw_final_lock_life_retro_v1().expect_err(needle);
        assert!(format!("{why:?}").contains(needle), "expected a refusal naming {needle:?}, got {why:?}");
        assert!(p.validate_palw_v2().is_err(), "{needle}: validate_palw_v2 asks it");
    };
    // Unsynced: the field without the mirror, and the mirror without the field.
    let mut unsynced = palw_t12_shipped_params();
    unsynced.palw_final_lock_life_retro = Some(ForkActivation::new(1_300));
    named(&unsynced, "disagrees with the V2 bundle's mirror");
    let mut stale = armed_at(1_300);
    stale.palw_final_lock_life_retro = None;
    named(&stale, "without palw_final_lock_life_retro armed");
    // `sync_palw_rcore_plus` re-mirrors it (every site that assembles or re-fences a bundle calls it).
    let mut via_rcore = palw_t12_shipped_params();
    via_rcore.palw_final_lock_life_retro = Some(ForkActivation::new(1_300));
    via_rcore.sync_palw_rcore_plus();
    assert_eq!(mirror(&via_rcore), Some(1_300));
    via_rcore.validate_palw_v2().expect("re-mirrored through R-core+'s setter");
    // Below lane V02's lock life: refused by name — on the launch ruleset (V02 dormant) and under it.
    let mut launch = palw_t12_launch_params_v1();
    launch.palw_final_lock_life_retro = Some(ForkActivation::new(1_300));
    launch.sync_palw_final_lock_life_retro();
    named(&launch, "without palw_final_lock_life at or below it");
    let below = armed_at(PALW_T12_POST_LAUNCH_FENCE_DAA - 1);
    named(&below, "without palw_final_lock_life at or below it");
    armed_at(PALW_T12_POST_LAUNCH_FENCE_DAA).validate_palw_v2().expect("at lane V02's own height");
    // Off ConsensusV2 (mainnet's bundle-free const), refused by name.
    let mut v1 = MAINNET_PARAMS;
    v1.palw_final_lock_life = Some(ForkActivation::new(500));
    v1.palw_final_lock_life_retro = Some(ForkActivation::new(500));
    assert!(v1.validate_palw_final_lock_life_retro_v1().is_err_and(|e| format!("{e:?}").contains("not ConsensusV2")));
    // Dormant (None or never) is legal anywhere.
    for (name, mut p) in presets() {
        p.palw_final_lock_life_retro = Some(ForkActivation::never());
        assert_eq!(p.validate_palw_final_lock_life_retro_v1(), Ok(()), "{name}: never() is dormant");
    }
    for height in HEIGHTS {
        armed_at(height).validate_palw_v2().expect("over the DAA-750 release, any height at or past 750 is legal");
    }
}
