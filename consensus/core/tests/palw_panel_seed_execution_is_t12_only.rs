//! **Lane F1 — the panel-seed fence (`Params::palw_panel_seed_execution`, post-launch, 2026-09-25) is
//! dormant on every shipped preset, and arming it is a scheduled fence like any other.**
//!
//! testnet-12 launched from `0e8ec984e` with the panel seed keyed on the anchor block's identity
//! (`wf_72c1a397-e23`, CRITICAL). The fix ships after launch behind this fence, which an operator arms
//! at one independent post-launch height. So:
//!
//! * as shipped the field is `None` everywhere, testnet-12 included, and testnet-12's three ids are the
//!   release's to the byte (`b8564b88…` / `5de80e64…` / `93da24cc…`, the shipping re-pin `9c717c16d`);
//! * armed at a future height it moves `consensus_params_id` and `consensus_schedule_id` — the ruleset
//!   a node announces and the schedule the operator log names — but NOT `consensus_identity_id`, so an
//!   armed node and a shipped node stay peers until the height (and a `Some(never())` collapses to
//!   absence: the fourth of the four places a Some-only fence needs);
//! * the fork id names the height: below it an armed build and the shipped build keep each other,
//!   and from it the armed build refuses the shipped one (a named partition at the flag day, not a
//!   silent fork) — which is why the height must not be one testnet-12 already schedules (1,000);
//! * `validate_palw_v2` refuses it without `palw_rcore_plus` at or below it.
//!
//! The rule itself is tested at the pure layer (`palw_panel_v2::…::panel_seed_2026_09_25`), on a
//! processor chain that crosses the fence (`t12_panel_seed_fence`), and against the attacker
//! (`void_after_panel_probe`, `void_after_panel_adv_verify`).

use kaspa_consensus_core::config::params::{
    ForkActivation, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};

/// testnet-12 as the release ships it (`0e8ec984e`; pinned by the shipping re-pin `9c717c16d` as
/// `palw_clock_lead_cap_is_t12_only::T12_WITH_THE_CAP`): params, identity, schedule.
const T12_RELEASE: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// Heights an operator might pick after launch: before external seats mature (1,000 DAA) and after.
/// Never 1,000 itself — `palw_bond_maturity`'s height on testnet-12, where a second fence would be
/// invisible to the fork id.
const HEIGHTS: [u64; 3] = [900, 1_234, 5_000];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed_at(height: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_panel_seed_execution = Some(ForkActivation::new(height));
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

/// **Dormant everywhere as shipped**, and testnet-12's ids are the release's: the field, its
/// Some-only writers and its collapse cost the live chain nothing until an operator arms it.
#[test]
fn the_fence_is_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_panel_seed_execution, None, "{name}: lane F1's fence ships dormant");
        assert!(!p.palw_panel_seed_execution_active_at(0) && !p.palw_panel_seed_execution_active_at(u64::MAX - 1), "{name}");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == "palw_panel_seed_execution" && fence.is_none()),
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
/// armed node and a shipped node stay peers until the height (M1-6). Armed at genesis it is a rule in
/// force from block one, and the identity separates the two. A `Some(never())` is absence.
#[test]
fn arming_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_shipped_params();
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
        assert!(armed.palw_panel_seed_execution_active_at(height) && !armed.palw_panel_seed_execution_active_at(height - 1));
    }
    // Some(never()): the normalised form of a scheduled fence — absence in the identity, or the
    // collapse in `normalize_values_a_scheduled_fence_drags_with_it` is gone (a-some-only-fence).
    let mut never = shipped.clone();
    never.palw_panel_seed_execution = Some(ForkActivation::never());
    assert_eq!(never.consensus_identity_id().to_string(), identity_id, "Some(never()) is absence in the identity");
    // At genesis the rule is in force from block one: another identity (a regenesis, not this lane).
    let genesis = armed_at(0);
    genesis.validate_palw_v2().expect("a network may be born with the rule");
    assert_ne!(genesis.consensus_identity_id().to_string(), identity_id, "in force at genesis separates identities");
}

/// **The fork id sees the height** (a-fence-at-a-scheduled-height-is-invisible-to-the-fork-id): the
/// fence is on the gate, at its own height, and not at testnet-12's scheduled 1,000. Below it an
/// armed build and the shipped build keep each other in BOTH directions; from it the armed build
/// refuses the shipped one — the flag day is a named refusal, not a silent fork.
#[test]
fn an_armed_build_below_its_fence_handshakes_with_the_shipped_build() {
    let shipped = palw_t12_shipped_params();
    let shipped_gate = fork_id_gate_fences_v1(&shipped);
    println!("testnet-12 as shipped gates on {shipped_gate:?}");
    for height in HEIGHTS {
        let armed = armed_at(height);
        let gate = fork_id_gate_fences_v1(&armed);
        assert!(gate.contains(&height), "armed at {height}: the height is on the fork-id gate ({gate:?})");
        assert!(!shipped_gate.contains(&height), "armed at {height}: an INDEPENDENT height, not one the release schedules");
        for daa in [0, 1, height / 2, height - 1] {
            let (a, s) = (fork_id_v1(&armed, daa), fork_id_v1(&shipped, daa));
            let armed_sees = evaluate_fork_id_v1(&armed, daa, s.fired.as_bytes().as_slice(), s.next);
            let shipped_sees = evaluate_fork_id_v1(&shipped, daa, a.fired.as_bytes().as_slice(), a.next);
            assert!(
                !armed_sees.refuses(),
                "armed at {height}, both at DAA {daa}: the armed node keeps the shipped one ({armed_sees:?})"
            );
            assert!(
                !shipped_sees.refuses(),
                "armed at {height}, both at DAA {daa}: the shipped node keeps the armed one ({shipped_sees:?})"
            );
        }
        let s = fork_id_v1(&shipped, height);
        let past = evaluate_fork_id_v1(&armed, height, s.fired.as_bytes().as_slice(), s.next);
        println!("armed at {height}: at the height the armed node says {past:?} to a shipped peer");
        assert!(past.refuses(), "armed at {height}: from the height the armed node refuses a node that did not upgrade");
    }
}

/// **What `validate_palw_v2` refuses**: the fence without R-core+ at or below it — there a heartbeat
/// may anchor a panel and has no execution commitment to key the seed on.
#[test]
fn the_fence_needs_rcore_plus_at_or_below_it() {
    for (name, mut p) in [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        p.palw_panel_seed_execution = Some(ForkActivation::new(1_234));
        let refused = p.validate_palw_v2();
        println!("{name} armed: {refused:?}");
        assert!(
            refused.is_err_and(|e| format!("{e:?}").contains("palw_panel_seed_execution")),
            "{name}: no R-core+, no fence — refused by name"
        );
    }
    // Dormant (None or never) is legal anywhere: the field costs a network that does not arm it nothing.
    for (name, mut p) in presets() {
        p.palw_panel_seed_execution = Some(ForkActivation::never());
        if name == "testnet-12" {
            p.validate_palw_v2().expect("never() is dormant");
        }
    }
    for height in HEIGHTS {
        armed_at(height).validate_palw_v2().expect("over testnet-12's genesis R-core+, any height is legal");
    }
}
