//! **ADR-0152 v2 F2's offence-attribution fence is testnet-12's alone.**
//!
//! `Params::palw_offence_attribution` changes which objective offences a block may carry and what
//! one convicts — past it the V1 `PanelFalseValid` is refused and `PanelFalseValidV2` is judged by
//! one adjudicator bound to the claim's committed root — so it is a consensus fence. It is `Some(0)`
//! on testnet-12 and `None` everywhere else, and it is hashed Some-only in every writer
//! (`consensus_params_id`, `consensus_schedule_id` and the `for_each_fence` walk under both), so
//! testnet-11, devnet and mainnet must fingerprint byte-identically to the build before the field
//! existed. Those numbers are `palw_clock_floor_is_t12_only`'s `BEFORE_THE_FLOOR`, which the parent
//! of the fence (`5bf72b46`) still produces; testnet-12's own numbers WITHOUT the fence are pinned to
//! that parent too, by running this file against it — so the fence is the only thing that moved
//! testnet-12, and nothing moved anything else.

use kaspa_consensus_core::config::params::{
    ForkActivation, Params, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_shipped_params,
};

/// `(network, consensus_params_id, consensus_identity_id, consensus_schedule_id)` of the presets
/// that do not arm the fence, at `5bf72b46` (= `BEFORE_THE_FLOOR`, whose numbers that commit
/// still produces).
const BEFORE_THE_ATTRIBUTION: &[(&str, &str, &str, &str)] = &[
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

/// testnet-12's `(consensus_params_id, consensus_identity_id, consensus_schedule_id)` at `5bf72b46`,
/// the build before the fence. testnet-12 with the fence taken away must still be exactly this.
const T12_BEFORE_THE_ATTRIBUTION: (&str, &str, &str) = (
    "39d512cf32f0d88a8ae5446d6c31ba6f49d35a7d20cad1679f0399b0433bbab9",
    "f7db37bfa2a8775f36c4301853f1786dc0ca41bc24fc682de198eb4bb869db8e",
    "b16bf05ab109d608e05432e43e66969b6fad7ee7ac5b4e6703a9b0c64b14213a",
);

fn shipped(name: &str) -> Params {
    match name {
        "testnet-11" => palw_rc_shipped_params(),
        "devnet" => devnet_shipped_params(),
        "mainnet" => mainnet_shipped_params(),
        other => panic!("no such preset {other}"),
    }
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

/// Armed on testnet-12 from genesis, over the four fences it convicts through; dormant everywhere
/// else, at every height.
#[test]
fn the_attribution_fence_is_armed_on_testnet12_only() {
    let t12 = palw_t12_shipped_params();
    assert_eq!(t12.palw_offence_attribution, Some(ForkActivation::always()), "testnet-12 arms it from genesis");
    assert!(t12.palw_offence_attribution_active_at(0), "in force from the first block");
    for (name, fence) in [
        ("palw_objective_offence", t12.palw_objective_offence),
        ("palw_audit_2026_09_23", t12.palw_audit_2026_09_23),
        ("palw_verification_v2", t12.palw_verification_v2),
        ("palw_economic_safety", t12.palw_economic_safety),
    ] {
        assert_eq!(fence, Some(ForkActivation::always()), "…over {name}, armed from genesis too");
    }
    t12.validate_palw_v2().expect("testnet-12 validates with the fence");
    for (name, _, _, _) in BEFORE_THE_ATTRIBUTION {
        let p = shipped(name);
        assert_eq!(p.palw_offence_attribution, None, "{name}: the fence must be dormant");
        assert_eq!(p.palw_offence_attribution_fence(), None, "{name}: and resolve dormant");
        for daa in [0, 8_500, u64::MAX] {
            assert!(!p.palw_offence_attribution_active_at(daa), "{name}: never in force (DAA {daa})");
        }
    }
}

/// **testnet-11, devnet and mainnet are byte-identical to the build before the fence** — the ruleset
/// a node announces, the identity two nodes must share to peer, and the schedule the operator log
/// names.
#[test]
fn testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_attribution_fence() {
    let mut moved = Vec::new();
    for (name, params_id, identity_id, schedule_id) in BEFORE_THE_ATTRIBUTION {
        let now = ids(&shipped(name));
        println!("{name}: params {} identity {} schedule {}", now.0, now.1, now.2);
        if (now.0.as_str(), now.1.as_str(), now.2.as_str()) != (*params_id, *identity_id, *schedule_id) {
            moved.push(format!("{name}: {now:?}"));
        }
    }
    assert!(moved.is_empty(), "a preset that does not arm the attribution fence moved: {moved:?}");
}

/// **The fence is in testnet-12's fingerprint, and it is the only thing that moved it.** With the
/// field taken away testnet-12 fingerprints exactly as it did at the parent; with it, its ruleset,
/// identity and schedule all name it.
#[test]
fn the_fence_moves_testnet12s_fingerprint_and_nothing_else_did() {
    let t12 = palw_t12_shipped_params();
    let mut without = t12.clone();
    without.palw_offence_attribution = None;
    let before = ids(&without);
    println!("testnet-12 without the fence: params {} identity {} schedule {}", before.0, before.1, before.2);
    println!("testnet-12 with the fence:    params {} identity {} schedule {}", ids(&t12).0, ids(&t12).1, ids(&t12).2);
    assert_eq!(
        (before.0.as_str(), before.1.as_str(), before.2.as_str()),
        T12_BEFORE_THE_ATTRIBUTION,
        "testnet-12 without the fence is testnet-12 at the parent"
    );
    assert_ne!(t12.consensus_params_id(), without.consensus_params_id(), "the ruleset a node announces names the fence");
    assert_ne!(t12.consensus_identity_id(), without.consensus_identity_id(), "in force from block one: two identities");
    assert_ne!(t12.consensus_schedule_id(), without.consensus_schedule_id(), "and the schedule the operator log names it");
}

/// **Genesis or not at all**: a fence at any later height is refused by `validate_palw_v2`, and
/// `never()` is absence.
#[test]
fn a_later_height_is_refused_by_validation() {
    let t12 = palw_t12_shipped_params();
    for height in [1u64, 1_000, 8_500] {
        let mut late = t12.clone();
        late.palw_offence_attribution = Some(ForkActivation::new(height));
        let refusal = late.validate_palw_v2().expect_err("a later crossing keys one false Valid under two ledgers");
        assert!(format!("{refusal:?}").contains("palw_offence_attribution may only be armed at genesis"), "{height}: {refusal:?}");
    }
    let mut never = t12.clone();
    never.palw_offence_attribution = Some(ForkActivation::never());
    never.validate_palw_v2().expect("never() is absence, and absence validates");
}
