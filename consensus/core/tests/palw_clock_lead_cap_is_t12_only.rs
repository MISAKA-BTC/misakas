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
        "testnet-11",
        "bd633ce933974d4134676efbdaf46b269dc2fb78f007e0907479aabd4d743f29",
        "44cb8fd729e9575a6e3b1e72c466b8abce4b9ecd81bb556685c9ba225487117f",
        "5a1d8d5679e0e8d7e9022255668fd5d4b3e4c8a6c367acf3882c6a3d480d8b64",
    ),
    (
        "devnet",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "edd80c01c791d225d602b9136f539f4dfeb506ba1b3071b177b0d873a661142f",
    ),
    (
        "mainnet",
        "eb866c61ca1a8ab58108be6cd1f39f951b582123472545575a5c7dbe0f1e5aa5",
        "7819e5ed2b3df50b3303df3df2f0fec7677ddcb37ed55f1d43455a37ecd9c9a8",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
    ),
    ("testnet-10", TESTNET_10_IDS.0, TESTNET_10_IDS.1, TESTNET_10_IDS.2),
    ("simnet", SIMNET_IDS.0, SIMNET_IDS.1, SIMNET_IDS.2),
];

/// testnet-10 and simnet, materialized. The params ids are `shipped_presets_have_pinned_fingerprints`'
/// pins at `befb59dfe` (`0d9cf361…`, `63238ba1…`), so the ruleset is unmoved; the identity and schedule
/// ids were taken from this file's own output on this tree, where both presets leave the cap `None`
/// and a Some-only writer writes nothing for it.
const TESTNET_10_IDS: (&str, &str, &str) = (
    "0d9cf361e02dea6d9e873014ff5e414c2e8e6879e8d705cde6c924a3a3f8dd88",
    "2c3067c01e76ac32f0bd3f78ba49cc25f2ea771af4eb848e5aaad17f825a0a64",
    "7ea3296f36fc827898aa6560a1f71159652a12a7dd69105178564b9f7723d5d0",
);
const SIMNET_IDS: (&str, &str, &str) = (
    "63238ba10766c824ff6915484829b01eb4fc3c105665a7db2cf6b175bf870dfd",
    "63238ba10766c824ff6915484829b01eb4fc3c105665a7db2cf6b175bf870dfd",
    "f981edc9bff1b71ae46abf030c0c56c40beafabeeae78d8435dd502ad6191f69",
);

/// testnet-12 at `befb59dfe` (the mainnet-values commit's own pins: `1870bc1f…` / `6e193d30…` /
/// `79563f51…`) — what testnet-12 is with the cap taken away: the cap is the only thing that moved it.
// re-pin 2026-09-25 @57c1fe323c44: t12 shipping re-pin 2026-09-25 (rcore/int-3 57c1fe32): mainnet values (λ 5, 1,620 s tolerance, level 225; mainnet takes t12's DNS set, carve, Decision A), the beat lead cap (132 s past the receiver clock), the execution-quantum maturity 120 DAA, the readiness-memory / duties / mempool node fixes; T41 v22 golden moves because R1 (palw_activation_pool) excludes genesis rows from silence reclamation. Genesis a27f8f44 and premine txid 5e0d5f1b unchanged. (was 1870bc1f…, 6e193d30…, 79563f51…)
const T12_BEFORE_THE_CAP: (&str, &str, &str) = (
    "730d7f10dc1416980ff88c9825a5279e4e7fd64f8f475f11b4e13d572ba17e47",
    "50990e0f0b634856aeae49eb38ffa38cfa991571be196d27c17befaefe805ec2",
    "01854a388d6e40393e6752261076eab7c1b0f11f62d4ec92bf356b7c8e977de6",
);

/// testnet-12 as this change ships it — the ids a testnet-12 node on this build announces.
// re-pin 2026-09-25 @57c1fe323c44: t12 shipping re-pin 2026-09-25 (rcore/int-3 57c1fe32): mainnet values (λ 5, 1,620 s tolerance, level 225; mainnet takes t12's DNS set, carve, Decision A), the beat lead cap (132 s past the receiver clock), the execution-quantum maturity 120 DAA, the readiness-memory / duties / mempool node fixes; T41 v22 golden moves because R1 (palw_activation_pool) excludes genesis rows from silence reclamation. Genesis a27f8f44 and premine txid 5e0d5f1b unchanged. (was b688f0d1…, ade04862…, 3f5180c0…)
const T12_WITH_THE_CAP: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
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
    without.validate_palw_v2().expect("the cap is independent: testnet-12 without it is the ruleset it was");

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
