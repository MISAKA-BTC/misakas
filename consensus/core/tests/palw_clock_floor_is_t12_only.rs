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
const BEFORE_THE_FLOOR: &[(&str, &str, &str, &str)] = &[
    (
        "testnet-11",
        "c99bb4f43891dc637e4d5634816c46b33d89f07a381875e2ce54bd3ef80ac74a",
        "19dbdbb8afd374aafa14f7fd7457fac7304a19df697706f69b34be0e1e995d4e",
        "5a1d8d5679e0e8d7e9022255668fd5d4b3e4c8a6c367acf3882c6a3d480d8b64",
    ),
    (
        "devnet",
        "9acd42be5357a25ee08c1c7037d1610ef00107e8bd47eb59e6c6a6f91c31f502",
        "9acd42be5357a25ee08c1c7037d1610ef00107e8bd47eb59e6c6a6f91c31f502",
        "edd80c01c791d225d602b9136f539f4dfeb506ba1b3071b177b0d873a661142f",
    ),
    (
        "mainnet",
        "badaa8e90f14ef0074048d6b18660864855be8ab854d0ecb01dfbb62171538e1",
        "00d98599bd45867f4a7b3bef1043431ccb8ef0d41cedbc552a87926b5e3b8af5",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
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
