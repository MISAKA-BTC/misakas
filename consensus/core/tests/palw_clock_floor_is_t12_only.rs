//! **The clock floor (the 2026-09-24 heartbeat audit's H3 and H5) is testnet-12's alone.**
//!
//! `Params::palw_clock_floor` changes which heartbeat headers and which clock steps are valid and
//! which block a slot is measured from, so it is a consensus fence. It is `Some(0)` on testnet-12 and
//! `None` everywhere else, and it is hashed Some-only in every writer — `consensus_params_id`,
//! `consensus_schedule_id` and the `for_each_fence` walk under both — so testnet-11, devnet and
//! mainnet must fingerprint byte-identically to the build before the field existed. That claim is
//! pinned to the numbers, taken from `0b14bf80` (the commit this change was built on) by running this
//! file against it: if a change here moves them, the fence is not dormant where it must be.

use kaspa_consensus_core::config::params::{
    ForkActivation, Params, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_shipped_params,
};

/// `(network, consensus_params_id, consensus_identity_id, consensus_schedule_id)` at `0b14bf80`.
///
/// **Re-pinned once for ADR-0152's v22 skeleton**: `PALW_STATE_V2_VERSION` 21 -> 22 is hashed into
/// the V2 arm of `consensus_params_id`, so testnet-11's and devnet's params and identity ids move
/// with the version (the one re-pin it forces on every V2 preset); every schedule id, and all of
/// mainnet (no V2 bundle), are the `0b14bf80` values still. v21: testnet-11 `c99bb4f4…` /
/// `19dbdbb8…`, devnet `9acd42be…`. The floor itself still moves none of them.
const BEFORE_THE_FLOOR: &[(&str, &str, &str, &str)] = &[
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was bd633ce9…, 44cb8fd7…, 5a1d8d56…)
        "testnet-11",
        "0c2c022a11572d73779fc0610cf65d4147feb499a9b58472901d3d73ecfc1f71",
        "bf4cc0b3e2fa208a18cce48d0c33db74e48adf275a069687b534856411f1c83e",
        "f023f74befcdaf732f213616c79fd94782d7728f835151bbfd986498f962bd64",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 7a27f341…, 7a27f341…, edd80c01…)
        "devnet",
        "f17c6555a617736bd13e90eaab80974223149c211d08af516008bd6500010b1f",
        "f17c6555a617736bd13e90eaab80974223149c211d08af516008bd6500010b1f",
        "d82068a0080c47f45f376c81c621ec002aa8541a22c49e9d0a373e263c39c680",
    ),
    // Re-pinned 2026-09-25: mainnet's DNS set moved to testnet-12's numbers (6 validators x 20M MSK,
    // 120M MSK active stake, the 600-DAA coinbase long maturity; user decision). The schedule id did not
    // move. Previous: params `badaa8e9…`, identity `00d98599…`.
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was eb866c61…, 7819e5ed…, a1ed7ff0…)
        "mainnet",
        "0befd2fa1da052fb88ef8a04cc1bd6412ea560371aaec606790194dee41ac3fa",
        "db7f8f9869a612c6e90ff8a01806569a0fe7391cfa8226c3c3a89680ae83d277",
        "7b8f4090c31d524312f1b6d8a91d0e47544b031a62560d90b9a1bc65b709e7c6",
    ),
];

fn shipped(name: &str) -> Params {
    match name {
        "testnet-11" => palw_rc_shipped_params(),
        "devnet" => devnet_shipped_params(),
        "mainnet" => mainnet_shipped_params(),
        other => panic!("no such preset {other}"),
    }
}

/// Armed on testnet-12 from genesis, with the cursor it refines; dormant everywhere else.
#[test]
fn the_clock_floor_is_armed_on_testnet12_only() {
    let t12 = palw_t12_shipped_params();
    assert_eq!(t12.palw_clock_floor, Some(ForkActivation::always()), "testnet-12 arms the floor from genesis");
    assert_eq!(t12.palw_clock_cursor, Some(ForkActivation::always()), "…at or above the cursor it refines");
    t12.validate_palw_v2().expect("testnet-12 validates with the floor");
    for (name, _, _, _) in BEFORE_THE_FLOOR {
        assert_eq!(shipped(name).palw_clock_floor, None, "{name}: the floor must be dormant");
    }
}

/// **testnet-11, devnet and mainnet are byte-identical to the build before the floor** — the ruleset
/// a node announces, the identity two nodes must share to peer, and the schedule the operator log
/// names.
#[test]
fn testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_floor() {
    let mut moved = Vec::new();
    for (name, params_id, identity_id, schedule_id) in BEFORE_THE_FLOOR {
        let p = shipped(name);
        let now = (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string());
        println!("{name}: params {} identity {} schedule {}", now.0, now.1, now.2);
        if (now.0.as_str(), now.1.as_str(), now.2.as_str()) != (*params_id, *identity_id, *schedule_id) {
            moved.push(format!("{name}: {now:?}"));
        }
    }
    assert!(moved.is_empty(), "a preset that does not arm the floor moved: {moved:?}");
}

/// The floor is in testnet-12's fingerprint, and a floor refused without its cursor.
#[test]
fn the_floor_moves_testnet12s_fingerprint_and_needs_the_cursor() {
    let t12 = palw_t12_shipped_params();
    let mut without = t12.clone();
    without.palw_clock_floor = None;
    assert_ne!(t12.consensus_params_id(), without.consensus_params_id(), "the ruleset a node announces names the floor");
    assert_ne!(t12.consensus_schedule_id(), without.consensus_schedule_id(), "and so does the schedule the operator log names");
    println!("testnet-12 params id with the floor {} / without {}", t12.consensus_params_id(), without.consensus_params_id());

    let mut orphaned = t12.clone();
    orphaned.palw_clock_cursor = None;
    orphaned.palw_anchor_clock = None;
    orphaned.set_palw_single_lottery(None);
    let refusal = orphaned.validate_palw_v2().expect_err("a floor without a cursor has no slot to bound");
    assert!(format!("{refusal:?}").contains("palw_clock_floor"), "{refusal:?}");
}
