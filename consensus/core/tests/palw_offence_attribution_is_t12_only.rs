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
/// still produces) — **re-pinned once for ADR-0152's v22 skeleton**, whose version bump moves
/// testnet-11's and devnet's params and identity ids and nothing else (see `BEFORE_THE_FLOOR`).
const BEFORE_THE_ATTRIBUTION: &[(&str, &str, &str, &str)] = &[
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
        "badaa8e90f14ef0074048d6b18660864855be8ab854d0ecb01dfbb62171538e1",
        "00d98599bd45867f4a7b3bef1043431ccb8ef0d41cedbc552a87926b5e3b8af5",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
    ),
];

/// testnet-12's `(consensus_params_id, consensus_identity_id, consensus_schedule_id)` at `5bf72b46`,
/// the build before the fence. testnet-12 with the fence taken away must still be exactly this.
///
/// **Re-pinned once for ADR-0152's v22 skeleton**, which moved testnet-12 itself: the version, the
/// `palw_rcore_plus` fence and C7's list, and the bundle's `COMPLETE_V5` context root are all in its
/// ids. The value is testnet-12 at v22 with this fence alone taken away (R-core+ still armed), so the
/// property is unchanged: this fence is exactly the difference. v21: `39d512cf…` / `f7db37bf…` /
/// `b16bf05a…`.
///
/// **Re-pinned once more by M3** (ADR-0152 DA-4, IMPL-7): the DA-disclosure-v4 context closes
/// `COMPLETE_V5`, so the bundle's context root, and with it testnet-12's params and identity ids,
/// moved; the schedule did not (no fence moved). v22 skeleton: `cf57a2e9…` / `2b48d4ee…`.
///
/// **Re-pinned once more by ADR-0152 §4-quater**: `palw_class_verify_deadline` is armed on
/// testnet-12 from genesis and named (Some-only) in all three ids, so testnet-12 with this fence taken
/// away moved with it. The value is testnet-12 with this fence alone taken away (the deadline fence
/// still armed); with BOTH taken away testnet-12 is exactly the M3 value below
/// ([`T12_BEFORE_THE_ATTRIBUTION_AND_THE_DEADLINE`]), which pins that the deadline fence is the only
/// thing that moved it.
const T12_BEFORE_THE_ATTRIBUTION: (&str, &str, &str) = (
    "8c1b6da8d04f7f0f5b74906bdb2c5890845c0dcc2dbb6d0ddef84677d43df47b",
    "e14dce009fcec4463be7945aca4b2fd6f32f6afda16bd8ab47f7c6c8702582b6",
    "7d9a75c47eb74812efbfa12a937736f108fb46c2cac9152ad926088e1671c866",
);

/// testnet-12 with this fence AND `palw_class_verify_deadline` taken away — the M3 value of
/// [`T12_BEFORE_THE_ATTRIBUTION`], unchanged: the class-verify-deadline fence moved nothing else.
const T12_BEFORE_THE_ATTRIBUTION_AND_THE_DEADLINE: (&str, &str, &str) = (
    "f5eee4746137d0e40a58fd5e99bfdc47738800550ad313811be7b1eed185821b",
    "75666d42d34312c76691de9d1f876465e58502e48d38bde1aa3d7cce0906e7c9",
    "75282ad229d5f8617aa5b3f5b818ddab9defdc893f1276c6a079c080af7debff",
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
    let mut without_both = without.clone();
    without_both.palw_class_verify_deadline = None;
    without_both.sync_palw_class_verify_deadline();
    let both = ids(&without_both);
    assert_eq!(
        (both.0.as_str(), both.1.as_str(), both.2.as_str()),
        T12_BEFORE_THE_ATTRIBUTION_AND_THE_DEADLINE,
        "with the deadline fence taken away too, testnet-12 is the M3 parent: that fence moved nothing else"
    );
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
    // ADR-0152: R-core+ is armed above this fence on testnet-12 and refuses to stand without it, so
    // the absence that validates is the fence's with R-core+ taken off too.
    never.palw_rcore_plus = None;
    never.palw_rcore_conservative_classes = &[];
    never.sync_palw_rcore_plus();
    never.validate_palw_v2().expect("never() is absence, and absence validates");
}
