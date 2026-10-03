//! **Lane accept-order — a merging block applies its mergeset parents-first
//! (`Params::palw_lane_accept_parents_first`, post-launch, 2026-09-26) — is dormant on every shipped
//! preset, and arming it is a scheduled fence like any other.**
//!
//! testnet-12 launched with every round block of a lane carrying its anchor's blue work (ADR-0125), so
//! the consensus order `(blue_work, hash)` lists a tied lane in hash order and the merging block
//! accepted it so: a carrier spending its parent block's output was skipped, a fee went to whichever
//! round block sorted first (the 2026-09-26 IBD root-cause audit, merging block `42285c86…` at DAA
//! 316). The fix ships behind this fence, armed with the other post-launch fences at one independent
//! height. So:
//!
//! **testnet-12 SHIPS this fence armed at DAA 750** (`PALW_T12_POST_LAUNCH_FENCE_DAA`), with the rest of
//! `PALW_T12_POST_LAUNCH_FENCES_V1` (the int-4 post-launch release; the user's decision of 2026-09-26
//! puts this finding into it). "Dormant" below is said of every OTHER preset and of the LAUNCH ruleset —
//! `palw_t12_launch_params_v1()`, the shipped ruleset with the list set back to dormant, byte for byte
//! the `b8564b88…` release a node that has not upgraded runs — which the tests judge this fence against;
//! the shipped (armed) ids are pinned in the release constant.
//!
//! * as launched the field is `None` everywhere, testnet-12 included; as shipped testnet-12 carries it at
//!   750 and its three ids are the post-launch release's to the byte;
//! * armed at a future height it moves `consensus_params_id` and `consensus_schedule_id` but NOT
//!   `consensus_identity_id`, and `Some(never())` collapses to absence;
//! * the fork id names the height: below it an armed build and the shipped build keep each other, from
//!   it the armed build refuses the shipped one;
//! * `validate_palw_v2` refuses it off ConsensusV2 and without the execution lane at or below it;
//! * it is an entry of the release's list, so the drill's `--palw-drill-fence-at` moves it with the
//!   others and the release's arming validates with it.

use kaspa_consensus_core::config::drill::{PalwDrillSaltV1, palw_drill_post_launch_fences_at_v1};
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_POST_LAUNCH_FENCE_DAA, PALW_T12_POST_LAUNCH_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS,
    devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_drill_params_v1, palw_t12_launch_params_v1,
    palw_t12_release_v1_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};

/// testnet-12 as THIS build ships it — the post-launch release, every fence of
/// `PALW_T12_POST_LAUNCH_FENCES_V1` (this one among them) at DAA 750, re-pinned by `scripts/t12-repin.sh`
/// with it (the launch release was `b8564b88…` / `5de80e64…` / `93da24cc…`, which
/// `palw_t12_launch_params_v1()` still hashes to): params, identity, schedule.
// re-pin 2026-10-04 @533e62b541f0: audit 1004 G-1: gen max_job_step_leaves 2^30 -> 2^22 (enumeration cap) (was b41089e3…, 34b0dc14…)
const T12_RELEASE: (&str, &str, &str) = (
    "5ee7fd8ee019968cf52929b844cf9ddfb1aad500842a89cf04bced8ba4edefb6",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "1678e07359f6727e96224041450a3b1d2aadcf8acd6bb6db0c277ff4d401c9b9",
);

/// The post-launch release's height (DAA 750) and two more an operator might pick. Never 1,000 —
/// `palw_bond_maturity`'s height on testnet-12, where a second fence would be invisible to the fork id.
const HEIGHTS: [u64; 3] = [750, 1_234, 5_000];

const NAME: &str = "palw_lane_accept_parents_first";

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

/// The LAUNCH ruleset with this fence alone at `height`.
fn armed_at(height: u64) -> Params {
    let mut p = palw_t12_launch_params_v1();
    p.palw_lane_accept_parents_first = Some(ForkActivation::new(height));
    p
}

fn presets() -> Vec<(&'static str, Params)> {
    vec![
        ("testnet-12 as launched", palw_t12_launch_params_v1()),
        ("testnet-11", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("mainnet", mainnet_shipped_params()),
        ("testnet-10", Params::from(TESTNET_PARAMS.net)),
        ("simnet", Params::from(SIMNET_PARAMS.net)),
    ]
}

/// **Dormant on every other preset and as launched; testnet-12 ships it at 750**, and its ids are the
/// post-launch release's.
#[test]
fn the_fence_is_dormant_on_every_other_preset_and_testnet12_arms_it_at_750() {
    for (name, p) in presets() {
        assert_eq!(p.palw_lane_accept_parents_first, None, "{name}: lane accept-order's fence ships dormant");
        assert!(!p.palw_lane_accept_parents_first_active_at(0) && !p.palw_lane_accept_parents_first_active_at(u64::MAX - 1), "{name}");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == NAME && fence.is_none()),
            "{name}: the fence is on the list fork_id_v1 and the schedule walk read"
        );
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    assert_eq!(
        t12.palw_lane_accept_parents_first,
        Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA)),
        "testnet-12 ships it armed at DAA 750, the post-launch release's height"
    );
    assert!(t12.palw_lane_accept_parents_first_active_at(750) && !t12.palw_lane_accept_parents_first_active_at(749));
    let now = ids(&t12);
    println!("testnet-12 on this build: {now:?}");
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
}

/// **Armed at a future height: the ruleset and the schedule name it, the identity does not**, and a
/// `Some(never())` is absence; armed at genesis the identities separate.
#[test]
fn arming_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_launch_params_v1();
    let (params_id, identity_id, schedule_id) = ids(&shipped);
    let mut seen = std::collections::BTreeSet::new();
    for height in HEIGHTS {
        let armed = armed_at(height);
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("armed at {height}: a runnable ruleset: {e:?}"));
        let (p, i, s) = ids(&armed);
        println!("testnet-12 armed at {height}: params {p} identity {i} schedule {s}");
        assert_ne!(p, params_id, "armed at {height}: the ruleset a node announces names the fence");
        assert_ne!(s, schedule_id, "armed at {height}: the schedule the operator log names it");
        assert_eq!(i, identity_id, "armed at {height}: a height not yet reached is not yet a rule — the two builds peer");
        assert!(seen.insert(p.clone()), "armed at {height}: the height is in the params id");
        assert!(armed.palw_lane_accept_parents_first_active_at(height) && !armed.palw_lane_accept_parents_first_active_at(height - 1));
    }
    let mut never = shipped.clone();
    never.palw_lane_accept_parents_first = Some(ForkActivation::never());
    never.validate_palw_v2().expect("never() is dormant");
    assert!(!never.palw_lane_accept_parents_first_active_at(u64::MAX - 1));
    assert_eq!(never.consensus_identity_id().to_string(), identity_id, "Some(never()) is absence in the identity");
    let genesis = armed_at(0);
    genesis.validate_palw_v2().expect("a network may be born with the rule (testnet-12 opens the lane at genesis)");
    assert_ne!(genesis.consensus_identity_id().to_string(), identity_id, "in force at genesis separates identities");
}

/// **The fork id sees the height**: on the gate at its own height; below it the builds keep each other
/// both ways, from it the armed build refuses the shipped.
#[test]
fn an_armed_build_below_its_fence_handshakes_with_the_shipped_build() {
    let shipped = palw_t12_launch_params_v1();
    let shipped_gate = fork_id_gate_fences_v1(&shipped);
    for height in HEIGHTS {
        let armed = armed_at(height);
        let gate = fork_id_gate_fences_v1(&armed);
        assert!(gate.contains(&height), "armed at {height}: the height is on the fork-id gate ({gate:?})");
        assert!(!shipped_gate.contains(&height), "armed at {height}: an INDEPENDENT height, not one the release schedules");
        for daa in [0, 1, height / 2, height - 1] {
            let (a, s) = (fork_id_v1(&armed, daa), fork_id_v1(&shipped, daa));
            assert!(
                !evaluate_fork_id_v1(&armed, daa, s.fired.as_bytes().as_slice(), s.next).refuses(),
                "armed at {height}, DAA {daa}"
            );
            assert!(
                !evaluate_fork_id_v1(&shipped, daa, a.fired.as_bytes().as_slice(), a.next).refuses(),
                "armed at {height}, DAA {daa}"
            );
        }
        let s = fork_id_v1(&shipped, height);
        assert!(
            evaluate_fork_id_v1(&armed, height, s.fired.as_bytes().as_slice(), s.next).refuses(),
            "armed at {height}: from the height the armed node refuses a node that did not upgrade"
        );
    }
}

/// **What `validate_palw_v2` refuses**: the fence where the execution lane opens above it (testnet-11
/// opens it at 6,001) and on a network that is not ConsensusV2 — by name.
#[test]
fn the_fence_needs_consensus_v2_and_the_execution_lane_at_or_below_it() {
    let mut t11 = palw_rc_shipped_params();
    let lane = t11.palw_execution_lane.expect("testnet-11 schedules the execution lane");
    assert!(lane.activation.daa_score() > 1_234);
    t11.palw_lane_accept_parents_first = Some(ForkActivation::new(1_234));
    let refused = t11.validate_palw_v2();
    println!("testnet-11 armed below its lane: {refused:?}");
    assert!(refused.is_err_and(|e| format!("{e:?}").contains(NAME)), "refused by name below the lane");
    t11.palw_lane_accept_parents_first = Some(lane.activation);
    t11.validate_palw_v2().expect("at the lane's own height the fence is legal");

    let mut t10 = Params::from(TESTNET_PARAMS.net);
    t10.palw_lane_accept_parents_first = Some(ForkActivation::new(1_234));
    let refused = t10.validate_palw_v2();
    println!("testnet-10 armed: {refused:?}");
    assert!(refused.is_err_and(|e| format!("{e:?}").contains(NAME)), "refused by name off ConsensusV2");
    assert!(!t10.palw_lane_accept_parents_first_active_at(u64::MAX - 1), "and never live there");

    for height in HEIGHTS {
        armed_at(height).validate_palw_v2().expect("over testnet-12's genesis lane, any height is legal");
    }
}

/// **It rides the release's list**: every post-launch fence at 750 on the shipped ruleset validates
/// with this one among them, and the drill's flag moves it with the others to a low height.
#[test]
fn the_release_list_arms_it_and_the_drill_moves_it() {
    let entry = PALW_T12_POST_LAUNCH_FENCES_V1.iter().find(|f| f.name == NAME).expect("an entry of the release's list");
    let mut armed = palw_t12_launch_params_v1();
    for fence in PALW_T12_POST_LAUNCH_FENCES_V1 {
        (fence.set)(&mut armed, Some(ForkActivation::new(750)));
    }
    armed.validate_palw_v2().expect("the release's list at DAA 750 is a runnable testnet-12 ruleset");
    assert_eq!(
        format!("{armed:?}"),
        format!("{:?}", palw_t12_release_v1_params()),
        "the DAA-750 release IS the list at 750 (the second flag day's list, `PALW_T12_POST_LAUNCH_FENCES_V2`, rides on top)"
    );
    assert!(armed.palw_lane_accept_parents_first_active_at(750) && !armed.palw_lane_accept_parents_first_active_at(749));
    let mut back = armed.clone();
    (entry.set)(&mut back, None);
    assert_eq!(back.palw_lane_accept_parents_first, None, "the entry sets it dormant again");

    let drill = palw_t12_drill_params_v1(&PalwDrillSaltV1::from_bytes([0x5a; 32]).expect("a drill salt"));
    let mut moved = drill.clone();
    let moves = palw_drill_post_launch_fences_at_v1(&mut moved, 40).expect("a drill crosses the flag day at DAA 40");
    assert!(moves.iter().any(|m| m.name == NAME && m.was == Some(750) && m.at == 40), "moved by the drill's flag from 750");
    assert!(moved.palw_lane_accept_parents_first_active_at(40) && !moved.palw_lane_accept_parents_first_active_at(39));
}
