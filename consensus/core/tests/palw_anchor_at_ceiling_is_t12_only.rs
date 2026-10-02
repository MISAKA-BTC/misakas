//! **Lane bind-deadlock — the anchor-at-ceiling fence (`Params::palw_anchor_at_ceiling`, post-launch,
//! 2026-09-26) is dormant on every shipped preset, and arming it is a scheduled fence like any other.**
//!
//! **Since the post-launch release (int-4; the user's decision of 2026-09-26) testnet-12 SHIPS this
//! fence armed at DAA 750** (`PALW_T12_POST_LAUNCH_FENCE_DAA`), with the rest of
//! `PALW_T12_POST_LAUNCH_FENCES_V1`. What is said below of "as shipped" / "the release" now holds of
//! the LAUNCH ruleset — `palw_t12_launch_params_v1()`, the shipped ruleset with the post-launch list
//! set back to dormant, byte for byte the `b8564b88…` release a node that has not upgraded runs — and
//! the tests judge this fence alone against it; the shipped (armed) ids are pinned in the release
//! constant, and the shipped ruleset is asserted to carry the fence at 750.
//!
//! testnet-12 launched from `0e8ec984e` where a chain block whose own attempt its bond cannot back
//! (admission item 8) is disqualified even when it is the only anchor a due claim can have, so a
//! fleet whose producers all stand at their exposure ceilings binds nothing until the `BindTimeout`
//! backstop (audit-lifecycle T12-052; `t12_bind_deadlock` at the processor). The fix ships behind
//! this fence, armed with the other post-launch fences at one independent height. So:
//!
//! * as shipped the field is `None` everywhere, testnet-12 included, and testnet-12's three ids are the
//!   release's to the byte (`b8564b88…` / `5de80e64…` / `93da24cc…`, the shipping re-pin `9c717c16d`);
//! * armed at a future height it moves `consensus_params_id` and `consensus_schedule_id` but NOT
//!   `consensus_identity_id`, and `Some(never())` collapses to absence (the fourth of the four places a
//!   Some-only fence needs);
//! * the fork id names the height: below it an armed build and the shipped build keep each other, from
//!   it the armed build refuses the shipped one;
//! * `validate_palw_v2` refuses it without `palw_rcore_plus` at or below it, and without lane A
//!   (`palw_operator_anchor`) at or below it (the int-4 audit's LOW: the binder's second condition is
//!   lane A's operator predicate), so the tests arm lane A with it.

use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_POST_LAUNCH_FENCE_DAA, PALW_T12_POST_LAUNCH_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS,
    devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_launch_params_v1, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};

/// testnet-12 as THIS build ships it — the post-launch release, every fence of
/// `PALW_T12_POST_LAUNCH_FENCES_V1` at DAA 750 (re-pinned by `scripts/t12-repin.sh` with it; the launch
/// release `0e8ec984e` was `b8564b88…` / `5de80e64…` / `93da24cc…`, which `palw_t12_launch_params_v1()`
/// still hashes to): params, identity, schedule — the same pins `palw_clock_lead_cap_is_t12_only`'s
/// `T12_WITH_THE_CAP` holds.
// re-pin 2026-10-01 @d45b74bafc8e: int-10: palw_tir_fence2 alone at DAA 3,600 (the model court window stays dormant) (was c5e9fd82…, 686f65ea…)
const T12_RELEASE: (&str, &str, &str) = (
    "254509533bb693ced0fed823a4c25e166ba2542d576e4021b0e4b4d6fe4079e1",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "1e39c738b97a695c8a2c2d4129660eda8fa7ac5f1e8b529b916314c01750c593",
);

/// The post-launch release's height (DAA 500, the user's decision of 2026-09-26) and two more an
/// operator might pick. Never 1,000 — `palw_bond_maturity`'s height on testnet-12, where a second
/// fence would be invisible to the fork id.
const HEIGHTS: [u64; 3] = [500, 1_234, 5_000];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

/// The launch ruleset with this fence at `height` — and lane A (with F1's seed it needs) at the same
/// height, which `validate_palw_v2` requires at or below it (the int-4 audit's LOW), as the release
/// arms them.
fn armed_at(height: u64) -> Params {
    let mut p = palw_t12_launch_params_v1();
    for name in ["palw_panel_seed_execution", "palw_operator_anchor"] {
        let fence = PALW_T12_POST_LAUNCH_FENCES_V1.iter().find(|f| f.name == name).expect("listed");
        (fence.set)(&mut p, Some(ForkActivation::new(height)));
    }
    p.palw_anchor_at_ceiling = Some(ForkActivation::new(height));
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

/// **Dormant everywhere as shipped**, and testnet-12's ids are the release's.
#[test]
fn the_fence_is_dormant_on_every_other_preset_and_testnet12_arms_it_at_750() {
    for (name, p) in presets() {
        assert_eq!(p.palw_anchor_at_ceiling, None, "{name}: lane bind-deadlock's fence ships dormant");
        assert!(!p.palw_anchor_at_ceiling_active_at(0) && !p.palw_anchor_at_ceiling_active_at(u64::MAX - 1), "{name}");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == "palw_anchor_at_ceiling" && fence.is_none()),
            "{name}: the fence is on the list fork_id_v1 and the schedule walk read"
        );
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    assert_eq!(
        t12.palw_anchor_at_ceiling,
        Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA)),
        "testnet-12 ships it armed at DAA 750, the post-launch release's height"
    );
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
        assert!(armed.palw_anchor_at_ceiling_active_at(height) && !armed.palw_anchor_at_ceiling_active_at(height - 1));
    }
    let mut never = shipped.clone();
    never.palw_anchor_at_ceiling = Some(ForkActivation::never());
    assert_eq!(never.consensus_identity_id().to_string(), identity_id, "Some(never()) is absence in the identity");
    let genesis = armed_at(0);
    genesis.validate_palw_v2().expect("a network may be born with the rule");
    assert_ne!(genesis.consensus_identity_id().to_string(), identity_id, "in force at genesis separates identities");
}

/// **The fork id sees the height**: on the gate at its own height, not at testnet-12's scheduled
/// 1,000; below it the builds keep each other both ways, from it the armed build refuses the shipped.
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

/// **What `validate_palw_v2` refuses**: the fence without R-core+ at or below it (below R-core+ a
/// heartbeat anchors, so there is no deadlock to break), by name.
#[test]
fn the_fence_needs_rcore_plus_at_or_below_it() {
    for (name, mut p) in [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        p.palw_anchor_at_ceiling = Some(ForkActivation::new(1_234));
        let refused = p.validate_palw_v2();
        println!("{name} armed: {refused:?}");
        assert!(refused.is_err_and(|e| format!("{e:?}").contains("palw_anchor_at_ceiling")), "{name}: refused by name");
    }
    let mut never = palw_t12_launch_params_v1();
    never.palw_anchor_at_ceiling = Some(ForkActivation::never());
    never.validate_palw_v2().expect("never() is dormant");
    for height in HEIGHTS {
        armed_at(height).validate_palw_v2().expect("over testnet-12's genesis R-core+ and lane A at the height, any height is legal");
    }
    // Without lane A at or below it: refused by name (the int-4 audit's LOW).
    let mut alone = palw_t12_launch_params_v1();
    alone.palw_anchor_at_ceiling = Some(ForkActivation::new(1_234));
    let refused = alone.validate_palw_v2();
    assert!(refused.is_err_and(|e| format!("{e:?}").contains("without palw_operator_anchor")), "lane A dormant: refused by name");
    let mut early = armed_at(1_234);
    early.palw_anchor_at_ceiling = Some(ForkActivation::new(1_233));
    assert!(early.validate_palw_v2().is_err(), "one DAA below lane A: refused");
}
