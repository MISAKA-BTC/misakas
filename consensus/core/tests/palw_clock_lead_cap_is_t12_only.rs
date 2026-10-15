//! **The lead cap (the 2026-09-25 mainnet-values review's HIGH, the user's option (a)) is armed where
//! the 1,620 s tolerance meets the heartbeat clock — testnet-12 and a mainnet card — and nowhere else.**
//!
//! `Params::palw_clock_lead_cap` changes which heartbeat headers and which clock steps a node admits
//! now (at most 132 s past its own clock), so it is a consensus fence. It is `Some(0)` on testnet-12,
//! stated `always()` by a mainnet card beside the tolerance it caps (`mainnet_card_base_v1`; checked
//! in `config::params`' own tests, where the card assembly lives), and `None` everywhere else. It is
//! hashed Some-only in every writer — `consensus_params_id`, `consensus_schedule_id` and the
//! `for_each_fence` walk under both — so testnet-11, devnet, mainnet as shipped (the bundle-free
//! preset while no card is pinned), testnet-10 and simnet must fingerprint byte-identically to the
//! build before the field existed: `befb59dfe` (the mainnet-values commit this change was built on),
//! whose numbers these are.

use kaspa_consensus_core::config::params::{
    ForkActivation, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_shipped_params,
};

/// `(network, consensus_params_id, consensus_identity_id, consensus_schedule_id)` at `befb59dfe`.
/// testnet-11, devnet and mainnet are `palw_clock_floor_is_t12_only`'s pins, unmoved; testnet-10 and
/// simnet below.
const BEFORE_THE_CAP: &[(&str, &str, &str, &str)] = &[
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
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was eb866c61…, 7819e5ed…, a1ed7ff0…)
        "mainnet",
        "0befd2fa1da052fb88ef8a04cc1bd6412ea560371aaec606790194dee41ac3fa",
        "db7f8f9869a612c6e90ff8a01806569a0fe7391cfa8226c3c3a89680ae83d277",
        "7b8f4090c31d524312f1b6d8a91d0e47544b031a62560d90b9a1bc65b709e7c6",
    ),
    ("testnet-10", TESTNET_10_IDS.0, TESTNET_10_IDS.1, TESTNET_10_IDS.2),
    ("simnet", SIMNET_IDS.0, SIMNET_IDS.1, SIMNET_IDS.2),
];

/// testnet-10 and simnet, materialized. The params ids are `shipped_presets_have_pinned_fingerprints`'
/// pins at `befb59dfe` (`0d9cf361…`, `63238ba1…`), so the ruleset is unmoved; the identity and schedule
/// ids were taken from this file's own output on this tree, where both presets leave the cap `None`
/// and a Some-only writer writes nothing for it.
const TESTNET_10_IDS: (&str, &str, &str) = (
    "a4153f537905eebd364d43742ccfc6e7264149cce61392f827dc0d45aee4d08d",
    "c9adc147ada258574c0926534fae5dd4eb82b5a2795bdc5300affd74abfe0380",
    "e86052cdfea252fde7c2277087aeda5566c41e9161f083705e99bf3180653ff6",
);
const SIMNET_IDS: (&str, &str, &str) = (
    "5fca6e0c631ea9327fc75e1bd5463b2d78814c3a9937f2d6cea282bdfc05ef56",
    "5fca6e0c631ea9327fc75e1bd5463b2d78814c3a9937f2d6cea282bdfc05ef56",
    "04ccb8fe229ae341d1c2b171b4bd7a578fcc326c3ecbf5f017ccaee878f3aee5",
);

/// testnet-12 at `befb59dfe` (the mainnet-values commit's own pins: `1870bc1f…` / `6e193d30…` /
/// `79563f51…`) — what testnet-12 is with the cap taken away: the cap is the only thing that moved it.
// re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was a9280987…, 50990e0f…, 250fe032…)
const T12_BEFORE_THE_CAP: (&str, &str, &str) = (
    "20206cf2e283e2bf1859d817196a93a8ff6e85e45ce01faa5302fb9ad9b142ad",
    "73312ebf4fa0fd951fff099bc9be1760a21e30ac00c92ee62225e7fa0404aded",
    "6a29a90722344aad14adea9b13e92736e5e16a1c2ea1f3b785a28a649249c50a",
);

/// testnet-12 as this change ships it — the ids a testnet-12 node on this build announces.
// re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 5ee7fd8e…, 5de80e64…, 1678e073…)
const T12_WITH_THE_CAP: (&str, &str, &str) = (
    "59a6d1b00434b42129201098fafb2c889a27d6392a87e531df6e1409beeb6ce7",
    "92042711856b239713d0376b7466422e561618fc51e115e1557be34b078e5acd",
    "bc0600cc47b9433b99b715e902e3892e86295460b98cc1893c4fccc26f2b780c",
);

fn shipped(name: &str) -> Params {
    match name {
        "testnet-11" => palw_rc_shipped_params(),
        "devnet" => devnet_shipped_params(),
        "mainnet" => mainnet_shipped_params(),
        // MATERIALIZED, as `shipped_presets_have_pinned_fingerprints` pins them: the value a node reports.
        "testnet-10" => Params::from(TESTNET_PARAMS.net),
        "simnet" => Params::from(SIMNET_PARAMS.net),
        other => panic!("no such preset {other}"),
    }
}

/// Armed on testnet-12 from genesis, beside the tolerance it caps; dormant everywhere else.
#[test]
fn the_lead_cap_is_armed_on_testnet12_only() {
    let t12 = palw_t12_shipped_params();
    assert_eq!(t12.palw_clock_lead_cap, Some(ForkActivation::always()), "testnet-12 arms the cap from genesis");
    assert_eq!(t12.timestamp_deviation_tolerance, 1_620, "…because it runs mainnet's 1,620 s tolerance");
    t12.validate_palw_v2().expect("testnet-12 validates with the cap");
    for (name, _, _, _) in BEFORE_THE_CAP {
        assert_eq!(shipped(name).palw_clock_lead_cap, None, "{name}: the cap must be dormant");
    }
}

/// **testnet-11, devnet, mainnet as shipped, testnet-10 and simnet are byte-identical to the build
/// before the cap** — the ruleset a node announces, the identity two nodes must share to peer, and
/// the schedule the operator log names.
#[test]
fn every_other_preset_fingerprints_as_it_did_before_the_cap() {
    let mut moved = Vec::new();
    for (name, params_id, identity_id, schedule_id) in BEFORE_THE_CAP {
        let p = shipped(name);
        let now = (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string());
        println!("{name}: params {} identity {} schedule {}", now.0, now.1, now.2);
        if (now.0.as_str(), now.1.as_str(), now.2.as_str()) != (*params_id, *identity_id, *schedule_id) {
            moved.push(format!("{name}: {now:?}"));
        }
    }
    assert!(moved.is_empty(), "a preset that does not arm the cap moved: {moved:?}");
}

/// **The cap is in testnet-12's fingerprint — all three ids — and it is the only thing that moved
/// it**; and a `Some(never())` collapses to `None`, so a normalised dormant cap writes nothing a build
/// without it would not.
#[test]
fn the_cap_moves_testnet12s_fingerprint_and_a_never_collapses() {
    let t12 = palw_t12_shipped_params();
    let mut without = t12.clone();
    without.palw_clock_lead_cap = None;
    assert_ne!(t12.consensus_params_id(), without.consensus_params_id(), "the ruleset a node announces names the cap");
    assert_ne!(t12.consensus_identity_id(), without.consensus_identity_id(), "in force from block one: two identities");
    assert_ne!(t12.consensus_schedule_id(), without.consensus_schedule_id(), "and the schedule the operator log names");
    let ids = |p: &Params| {
        (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
    };
    println!("testnet-12 with the cap: {:?}", ids(&t12));
    println!("testnet-12 without it:   {:?}", ids(&without));
    let (w, n) = (ids(&t12), ids(&without));
    assert_eq!(
        (n.0.as_str(), n.1.as_str(), n.2.as_str()),
        T12_BEFORE_THE_CAP,
        "take the cap away and testnet-12 is befb59dfe's, to the id"
    );
    assert_eq!((w.0.as_str(), w.1.as_str(), w.2.as_str()), T12_WITH_THE_CAP);
    // int-12: the Useful Work Transition's tick rides the lead cap, so a ruleset without the cap validates only without the tick too.
    let mut validated = without.clone();
    (kaspa_consensus_core::palw_real_share_v1::PALW_T12_REAL_CLOCK_TICK_ENTRY.set)(&mut validated, None);
    validated.validate_palw_v2().expect("the cap is independent: testnet-12 without it is the ruleset it was");

    // The identity two nodes compare normalizes `Some(never())` — and a height not yet reached — to
    // absence (the collapse in `normalize_values_a_scheduled_fence_drags_with_it`), so a node that
    // wrote either still peers with testnet-11 as shipped.
    let rc = palw_rc_shipped_params();
    for dormant in [ForkActivation::never(), ForkActivation::new(1_000_000)] {
        let mut p = rc.clone();
        p.palw_clock_lead_cap = Some(dormant);
        assert_eq!(p.consensus_identity_id(), rc.consensus_identity_id(), "{dormant:?} is absence in the identity");
    }
}
