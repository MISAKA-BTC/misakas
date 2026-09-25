//! **testnet-12 takes mainnet's numbers, and those numbers are the only thing that moved it** (user
//! decision 2026-09-25: 「t12 では bond やバリデータの stake fee など全て mainnet を想定した数値で行う」).
//!
//! Three testnet-12 values moved to mainnet's:
//!
//! * ADR-0130's `λ` (`palw_panel_exposure_floor.reward_multiple_permille`) 2,000 ‰ -> 5,000 ‰ (ADR-0130
//!   D1's mainnet statement, "λ = 5–10 under consideration"; the user took 5);
//! * `timestamp_deviation_tolerance` 132 s -> 1,620 s (the value a card derives from its 27-sample
//!   median window at 120 s);
//! * `max_block_level` 250 -> 225 (`MAINNET_PARAMS`).
//!
//! Testnet-12's DNS set did NOT move: mainnet's `PRODUCTION_DNS_PARAMS` moved to the four numbers
//! testnet-12 already overrode it with (6 validators, a 20M MSK bond, 120M MSK of active stake, the
//! 600-DAA coinbase long maturity), so `PALW_T12_DNS_PARAMS` inherits what it used to override.
//!
//! Pinned against the parent (`rcore/int-3` at `8270cf03`): set the three back and take the offence
//! attribution fence away, and testnet-12 is exactly `palw_offence_attribution_is_t12_only`'s
//! `T12_BEFORE_THE_ATTRIBUTION` as it stood there — so nothing else in this change reached
//! testnet-12's ruleset, identity or schedule.

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PalwPanelExposureFloorV1, Params, palw_t12_shipped_params,
};

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

/// testnet-12 with the three values set back to what `8270cf03` shipped.
fn at_the_parent(mut p: Params) -> Params {
    p.palw_panel_exposure_floor =
        Some(PalwPanelExposureFloorV1 { activation: ForkActivation::always(), reward_multiple_permille: 2_000 });
    p.timestamp_deviation_tolerance = 132;
    p.max_block_level = 250;
    p
}

/// `palw_offence_attribution_is_t12_only`'s `T12_BEFORE_THE_ATTRIBUTION` at `8270cf03` (testnet-12
/// with the attribution fence taken away), before this change moved it.
const PARENT_WITHOUT_THE_ATTRIBUTION: (&str, &str, &str) = (
    "f5b564f3267e3a0a5c19dafaf5901069481132c05f4bfa464773848a3e44f1d0",
    "8eaabe3eea0f6feb11e6a211f893bc590dc2e2427771e73c8f0629cf6dbe830c",
    "b758ee801d47058306f97dcc7d558009f27c8162439990259bbee756c2a90478",
);

#[test]
fn testnet12_runs_the_three_mainnet_values() {
    let t12 = palw_t12_shipped_params();
    assert_eq!(
        t12.palw_panel_exposure_floor.map(|f| (f.activation, f.reward_multiple_permille)),
        Some((ForkActivation::always(), 5_000))
    );
    assert_eq!(t12.timestamp_deviation_tolerance, 1_620);
    assert_eq!(t12.max_block_level, MAINNET_PARAMS.max_block_level);
    assert_eq!(t12.max_block_level, 225);
    t12.validate_palw_v2().expect("testnet-12 validates with mainnet's values");
}

/// **Set the three back and testnet-12 is the parent's, to the id.** The params id, the identity and
/// the schedule all move with the change (`λ` rides beside its height in the schedule id; the tolerance
/// and the ceiling are hashed values), and every one of them returns to the parent's value when the
/// three are reverted.
#[test]
fn the_three_mainnet_values_are_the_only_thing_that_moved_testnet12() {
    let t12 = palw_t12_shipped_params();
    let parent = at_the_parent(t12.clone());
    let (now, before) = (ids(&t12), ids(&parent));
    println!("testnet-12 now:           params {} identity {} schedule {}", now.0, now.1, now.2);
    println!("testnet-12 at the parent: params {} identity {} schedule {}", before.0, before.1, before.2);
    assert_ne!(now.0, before.0, "the ruleset a node announces names the new values");
    assert_ne!(now.1, before.1, "in force from block one: two identities");
    assert_ne!(now.2, before.2, "λ is reported beside its height in the schedule id");
    let mut parent_without = parent;
    parent_without.palw_offence_attribution = None;
    // ADR-0151's stated execution-quantum maturity (user decision 2026-09-25) landed after this pin
    // was taken; taken away as well, and `palw_exec_maturity_is_t12_only` pins what it moves.
    parent_without.palw_exec_quantum_maturity_daa = None;
    let got = ids(&parent_without);
    assert_eq!(
        (got.0.as_str(), got.1.as_str(), got.2.as_str()),
        PARENT_WITHOUT_THE_ATTRIBUTION,
        "testnet-12 with the three values set back (and the attribution fence away) is the parent's"
    );
}
